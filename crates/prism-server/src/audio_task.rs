/// Synthetic audio source — generates a 440 Hz sine wave as a stand-in for
/// real WASAPI capture.  Proves the audio pipeline (frame generation, silence
/// detection, future Opus encoding) without requiring COM/FFI integration.
pub struct SyntheticAudioSource {
    sample_rate: u32,
    channels: u16,
    phase: f64,
    frequency: f64,
}

impl SyntheticAudioSource {
    /// Create a new source at 48 kHz stereo, 440 Hz tone.
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            phase: 0.0,
            frequency: 440.0,
        }
    }

    /// Generate 20 ms of interleaved float audio (960 samples × channels).
    ///
    /// Samples are in the range [-1.0, 1.0] at amplitude 0.3.
    pub fn generate_frame(&mut self) -> Vec<f32> {
        let samples_per_frame = (self.sample_rate as usize * 20) / 1000; // 960 at 48 kHz
        let mut buf = Vec::with_capacity(samples_per_frame * self.channels as usize);
        for _ in 0..samples_per_frame {
            let sample = (self.phase * 2.0 * std::f64::consts::PI).sin() as f32 * 0.3;
            for _ in 0..self.channels {
                buf.push(sample);
            }
            self.phase += self.frequency / self.sample_rate as f64;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }
        buf
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of interleaved channels.
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

impl Default for SyntheticAudioSource {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_correct_sample_count() {
        let mut src = SyntheticAudioSource::new();
        let frame = src.generate_frame();
        // 960 samples * 2 channels = 1920 floats
        assert_eq!(frame.len(), 960 * 2);
    }

    #[test]
    fn samples_in_valid_range() {
        let mut src = SyntheticAudioSource::new();
        let frame = src.generate_frame();
        for &s in &frame {
            assert!(s >= -1.0 && s <= 1.0, "sample {s} out of [-1.0, 1.0]");
        }
    }

    #[test]
    fn generates_non_silent_audio() {
        let mut src = SyntheticAudioSource::new();
        let frame = src.generate_frame();
        let rms = (frame.iter().map(|&s| (s as f64).powi(2)).sum::<f64>()
            / frame.len() as f64)
            .sqrt();
        assert!(rms > 0.1, "RMS {rms:.4} is too low — audio should not be silent");
    }
}
