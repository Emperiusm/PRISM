// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Profiles: performance profile management.

use serde::{Deserialize, Serialize};

/// Encoder quality/latency trade-off preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderPreset {
    UltraLowLatency,
    Quality,
    Balanced,
}

/// Display-channel specific profile settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayProfile {
    pub prefer_lossless_text: bool,
    pub max_fps: u8,
    pub region_detection: bool,
    pub encoder_preset: EncoderPreset,
}

/// A named collection of settings that tunes the remote-desktop experience.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub name: String,
    pub display: DisplayProfile,
}

impl ConnectionProfile {
    /// Preset tuned for gaming: maximum frame rate, minimum latency, no lossless text pass.
    pub fn gaming() -> Self {
        Self {
            name: "Gaming".to_string(),
            display: DisplayProfile {
                prefer_lossless_text: false,
                max_fps: 120,
                region_detection: false,
                encoder_preset: EncoderPreset::UltraLowLatency,
            },
        }
    }

    /// Preset tuned for development work: lossless text rendering and region-change detection.
    pub fn coding() -> Self {
        Self {
            name: "Coding".to_string(),
            display: DisplayProfile {
                prefer_lossless_text: true,
                max_fps: 60,
                region_detection: true,
                encoder_preset: EncoderPreset::Quality,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaming_defaults() {
        let p = ConnectionProfile::gaming();
        assert_eq!(p.name, "Gaming");
        assert_eq!(p.display.max_fps, 120);
        assert!(!p.display.prefer_lossless_text);
        assert!(!p.display.region_detection);
        assert_eq!(p.display.encoder_preset, EncoderPreset::UltraLowLatency);
    }

    #[test]
    fn coding_defaults() {
        let p = ConnectionProfile::coding();
        assert_eq!(p.name, "Coding");
        assert_eq!(p.display.max_fps, 60);
        assert!(p.display.prefer_lossless_text);
        assert!(p.display.region_detection);
        assert_eq!(p.display.encoder_preset, EncoderPreset::Quality);
    }
}
