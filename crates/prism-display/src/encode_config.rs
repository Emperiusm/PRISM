// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::CodecId;

/// High-level quality/latency trade-off for the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderPreset {
    /// Minimise encode latency at the expense of quality and compression ratio.
    UltraLowLatency,
    /// Prioritise output quality; may add encode latency.
    Quality,
    /// Balanced trade-off between latency and quality.
    Balanced,
}

/// Policy governing when the encoder inserts IDR / keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyframeInterval {
    /// Insert an IDR every `n` frames exactly.
    Fixed(u32),
    /// Adapt the IDR interval dynamically within `[min_frames, max_frames]`.
    Adaptive { min_frames: u32, max_frames: u32 },
    /// Only insert an IDR on explicit request (scene change / connection event).
    OnDemand,
}

/// How many slices per encoded frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SliceMode {
    /// Encode each frame as a single NALU slice.
    Single,
    /// Fixed number of slices per frame.
    Fixed(u8),
    /// Let the encoder vary slices between `min` and `max` based on content.
    Adaptive { min: u8, max: u8 },
}

/// Complete configuration for an encoder session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub codec: CodecId,
    pub preset: EncoderPreset,
    /// Nominal target bitrate in bits per second.
    pub bitrate_bps: u64,
    /// Maximum encode frame rate.
    pub max_fps: u8,
    /// Output resolution `(width, height)` in pixels.
    pub resolution: (u32, u32),
    pub keyframe_interval: KeyframeInterval,
    pub slice_mode: SliceMode,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            codec: CodecId::H264,
            preset: EncoderPreset::Balanced,
            bitrate_bps: 5_000_000,
            max_fps: 60,
            resolution: (1920, 1080),
            keyframe_interval: KeyframeInterval::Fixed(120),
            slice_mode: SliceMode::Single,
        }
    }
}

// ---------------------------------------------------------------------------
// RateControlHinter
// ---------------------------------------------------------------------------

/// Rolling average statistics for a single window/source.
#[derive(Debug, Clone)]
pub struct ComplexityEstimate {
    /// Exponential-moving-average QP (lower = better quality, more bits).
    pub avg_qp: f32,
    /// Exponential-moving-average bitrate in bits per second.
    pub avg_bitrate_bps: u64,
    /// Total number of frames recorded.
    pub frame_count: u32,
}

/// Tracks per-window encode complexity to guide bitrate allocation.
///
/// Uses a simple running (arithmetic) average so callers can retrieve a hint
/// for a given window handle without storing every sample.
#[derive(Debug, Default)]
pub struct RateControlHinter {
    estimates: HashMap<u64, ComplexityEstimate>,
}

impl RateControlHinter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new observation for `hwnd`.
    ///
    /// The `ComplexityEstimate` stored is the cumulative average over all
    /// recorded samples.
    pub fn record(&mut self, hwnd: u64, qp: f32, bitrate_bps: u64) {
        let entry = self.estimates.entry(hwnd).or_insert_with(|| ComplexityEstimate {
            avg_qp: 0.0,
            avg_bitrate_bps: 0,
            frame_count: 0,
        });

        let n = entry.frame_count + 1;
        // Running / cumulative average:  new_avg = old_avg + (x - old_avg) / n
        entry.avg_qp += (qp - entry.avg_qp) / n as f32;
        entry.avg_bitrate_bps = ((entry.avg_bitrate_bps as u128 * (n - 1) as u128
            + bitrate_bps as u128)
            / n as u128) as u64;
        entry.frame_count = n;
    }

    /// Return the current estimate for `hwnd`, or `None` if no samples have
    /// been recorded.
    pub fn hint(&self, hwnd: u64) -> Option<&ComplexityEstimate> {
        self.estimates.get(&hwnd)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_hinter_records_and_hints() {
        let mut hinter = RateControlHinter::new();
        let hwnd = 0xABCD_1234_u64;

        hinter.record(hwnd, 20.0, 4_000_000);
        hinter.record(hwnd, 30.0, 6_000_000);

        let hint = hinter.hint(hwnd).expect("should have estimate");
        assert_eq!(hint.frame_count, 2);
        // avg_qp should be (20 + 30) / 2 = 25
        assert!((hint.avg_qp - 25.0).abs() < 1e-4, "avg_qp={}", hint.avg_qp);
        // avg_bitrate should be (4_000_000 + 6_000_000) / 2 = 5_000_000
        assert_eq!(hint.avg_bitrate_bps, 5_000_000);
    }

    #[test]
    fn rate_hinter_unknown_window() {
        let hinter = RateControlHinter::new();
        assert!(hinter.hint(0xDEAD_BEEF).is_none());
    }

    #[test]
    fn encoder_config_defaults() {
        let cfg = EncoderConfig::default();
        assert_eq!(cfg.codec, CodecId::H264);
        assert_eq!(cfg.preset, EncoderPreset::Balanced);
        assert_eq!(cfg.bitrate_bps, 5_000_000);
        assert_eq!(cfg.max_fps, 60);
        assert_eq!(cfg.resolution, (1920, 1080));
        assert_eq!(cfg.keyframe_interval, KeyframeInterval::Fixed(120));
        assert_eq!(cfg.slice_mode, SliceMode::Single);
    }
}
