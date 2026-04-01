// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// ── SilenceDetector ──────────────────────────────────────────────────────────

/// Detects silence in a stream of PCM audio frames using RMS energy.
///
/// A frame is considered silent when its RMS amplitude falls below
/// `threshold_rms`. Once `silent_threshold` consecutive silent frames have
/// been observed, `is_silent` returns `true`.
pub struct SilenceDetector {
    threshold_rms: f32,
    silent_frames: u32,
    silent_threshold: u32,
}

impl SilenceDetector {
    pub fn new(threshold_rms: f32, silent_threshold_frames: u32) -> Self {
        Self {
            threshold_rms,
            silent_frames: 0,
            silent_threshold: silent_threshold_frames,
        }
    }

    /// Returns `true` once at least `silent_threshold` consecutive silent
    /// frames have been seen (including this one).
    pub fn is_silent(&mut self, samples: &[f32]) -> bool {
        let rms = if samples.is_empty() {
            0.0_f32
        } else {
            let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
            (sum_sq / samples.len() as f32).sqrt()
        };

        if rms < self.threshold_rms {
            self.silent_frames += 1;
        } else {
            self.silent_frames = 0;
        }

        self.silent_frames >= self.silent_threshold
    }

    /// Reset the consecutive-silence counter.
    pub fn reset(&mut self) {
        self.silent_frames = 0;
    }
}

impl Default for SilenceDetector {
    fn default() -> Self {
        Self::new(0.001, 5)
    }
}

// ── AudioFrameHeader ─────────────────────────────────────────────────────────

/// Fixed-size 8-byte header prefixed to every audio datagram.
///
/// Layout (little-endian):
///   [0..4] sample_rate     (u32)
///   [4..6] channels        (u16)
///   [6..8] frame_duration_ms (u16)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrameHeader {
    pub sample_rate: u32,
    pub channels: u16,
    pub frame_duration_ms: u16,
}

pub const AUDIO_HEADER_SIZE: usize = 8;

impl AudioFrameHeader {
    pub fn to_bytes(&self) -> [u8; AUDIO_HEADER_SIZE] {
        let mut buf = [0u8; AUDIO_HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.sample_rate.to_le_bytes());
        buf[4..6].copy_from_slice(&self.channels.to_le_bytes());
        buf[6..8].copy_from_slice(&self.frame_duration_ms.to_le_bytes());
        buf
    }

    /// Returns `None` if `bytes` is shorter than `AUDIO_HEADER_SIZE`.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < AUDIO_HEADER_SIZE {
            return None;
        }
        let sample_rate = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let channels = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        let frame_duration_ms = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        Some(Self {
            sample_rate,
            channels,
            frame_duration_ms,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_after_threshold() {
        let mut det = SilenceDetector::new(0.001, 3);
        let silent = vec![0.0f32; 64];
        // Frames 1 and 2 — below threshold count but not yet at 3.
        assert!(!det.is_silent(&silent));
        assert!(!det.is_silent(&silent));
        // Frame 3 — reaches threshold.
        assert!(det.is_silent(&silent));
    }

    #[test]
    fn sound_resets_counter() {
        let mut det = SilenceDetector::new(0.001, 2);
        let silent = vec![0.0f32; 64];
        let loud = vec![1.0f32; 64];
        assert!(!det.is_silent(&silent)); // silent_frames = 1
        assert!(!det.is_silent(&loud)); // loud resets to 0
        assert!(!det.is_silent(&silent)); // silent_frames = 1 again
        assert!(det.is_silent(&silent)); // silent_frames = 2 → true
    }

    #[test]
    fn empty_slice_is_silent() {
        let mut det = SilenceDetector::new(0.001, 1);
        // RMS of empty slice is 0.0, which is below threshold → silent.
        assert!(det.is_silent(&[]));
    }

    #[test]
    fn header_roundtrip() {
        let hdr = AudioFrameHeader {
            sample_rate: 48_000,
            channels: 2,
            frame_duration_ms: 20,
        };
        let bytes = hdr.to_bytes();
        assert_eq!(bytes.len(), AUDIO_HEADER_SIZE);
        let decoded = AudioFrameHeader::from_bytes(&bytes).expect("decode failed");
        assert_eq!(decoded, hdr);
    }

    #[test]
    fn header_too_short() {
        let short = [0u8; 4];
        assert!(AudioFrameHeader::from_bytes(&short).is_none());
    }
}
