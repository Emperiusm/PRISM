// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use crate::types::CodecId;
use prism_transport::quality::QualityRecommendation;

/// A single step on the degradation ladder.
#[derive(Debug, Clone, PartialEq)]
pub struct DegradationLevel {
    pub name: String,
    /// Maximum bitrate in megabits per second.
    pub max_bitrate_mbps: u64,
    /// Target resolution as (width, height).
    pub resolution: (u32, u32),
    /// Maximum frames per second.
    pub max_fps: u8,
    /// Codec to use at this level.
    pub codec: CodecId,
    /// Whether region detection is enabled.
    pub region_detection: bool,
    /// Forward-error-correction ratio (0.0 = disabled).
    pub fec_ratio: f32,
}

/// An ordered sequence of degradation levels for a given use-case profile.
///
/// Level 0 is the highest quality; higher indices are progressively degraded.
#[derive(Debug, Clone)]
pub struct DegradationLadder {
    pub profile_name: String,
    pub levels: Vec<DegradationLevel>,
}

impl DegradationLadder {
    /// Gaming profile: prioritise high resolution and frame-rate.
    pub fn gaming() -> Self {
        Self {
            profile_name: "gaming".into(),
            levels: vec![
                DegradationLevel {
                    name: "optimal".into(),
                    max_bitrate_mbps: 80,
                    resolution: (3840, 2160),
                    max_fps: 120,
                    codec: CodecId::H265,
                    region_detection: false,
                    fec_ratio: 0.0,
                },
                DegradationLevel {
                    name: "reduced_res".into(),
                    max_bitrate_mbps: 40,
                    resolution: (2560, 1440),
                    max_fps: 120,
                    codec: CodecId::H265,
                    region_detection: false,
                    fec_ratio: 0.0,
                },
                DegradationLevel {
                    name: "reduced_fps".into(),
                    max_bitrate_mbps: 20,
                    resolution: (1920, 1080),
                    max_fps: 60,
                    codec: CodecId::H264,
                    region_detection: false,
                    fec_ratio: 0.0,
                },
                DegradationLevel {
                    name: "minimum".into(),
                    max_bitrate_mbps: 8,
                    resolution: (1280, 720),
                    max_fps: 30,
                    codec: CodecId::H264,
                    region_detection: false,
                    fec_ratio: 0.1,
                },
            ],
        }
    }

    /// Coding/productivity profile: prioritise region detection for lossless text areas.
    pub fn coding() -> Self {
        Self {
            profile_name: "coding".into(),
            levels: vec![
                DegradationLevel {
                    name: "optimal".into(),
                    max_bitrate_mbps: 20,
                    resolution: (3840, 2160),
                    max_fps: 60,
                    codec: CodecId::H265,
                    region_detection: true,
                    fec_ratio: 0.0,
                },
                DegradationLevel {
                    name: "reduced_bw".into(),
                    max_bitrate_mbps: 8,
                    resolution: (2560, 1440),
                    max_fps: 60,
                    codec: CodecId::H264,
                    region_detection: true,
                    fec_ratio: 0.0,
                },
                DegradationLevel {
                    name: "reduced_fps".into(),
                    max_bitrate_mbps: 4,
                    resolution: (1920, 1080),
                    max_fps: 30,
                    codec: CodecId::H264,
                    region_detection: true,
                    fec_ratio: 0.0,
                },
                DegradationLevel {
                    name: "minimum".into(),
                    max_bitrate_mbps: 1,
                    resolution: (1280, 720),
                    max_fps: 15,
                    codec: CodecId::H264,
                    region_detection: true,
                    fec_ratio: 0.15,
                },
            ],
        }
    }

    /// Map a `QualityRecommendation` to a level index in this ladder.
    ///
    /// Returns the index (0 = best) that best satisfies the recommendation.
    pub fn target_level(&self, rec: &QualityRecommendation) -> usize {
        let last = self.levels.len().saturating_sub(1);
        match rec {
            QualityRecommendation::Optimal => 0,
            QualityRecommendation::ReduceBitrate { .. } => 1.min(last),
            QualityRecommendation::ReduceResolution => 1.min(last),
            QualityRecommendation::ReduceFramerate => 2.min(last),
            QualityRecommendation::EnableFec { .. } => last,
            QualityRecommendation::SwitchToStreamOnly => 2.min(last),
            QualityRecommendation::PauseNonEssential => last,
            QualityRecommendation::ConnectionUnusable => last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaming_level0_optimal() {
        let ladder = DegradationLadder::gaming();
        let l = &ladder.levels[0];
        assert_eq!(l.name, "optimal");
        assert_eq!(l.max_bitrate_mbps, 80);
        assert_eq!(l.resolution, (3840, 2160));
        assert_eq!(l.max_fps, 120);
        assert_eq!(l.codec, CodecId::H265);
        assert!(!l.region_detection);
        assert_eq!(l.fec_ratio, 0.0);
    }

    #[test]
    fn coding_always_has_regions() {
        let ladder = DegradationLadder::coding();
        for level in &ladder.levels {
            assert!(
                level.region_detection,
                "level '{}' should have region_detection=true",
                level.name
            );
        }
    }

    #[test]
    fn gaming_drops_res_before_fps() {
        let ladder = DegradationLadder::gaming();
        // Level 0→1: resolution drops, fps stays at 120
        assert!(ladder.levels[1].resolution.0 < ladder.levels[0].resolution.0);
        assert_eq!(ladder.levels[1].max_fps, ladder.levels[0].max_fps);
        // Level 1→2: fps drops
        assert!(ladder.levels[2].max_fps < ladder.levels[1].max_fps);
    }

    #[test]
    fn coding_drops_fps_before_res() {
        let ladder = DegradationLadder::coding();
        // Level 0→1: bitrate drops but fps stays at 60
        assert!(ladder.levels[1].max_bitrate_mbps < ladder.levels[0].max_bitrate_mbps);
        assert_eq!(ladder.levels[1].max_fps, ladder.levels[0].max_fps);
        // Level 1→2: fps drops
        assert!(ladder.levels[2].max_fps < ladder.levels[1].max_fps);
    }

    #[test]
    fn target_level_for_recommendation() {
        let ladder = DegradationLadder::gaming();
        assert_eq!(ladder.target_level(&QualityRecommendation::Optimal), 0);
        assert_eq!(
            ladder.target_level(&QualityRecommendation::ReduceResolution),
            1
        );
        assert_eq!(
            ladder.target_level(&QualityRecommendation::ReduceFramerate),
            2
        );
        assert_eq!(
            ladder.target_level(&QualityRecommendation::ConnectionUnusable),
            3
        );
    }

    #[test]
    fn all_profiles_have_4_levels() {
        assert_eq!(DegradationLadder::gaming().levels.len(), 4);
        assert_eq!(DegradationLadder::coding().levels.len(), 4);
    }

    #[test]
    fn bitrate_decreases_with_level() {
        for ladder in [DegradationLadder::gaming(), DegradationLadder::coding()] {
            let bitrates: Vec<u64> = ladder.levels.iter().map(|l| l.max_bitrate_mbps).collect();
            for w in bitrates.windows(2) {
                assert!(w[0] > w[1], "bitrate should decrease: {} > {}", w[0], w[1]);
            }
        }
    }
}
