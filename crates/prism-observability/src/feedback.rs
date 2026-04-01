// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use serde::{Deserialize, Serialize};

/// Aggregated decode/render statistics reported by a client each feedback interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientFeedback {
    /// Mean decode latency over the reporting interval (µs).
    pub avg_decode_us: f64,
    /// Mean render latency over the reporting interval (µs).
    pub avg_render_us: f64,
    /// Total frames successfully decoded in the interval.
    pub frames_decoded: u32,
    /// Frames the decoder discarded (arrived too late or could not be decoded).
    pub frames_dropped: u32,
    /// Frames that were decoded but missed their display deadline.
    pub frames_late: u32,
    /// Current depth of the decoder input queue (unprocessed frames).
    pub decoder_queue_depth: u32,
}

/// Configuration controlling how often a client sends feedback.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientFeedbackConfig {
    /// Interval between feedback reports when the client is healthy (ms).
    pub normal_interval_ms: u32,
    /// Interval between feedback reports when the client is stressed (ms).
    pub stressed_interval_ms: u32,
    /// Queue depth at or above which the client is considered stressed.
    pub stress_threshold_queue_depth: u32,
    /// Drop-rate fraction at or above which the client is considered stressed.
    pub stress_threshold_drop_rate: f64,
}

impl Default for ClientFeedbackConfig {
    fn default() -> Self {
        Self {
            normal_interval_ms: 1_000,
            stressed_interval_ms: 200,
            stress_threshold_queue_depth: 3,
            stress_threshold_drop_rate: 0.05,
        }
    }
}

impl ClientFeedbackConfig {
    /// Returns `true` when the client should switch to the faster stressed interval.
    ///
    /// `queue_depth` is the current decoder queue depth and `drop_rate` is the
    /// fraction of frames dropped in the last measurement window (0.0–1.0).
    pub fn is_stressed(&self, queue_depth: u32, drop_rate: f64) -> bool {
        queue_depth >= self.stress_threshold_queue_depth
            || drop_rate >= self.stress_threshold_drop_rate
    }
}

/// Out-of-band alert raised by a client and delivered to the server.
///
/// Serialised with an internally-tagged `"type"` field so that new variants
/// can be added without breaking existing JSON parsers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientAlert {
    /// The decoder queue is persistently full; the server should reduce bitrate.
    DecoderOverloaded {
        /// Current decoder queue depth when the alert was raised.
        queue_depth: u32,
        /// Drop-rate that triggered the alert (fraction, 0.0–1.0).
        drop_rate: f64,
    },
    /// The client process has critically low memory.
    OutOfMemory,
    /// The display configuration has changed (resolution, DPI, etc.).
    DisplayChanged {
        /// New horizontal resolution in pixels.
        new_resolution: (u32, u32),
        /// New display scale factor (e.g. 1.5 for 150 % DPI).
        new_scale: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_feedback() -> ClientFeedback {
        ClientFeedback {
            avg_decode_us: 1_500.0,
            avg_render_us: 800.0,
            frames_decoded: 300,
            frames_dropped: 5,
            frames_late: 2,
            decoder_queue_depth: 1,
        }
    }

    #[test]
    fn client_feedback_json_roundtrip() {
        let fb = sample_feedback();
        let json = serde_json::to_string(&fb).expect("serialise");
        let back: ClientFeedback = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(fb, back);
    }

    #[test]
    fn config_defaults() {
        let cfg = ClientFeedbackConfig::default();
        assert_eq!(cfg.normal_interval_ms, 1_000);
        assert_eq!(cfg.stressed_interval_ms, 200);
        assert_eq!(cfg.stress_threshold_queue_depth, 3);
        assert!((cfg.stress_threshold_drop_rate - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn is_stressed_queue_depth() {
        let cfg = ClientFeedbackConfig::default();
        assert!(!cfg.is_stressed(2, 0.0), "depth 2 < 3 should be healthy");
        assert!(cfg.is_stressed(3, 0.0), "depth 3 >= 3 should be stressed");
        assert!(cfg.is_stressed(10, 0.0), "high depth stressed");
    }

    #[test]
    fn is_stressed_drop_rate() {
        let cfg = ClientFeedbackConfig::default();
        assert!(!cfg.is_stressed(0, 0.04), "drop 4% < 5% should be healthy");
        assert!(cfg.is_stressed(0, 0.05), "drop 5% >= 5% should be stressed");
        assert!(cfg.is_stressed(0, 0.5), "high drop rate stressed");
    }

    #[test]
    fn alert_decoder_overloaded_roundtrip() {
        let alert = ClientAlert::DecoderOverloaded {
            queue_depth: 7,
            drop_rate: 0.12,
        };
        let json = serde_json::to_string(&alert).expect("serialise");
        assert!(json.contains("\"type\":\"DecoderOverloaded\""));
        let back: ClientAlert = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(alert, back);
    }

    #[test]
    fn alert_out_of_memory_roundtrip() {
        let alert = ClientAlert::OutOfMemory;
        let json = serde_json::to_string(&alert).expect("serialise");
        let back: ClientAlert = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(alert, back);
    }

    #[test]
    fn alert_display_changed_roundtrip() {
        let alert = ClientAlert::DisplayChanged {
            new_resolution: (2560, 1440),
            new_scale: 1.5,
        };
        let json = serde_json::to_string(&alert).expect("serialise");
        let back: ClientAlert = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(alert, back);
    }
}
