//! Tauri shell: exposes the audio engine to the web UI as commands.

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::State;
use yoin_engine::{DeviceList, Engine, Status};

/// The engine is `None` when no output device could be opened; the UI shows
/// the error and lets the user pick another device.
struct AppState {
    engine: Mutex<Option<Engine>>,
    startup_error: Mutex<Option<String>>,
}

fn with_engine<R>(
    state: &State<AppState>,
    f: impl FnOnce(&mut Engine) -> Result<R, String>,
) -> Result<R, String> {
    let mut guard = state.engine.lock().map_err(|e| e.to_string())?;
    match guard.as_mut() {
        Some(engine) => f(engine),
        None => Err("Audio is not running. Pick an output device in Settings.".into()),
    }
}

#[tauri::command]
fn list_devices() -> DeviceList {
    yoin_engine::list_devices()
}

#[tauri::command]
fn set_devices(
    state: State<AppState>,
    input: Option<String>,
    output: Option<String>,
) -> Result<(), String> {
    let mut guard = state.engine.lock().map_err(|e| e.to_string())?;
    match guard.as_mut() {
        Some(engine) => engine.set_devices(input.as_deref(), output.as_deref()),
        None => {
            *guard = Some(Engine::new(input.as_deref(), output.as_deref())?);
            *state.startup_error.lock().map_err(|e| e.to_string())? = None;
            Ok(())
        }
    }
}

#[tauri::command]
fn status(state: State<AppState>) -> Result<Status, String> {
    let guard = state.engine.lock().map_err(|e| e.to_string())?;
    match guard.as_ref() {
        Some(engine) => Ok(engine.status()),
        None => Err(state
            .startup_error
            .lock()
            .ok()
            .and_then(|e| e.clone())
            .unwrap_or_else(|| "Audio is not running.".into())),
    }
}

#[tauri::command]
fn peaks(state: State<AppState>, bins: usize) -> Result<Vec<f32>, String> {
    with_engine(&state, |e| Ok(e.peaks(bins.min(8192))))
}

#[tauri::command]
fn record(state: State<AppState>) -> Result<(), String> {
    with_engine(&state, |e| e.record())
}

#[tauri::command]
fn play(state: State<AppState>) -> Result<(), String> {
    with_engine(&state, |e| e.play())
}

#[tauri::command]
fn stop(state: State<AppState>) -> Result<(), String> {
    with_engine(&state, |e| {
        e.stop();
        Ok(())
    })
}

#[tauri::command]
fn seek(state: State<AppState>, secs: f64) -> Result<(), String> {
    with_engine(&state, |e| {
        e.seek(secs);
        Ok(())
    })
}

#[tauri::command]
fn export_wav(state: State<AppState>, path: PathBuf) -> Result<(), String> {
    with_engine(&state, |e| e.export_wav(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (engine, startup_error) = match Engine::new(None, None) {
        Ok(engine) => (Some(engine), None),
        Err(e) => (None, Some(e)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            engine: Mutex::new(engine),
            startup_error: Mutex::new(startup_error),
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            set_devices,
            status,
            peaks,
            record,
            play,
            stop,
            seek,
            export_wav
        ])
        .run(tauri::generate_context!())
        .expect("error while running Yoin");
}
