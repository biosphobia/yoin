//! Yoin's audio engine.
//!
//! One mono track: record from the input device, play back through the output
//! device. The audio callbacks never block: they talk to the rest of the app
//! through atomics, a lock-free ring buffer (input) and a `try_read` on the take
//! (output). Everything that can block lives on the control side.

mod devices;
mod take;

pub use devices::{list_devices, DeviceInfo, DeviceList};
pub use take::Take;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Stopped = 0,
    Playing = 1,
    Recording = 2,
}

impl Transport {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Transport::Playing,
            2 => Transport::Recording,
            _ => Transport::Stopped,
        }
    }
}

/// A snapshot of the engine for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub transport: Transport,
    /// Playhead in seconds (while recording: how much has been recorded).
    pub position_secs: f64,
    /// Length of the take in seconds.
    pub length_secs: f64,
    /// Peak input level since the last status call, 0..1.
    pub input_peak: f32,
    /// Peak output level since the last status call, 0..1.
    pub output_peak: f32,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: u32,
    /// Last stream error reported by the audio driver, if any.
    pub error: Option<String>,
}

/// State shared between the control side and the audio callbacks.
struct Shared {
    transport: AtomicU8,
    /// Playhead, in take samples, stored as `f64` bits.
    play_pos: AtomicU64,
    take: RwLock<Take>,
    take_len: AtomicU64,
    take_rate: AtomicU32,
    input_rate: AtomicU32,
    output_rate: AtomicU32,
    /// Peak levels as `f32` bits. Non-negative floats order the same as their bits,
    /// so `fetch_max` on the bits is a max on the values.
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    /// Consumer side of the input ring buffer. Only the recorder thread and
    /// `stop()` touch it, never an audio callback.
    recorded: Mutex<Option<rtrb::Consumer<f32>>>,
    last_error: Mutex<Option<String>>,
    shutdown: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            transport: AtomicU8::new(Transport::Stopped as u8),
            play_pos: AtomicU64::new(0f64.to_bits()),
            take: RwLock::new(Take::new(48_000)),
            take_len: AtomicU64::new(0),
            take_rate: AtomicU32::new(48_000),
            input_rate: AtomicU32::new(0),
            output_rate: AtomicU32::new(0),
            input_peak: AtomicU32::new(0),
            output_peak: AtomicU32::new(0),
            recorded: Mutex::new(None),
            last_error: Mutex::new(None),
            shutdown: AtomicBool::new(false),
        }
    }

    fn transport(&self) -> Transport {
        Transport::from_u8(self.transport.load(Ordering::Acquire))
    }

    fn set_transport(&self, t: Transport) {
        self.transport.store(t as u8, Ordering::Release);
    }

    fn pos(&self) -> f64 {
        f64::from_bits(self.play_pos.load(Ordering::Acquire))
    }

    fn set_pos(&self, p: f64) {
        self.play_pos.store(p.to_bits(), Ordering::Release);
    }

    fn report_error(&self, e: impl ToString) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = Some(e.to_string());
        }
    }

    /// Moves everything waiting in the input ring into the take.
    /// `keep` false discards it (used when not recording).
    fn drain_recorded(&self, keep: bool) {
        let mut guard = match self.recorded.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let Some(consumer) = guard.as_mut() else {
            return;
        };
        let n = consumer.slots();
        if n == 0 {
            return;
        }
        let Ok(chunk) = consumer.read_chunk(n) else {
            return;
        };
        if keep {
            let (a, b) = chunk.as_slices();
            if let Ok(mut take) = self.take.write() {
                take.samples.extend_from_slice(a);
                take.samples.extend_from_slice(b);
                self.take_len
                    .store(take.samples.len() as u64, Ordering::Release);
            }
        }
        chunk.commit_all();
    }
}

/// Owns the device streams. Streams are created and dropped on their own thread
/// because some backends don't allow moving them between threads.
struct AudioThread {
    stop_tx: mpsc::Sender<()>,
    stream_thread: Option<JoinHandle<()>>,
    recorder_thread: Option<JoinHandle<()>>,
    input_name: Option<String>,
    output_name: Option<String>,
}

