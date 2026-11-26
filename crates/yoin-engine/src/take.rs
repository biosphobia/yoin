//! A recorded take: mono audio at a fixed sample rate, plus the pure helpers
//! (waveform peaks, WAV export, interpolated reads) that don't touch devices.

use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Take {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Take {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            samples: Vec::new(),
            sample_rate,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / self.sample_rate as f64
    }

    /// Linearly interpolated sample at a fractional position. Returns 0 past the end.
    #[inline]
    pub fn sample_at(&self, pos: f64) -> f32 {
        if pos < 0.0 {
            return 0.0;
        }
        let i = pos as usize;
        let Some(&a) = self.samples.get(i) else {
            return 0.0;
        };
        let b = self.samples.get(i + 1).copied().unwrap_or(0.0);
        let frac = (pos - i as f64) as f32;
        a + (b - a) * frac
    }

    /// Peak absolute value per bin, `bins` values across the whole take.
    pub fn peaks(&self, bins: usize) -> Vec<f32> {
        if bins == 0 || self.samples.is_empty() {
            return Vec::new();
        }
        let len = self.samples.len();
        (0..bins)
            .map(|b| {
                let start = b * len / bins;
                let end = ((b + 1) * len / bins).max(start + 1).min(len);
                self.samples[start..end]
                    .iter()
                    .fold(0.0f32, |m, s| m.max(s.abs()))
            })
            .collect()
    }

    /// Writes the take as a mono 32-bit float WAV file.
    pub fn write_wav(&self, path: &Path) -> Result<(), hound::Error> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)?;
        for &s in &self.samples {
            writer.write_sample(s)?;
        }
        writer.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_between_samples() {
        let t = Take {
            samples: vec![0.0, 1.0, 0.0],
            sample_rate: 4,
        };
        assert_eq!(t.sample_at(0.0), 0.0);
        assert_eq!(t.sample_at(0.5), 0.5);
        assert_eq!(t.sample_at(1.0), 1.0);
        assert_eq!(t.sample_at(1.25), 0.75);
        assert_eq!(t.sample_at(10.0), 0.0);
        assert_eq!(t.sample_at(-1.0), 0.0);
    }

    #[test]
    fn peaks_cover_whole_take() {
        let t = Take {
            samples: vec![0.1, -0.9, 0.2, 0.3, -0.4, 0.0],
            sample_rate: 6,
        };
        assert_eq!(t.peaks(3), vec![0.9, 0.3, 0.4]);
        assert_eq!(t.peaks(0), Vec::<f32>::new());
        // More bins than samples still yields `bins` values.
        assert_eq!(t.peaks(12).len(), 12);
        assert!((t.duration_secs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn wav_roundtrip() {
        let t = Take {
            samples: vec![0.0, 0.5, -0.5, 1.0],
            sample_rate: 48_000,
        };
        let path = std::env::temp_dir().join(format!("yoin-test-{}.wav", std::process::id()));
        t.write_wav(&path).unwrap();
        let mut r = hound::WavReader::open(&path).unwrap();
        assert_eq!(r.spec().sample_rate, 48_000);
        let back: Vec<f32> = r.samples::<f32>().map(|s| s.unwrap()).collect();
        assert_eq!(back, t.samples);
        let _ = std::fs::remove_file(path);
    }
}
