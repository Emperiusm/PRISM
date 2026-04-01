// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Direct3D 11 device and context management.

// ── D3DFeatureLevel ───────────────────────────────────────────────────────────

/// Direct3D feature level, ordered from lowest to highest capability.
///
/// Used to negotiate the minimum acceptable feature set when creating a D3D11
/// device and to gate capabilities that require specific hardware tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum D3DFeatureLevel {
    /// D3D_FEATURE_LEVEL_9_1
    Level9_1,
    /// D3D_FEATURE_LEVEL_10_0
    Level10_0,
    /// D3D_FEATURE_LEVEL_10_1
    Level10_1,
    /// D3D_FEATURE_LEVEL_11_0
    Level11_0,
    /// D3D_FEATURE_LEVEL_11_1
    Level11_1,
    /// D3D_FEATURE_LEVEL_12_0
    Level12_0,
    /// D3D_FEATURE_LEVEL_12_1
    Level12_1,
}

impl D3DFeatureLevel {
    /// Returns `true` if this feature level supports Desktop Duplication API.
    ///
    /// DDA requires at least D3D 11.0 hardware.
    #[inline]
    pub fn supports_dda(self) -> bool {
        self >= D3DFeatureLevel::Level11_0
    }

    /// Returns `true` if this feature level supports compute shaders (CS 5.0).
    ///
    /// Compute shaders also require at least D3D 11.0 hardware.
    #[inline]
    pub fn supports_compute_shader(self) -> bool {
        self >= D3DFeatureLevel::Level11_0
    }

    /// Map to the raw UINT value passed to `D3D11CreateDevice`.
    ///
    /// These constants match the `D3D_FEATURE_LEVEL_*` enum in `d3dcommon.h`.
    pub fn to_d3d_value(self) -> u32 {
        match self {
            D3DFeatureLevel::Level9_1  => 0x9100,
            D3DFeatureLevel::Level10_0 => 0xa000,
            D3DFeatureLevel::Level10_1 => 0xa100,
            D3DFeatureLevel::Level11_0 => 0xb000,
            D3DFeatureLevel::Level11_1 => 0xb100,
            D3DFeatureLevel::Level12_0 => 0xc000,
            D3DFeatureLevel::Level12_1 => 0xc100,
        }
    }
}

// ── D3DDeviceConfig ───────────────────────────────────────────────────────────

/// Configuration used when creating a D3D11 device for capture or encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D3DDeviceConfig {
    /// Zero-based DXGI adapter index to use.  `0` selects the default adapter.
    pub adapter_index: u32,
    /// The lowest feature level the application will accept.
    pub min_feature_level: D3DFeatureLevel,
    /// Enable the D3D11 debug layer (requires the Windows SDK to be installed).
    pub debug_layer: bool,
}

impl Default for D3DDeviceConfig {
    /// Default: adapter 0, minimum feature level 11.0, no debug layer.
    fn default() -> Self {
        Self {
            adapter_index: 0,
            min_feature_level: D3DFeatureLevel::Level11_0,
            debug_layer: false,
        }
    }
}

impl D3DDeviceConfig {
    /// Return a config targeting the given adapter index, keeping all other
    /// fields at their defaults.
    pub fn with_adapter(adapter_index: u32) -> Self {
        Self {
            adapter_index,
            ..Self::default()
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_level_ordering() {
        assert!(D3DFeatureLevel::Level9_1  < D3DFeatureLevel::Level10_0);
        assert!(D3DFeatureLevel::Level10_0 < D3DFeatureLevel::Level10_1);
        assert!(D3DFeatureLevel::Level10_1 < D3DFeatureLevel::Level11_0);
        assert!(D3DFeatureLevel::Level11_0 < D3DFeatureLevel::Level11_1);
        assert!(D3DFeatureLevel::Level11_1 < D3DFeatureLevel::Level12_0);
        assert!(D3DFeatureLevel::Level12_0 < D3DFeatureLevel::Level12_1);
    }

    #[test]
    fn dda_requires_11_0() {
        assert!(!D3DFeatureLevel::Level9_1.supports_dda());
        assert!(!D3DFeatureLevel::Level10_0.supports_dda());
        assert!(!D3DFeatureLevel::Level10_1.supports_dda());
        assert!(D3DFeatureLevel::Level11_0.supports_dda());
        assert!(D3DFeatureLevel::Level11_1.supports_dda());
        assert!(D3DFeatureLevel::Level12_0.supports_dda());
        assert!(D3DFeatureLevel::Level12_1.supports_dda());

        // Compute shader gate is identical to DDA gate.
        assert!(!D3DFeatureLevel::Level10_1.supports_compute_shader());
        assert!(D3DFeatureLevel::Level11_0.supports_compute_shader());
    }

    #[test]
    fn d3d_value_encoding() {
        assert_eq!(D3DFeatureLevel::Level9_1.to_d3d_value(),  0x9100);
        assert_eq!(D3DFeatureLevel::Level10_0.to_d3d_value(), 0xa000);
        assert_eq!(D3DFeatureLevel::Level10_1.to_d3d_value(), 0xa100);
        assert_eq!(D3DFeatureLevel::Level11_0.to_d3d_value(), 0xb000);
        assert_eq!(D3DFeatureLevel::Level11_1.to_d3d_value(), 0xb100);
        assert_eq!(D3DFeatureLevel::Level12_0.to_d3d_value(), 0xc000);
        assert_eq!(D3DFeatureLevel::Level12_1.to_d3d_value(), 0xc100);
    }

    #[test]
    fn default_config() {
        let cfg = D3DDeviceConfig::default();
        assert_eq!(cfg.adapter_index, 0);
        assert_eq!(cfg.min_feature_level, D3DFeatureLevel::Level11_0);
        assert!(!cfg.debug_layer);
    }

    #[test]
    fn config_with_adapter() {
        let cfg = D3DDeviceConfig::with_adapter(2);
        assert_eq!(cfg.adapter_index, 2);
        // Other fields stay at defaults.
        assert_eq!(cfg.min_feature_level, D3DFeatureLevel::Level11_0);
        assert!(!cfg.debug_layer);
    }
}