impl AudioThread {
    fn shut_down(mut self, shared: &Shared) {
        let _ = self.stop_tx.send(());
        if let Some(h) = self.stream_thread.take() {
            let _ = h.join();
        }
        shared.shutdown.store(true, Ordering::Release);
        if let Some(h) = self.recorder_thread.take() {
            let _ = h.join();
        }
        shared.shutdown.store(false, Ordering::Release);
    }
}

pub struct Engine {
    shared: Arc<Shared>,
    audio: Option<AudioThread>,
}

impl Engine {
    /// Opens the given devices (by id from [`list_devices`]), or the system
    /// defaults when `None`. A missing input device is not an error: playback
    /// still works, recording reports that there is no input.
    pub fn new(input_id: Option<&str>, output_id: Option<&str>) -> Result<Self, String> {
        let mut engine = Self {
            shared: Arc::new(Shared::new()),
            audio: None,
        };
        engine.set_devices(input_id, output_id)?;
        Ok(engine)
    }

    /// Reopens audio on other devices. Stops recording or playback first.
    pub fn set_devices(
        &mut self,
        input_id: Option<&str>,
        output_id: Option<&str>,
    ) -> Result<(), String> {
        self.stop();
        if let Some(audio) = self.audio.take() {
            audio.shut_down(&self.shared);
        }
        *self.shared.last_error.lock().map_err(|e| e.to_string())? = None;

        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let shared = self.shared.clone();
        let input_id = input_id.map(str::to_owned);
        let output_id = output_id.map(str::to_owned);

        let stream_thread = std::thread::Builder::new()
            .name("yoin-audio".into())
            .spawn(move || {
                match open_streams(&shared, input_id.as_deref(), output_id.as_deref()) {
                    Ok((streams, names)) => {
                        let _ = ready_tx.send(Ok(names));
                        // Keep the streams alive until asked to stop.
                        let _ = stop_rx.recv();
                        drop(streams);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        let (input_name, output_name) = match ready_rx.recv() {
            Ok(Ok(names)) => names,
            Ok(Err(e)) => {
                let _ = stream_thread.join();
                return Err(e);
            }
            Err(_) => {
                let _ = stream_thread.join();
                return Err("audio thread exited unexpectedly".into());
            }
        };

        let shared = self.shared.clone();
        let recorder_thread = std::thread::Builder::new()
            .name("yoin-recorder".into())
            .spawn(move || {
                while !shared.shutdown.load(Ordering::Acquire) {
                    let keep = shared.transport() == Transport::Recording;
                    shared.drain_recorded(keep);
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
            .map_err(|e| e.to_string())?;

        self.audio = Some(AudioThread {
            stop_tx,
            stream_thread: Some(stream_thread),
            recorder_thread: Some(recorder_thread),
            input_name,
            output_name,
        });
        Ok(())
    }

    /// Starts a new take, replacing the old one.
    pub fn record(&mut self) -> Result<(), String> {
        let has_input = self
            .audio
            .as_ref()
            .map(|a| a.input_name.is_some())
            .unwrap_or(false);
        if !has_input {
            return Err("No input device. Plug in a microphone or pick one in Settings.".into());
        }
        self.stop();
        // Throw away anything captured before the button was pressed.
        self.shared.drain_recorded(false);
        let rate = self.shared.input_rate.load(Ordering::Acquire);
        {
            let mut take = self.shared.take.write().map_err(|e| e.to_string())?;
            *take = Take::new(rate);
        }
        self.shared.take_len.store(0, Ordering::Release);
        self.shared.take_rate.store(rate, Ordering::Release);
        self.shared.set_pos(0.0);
        self.shared.set_transport(Transport::Recording);
        Ok(())
    }

    /// Plays the take from the playhead.
    pub fn play(&mut self) -> Result<(), String> {
        match self.shared.transport() {
            Transport::Playing => return Ok(()),
            Transport::Recording => self.stop(),
            Transport::Stopped => {}
        }
        let len = self.shared.take_len.load(Ordering::Acquire) as f64;
        if len == 0.0 {
            return Err("Nothing recorded yet.".into());
        }
        if self.shared.pos() >= len {
            self.shared.set_pos(0.0);
        }
        self.shared.set_transport(Transport::Playing);
        Ok(())
    }

    /// Stops playback or recording. Stopping while already stopped rewinds to the start.
    pub fn stop(&mut self) {
        match self.shared.transport() {
            Transport::Recording => {
                self.shared.set_transport(Transport::Stopped);
                // Keep the tail that hadn't been moved into the take yet.
                self.shared.drain_recorded(true);
                self.shared.set_pos(0.0);
            }
            Transport::Playing => self.shared.set_transport(Transport::Stopped),
            Transport::Stopped => self.shared.set_pos(0.0),
        }
    }

    /// Moves the playhead. Ignored while recording.
    pub fn seek(&mut self, secs: f64) {
        if self.shared.transport() == Transport::Recording {
            return;
        }
        let rate = self.shared.take_rate.load(Ordering::Acquire) as f64;
        let len = self.shared.take_len.load(Ordering::Acquire) as f64;
        self.shared.set_pos((secs.max(0.0) * rate).min(len));
    }

    pub fn status(&self) -> Status {
        let shared = &self.shared;
        let rate = shared.take_rate.load(Ordering::Acquire).max(1) as f64;
        let len = shared.take_len.load(Ordering::Acquire) as f64;
        let transport = shared.transport();
        let position = if transport == Transport::Recording {
            len
        } else {
            shared.pos()
        };
        Status {
            transport,
            position_secs: position / rate,
            length_secs: len / rate,
            input_peak: f32::from_bits(shared.input_peak.swap(0, Ordering::AcqRel)),
            output_peak: f32::from_bits(shared.output_peak.swap(0, Ordering::AcqRel)),
            input_device: self.audio.as_ref().and_then(|a| a.input_name.clone()),
            output_device: self.audio.as_ref().and_then(|a| a.output_name.clone()),
            sample_rate: shared.output_rate.load(Ordering::Acquire),
            error: shared.last_error.lock().ok().and_then(|e| e.clone()),
        }
    }

    /// Waveform overview of the take: `bins` peak values in 0..1.
    pub fn peaks(&self, bins: usize) -> Vec<f32> {
        self.shared
            .take
            .read()
            .map(|t| t.peaks(bins))
            .unwrap_or_default()
    }

    /// Saves the take as a WAV file.
    pub fn export_wav(&self, path: &Path) -> Result<(), String> {
        let take = self.shared.take.read().map_err(|e| e.to_string())?;
        if take.is_empty() {
            return Err("Nothing recorded yet.".into());
        }
        take.write_wav(path).map_err(|e| e.to_string())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(audio) = self.audio.take() {
            audio.shut_down(&self.shared);
        }
    }
}

type Names = (Option<String>, Option<String>);

/// Opens the output stream (required) and the input stream (optional).
fn open_streams(
    shared: &Arc<Shared>,
    input_id: Option<&str>,
    output_id: Option<&str>,
) -> Result<(Vec<cpal::Stream>, Names), String> {
    let host = cpal::default_host();
    let mut streams = Vec::new();

    let output = devices::find_device(&host, output_id, false)
        .ok_or("No output device found. Check that your speakers or headphones are connected.")?;
    let output_name = devices::device_name(&output);
    let out_stream = build_output(&output, shared.clone())?;
    out_stream
        .play()
        .map_err(|e| format!("Could not start output: {e}"))?;
    streams.push(out_stream);

    let mut input_name = None;
    if let Some(input) = devices::find_device(&host, input_id, true) {
        match build_input(&input, shared.clone()) {
            Ok(stream) => match stream.play() {
                Ok(()) => {
                    input_name = Some(devices::device_name(&input));
                    streams.push(stream);
                }
                Err(e) => shared.report_error(format!("Could not start input: {e}")),
            },
            Err(e) => shared.report_error(e),
        }
    }

    Ok((streams, (input_name, Some(output_name))))
}

fn build_output(device: &cpal::Device, shared: Arc<Shared>) -> Result<cpal::Stream, String> {
    let supported = device
        .default_output_config()
        .map_err(|e| format!("Output device has no usable format: {e}"))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    shared
        .output_rate
        .store(config.sample_rate, Ordering::Release);
    match format {
        SampleFormat::F32 => output_stream::<f32>(device, config, shared),
        SampleFormat::F64 => output_stream::<f64>(device, config, shared),
        SampleFormat::I16 => output_stream::<i16>(device, config, shared),
        SampleFormat::I32 => output_stream::<i32>(device, config, shared),
        SampleFormat::U16 => output_stream::<u16>(device, config, shared),
        SampleFormat::U8 => output_stream::<u8>(device, config, shared),
        SampleFormat::I8 => output_stream::<i8>(device, config, shared),
        other => Err(format!("Unsupported output sample format: {other}")),
    }
}

fn output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels.max(1) as usize;
    let out_rate = config.sample_rate.max(1) as f64;
    let err_shared = shared.clone();
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                let silence = T::EQUILIBRIUM;
                let take = if shared.transport() == Transport::Playing {
                    shared.take.try_read().ok()
                } else {
                    None
                };
                let Some(take) = take else {
                    data.fill(silence);
                    return;
                };
                let step = take.sample_rate as f64 / out_rate;
                let len = take.samples.len() as f64;
                let mut pos = shared.pos();
                let mut ended = pos >= len;
                let mut peak = 0f32;
                for frame in data.chunks_mut(channels) {
                    let s = if ended { 0.0 } else { take.sample_at(pos) };
                    pos += step;
                    ended |= pos >= len;
                    peak = peak.max(s.abs());
                    let v = T::from_sample(s);
                    frame.fill(v);
                }
                shared
                    .output_peak
                    .fetch_max(peak.to_bits(), Ordering::AcqRel);
                // Only write back if nobody pressed Stop meanwhile.
                if shared.transport() == Transport::Playing {
                    if ended {
                        shared.set_pos(0.0);
                        shared.set_transport(Transport::Stopped);
                    } else {
                        shared.set_pos(pos);
                    }
                }
            },
            move |e| err_shared.report_error(format!("Output: {e}")),
            None,
        )
        .map_err(|e| format!("Could not open output: {e}"))
}

fn build_input(device: &cpal::Device, shared: Arc<Shared>) -> Result<cpal::Stream, String> {
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Input device has no usable format: {e}"))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    shared
        .input_rate
        .store(config.sample_rate, Ordering::Release);

    // Two seconds of slack between the input callback and the recorder thread.
    let (producer, consumer) = rtrb::RingBuffer::<f32>::new(config.sample_rate as usize * 2);
    *shared.recorded.lock().map_err(|e| e.to_string())? = Some(consumer);

    match format {
        SampleFormat::F32 => input_stream::<f32>(device, config, shared, producer),
        SampleFormat::F64 => input_stream::<f64>(device, config, shared, producer),
        SampleFormat::I16 => input_stream::<i16>(device, config, shared, producer),
        SampleFormat::I32 => input_stream::<i32>(device, config, shared, producer),
        SampleFormat::U16 => input_stream::<u16>(device, config, shared, producer),
        SampleFormat::U8 => input_stream::<u8>(device, config, shared, producer),
        SampleFormat::I8 => input_stream::<i8>(device, config, shared, producer),
        other => Err(format!("Unsupported input sample format: {other}")),
    }
}

fn input_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: Arc<Shared>,
    mut producer: rtrb::Producer<f32>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    let err_shared = shared.clone();
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                let recording = shared.transport() == Transport::Recording;
                let mut peak = 0f32;
                for frame in data.chunks(channels) {
                    let sum: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum();
                    let mono = sum / frame.len() as f32;
                    // Guard against drivers handing us garbage (NaN/inf).
                    let mono = if mono.is_finite() {
                        mono.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    peak = peak.max(mono.abs());
                    if recording {
                        // If the ring is full the recorder thread has stalled; drop
                        // samples rather than block the audio thread.
                        let _ = producer.push(mono);
                    }
                }
                shared
                    .input_peak
                    .fetch_max(peak.to_bits(), Ordering::AcqRel);
            },
            move |e| err_shared.report_error(format!("Input: {e}")),
            None,
        )
        .map_err(|e| format!("Could not open input: {e}"))
}
