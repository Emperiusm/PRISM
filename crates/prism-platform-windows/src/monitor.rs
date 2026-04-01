// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! DXGI monitor enumeration helpers.
//!
//! Converts raw DXGI adapter/output data into PRISM `MonitorInfo` values and
//! provides a compact encoding for `DisplayId` that embeds both the adapter
//! index and the output index within that adapter.
//!
//! # DisplayId encoding
//! `display_id = adapter_index * 16 + output_index`
//!
//! This supports up to 16 outputs per adapter, which covers all practical
//! multi-GPU, multi-monitor configurations.

use prism_display::{DisplayId, MonitorInfo};

// ── OutputRotation ────────────────────────────────────────────────────────────

/// Rotation state of a DXGI output as reported by `DXGI_OUTPUT_DESC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputRotation {
    /// No rotation (landscape, standard orientation).
    Identity,
    /// 90° clockwise rotation (portrait, right-side up).
    Rotate90,
    /// 180° rotation (landscape, upside down).
    Rotate180,
    /// 270° clockwise rotation (portrait, upside down).
    Rotate270,
}

// ── DxgiOutputInfo ────────────────────────────────────────────────────────────

/// Raw metadata for one DXGI adapter output, prior to conversion into the
/// platform-agnostic `MonitorInfo`.
#[derive(Debug, Clone)]
pub struct DxgiOutputInfo {
    /// Zero-based index of the DXGI adapter that owns this output.
    pub adapter_index: u32,
    /// Zero-based index of this output within its adapter.
    pub output_index: u32,
    /// Human-readable monitor name from `DXGI_OUTPUT_DESC.DeviceName`.
    pub name: String,
    /// Native resolution `(width, height)` in pixels.
    pub resolution: (u32, u32),
    /// Desktop position of the top-left corner of the monitor.
    pub position: (i32, i32),
    /// Vertical refresh rate in Hz.
    pub refresh_rate: u8,
    /// Whether this is the primary monitor.
    pub is_primary: bool,
    /// Rotation state of the physical panel.
    pub rotation: OutputRotation,
}

impl DxgiOutputInfo {
    /// Encode this output's adapter/output indices into a `DisplayId`.
    ///
    /// `display_id = adapter_index * 16 + output_index`
    pub fn display_id(&self) -> DisplayId {
        DisplayId(self.adapter_index * 16 + self.output_index)
    }

    /// Effective (logical) resolution after accounting for rotation.
    ///
    /// For 90° and 270° rotations the width and height are swapped because
    /// the OS presents the rotated framebuffer as if width < height.
    pub fn effective_resolution(&self) -> (u32, u32) {
        match self.rotation {
            OutputRotation::Rotate90 | OutputRotation::Rotate270 => {
                (self.resolution.1, self.resolution.0)
            }
            OutputRotation::Identity | OutputRotation::Rotate180 => self.resolution,
        }
    }

    /// Convert to the platform-agnostic `MonitorInfo` used throughout PRISM.
    ///
    /// - `display_id` is encoded from adapter + output indices.
    /// - `scale_factor` defaults to `1.0` (DPI information is not available
    ///   from DXGI alone; callers can override after conversion if needed).
    /// - `is_virtual` is always `false` for physical DXGI outputs.
    pub fn to_monitor_info(&self) -> MonitorInfo {
        let (w, h) = self.effective_resolution();
        MonitorInfo {
            display_id: self.display_id(),
            name: self.name.clone(),
            resolution: (w, h),
            position: self.position,
            scale_factor: 1.0,
            refresh_rate: self.refresh_rate,
            primary: self.is_primary,
            is_virtual: false,
        }
    }
}

// ── parse_display_id ──────────────────────────────────────────────────────────

/// Decode a `DisplayId` back into `(adapter_index, output_index)`.
///
/// Inverse of the encoding performed by `DxgiOutputInfo::display_id()`.
pub fn parse_display_id(id: DisplayId) -> (u32, u32) {
    let adapter = id.0 / 16;
    let output = id.0 % 16;
    (adapter, output)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_output(adapter: u32, output: u32, rotation: OutputRotation) -> DxgiOutputInfo {
        DxgiOutputInfo {
            adapter_index: adapter,
            output_index: output,
            name: format!("DISPLAY{}", output),
            resolution: (1920, 1080),
            position: (0, 0),
            refresh_rate: 60,
            is_primary: adapter == 0 && output == 0,
            rotation,
        }
    }

    #[test]
    fn to_monitor_info_identity() {
        let info = make_output(0, 0, OutputRotation::Identity).to_monitor_info();
        assert_eq!(info.display_id, DisplayId(0));
        assert_eq!(info.resolution, (1920, 1080));
        assert_eq!(info.refresh_rate, 60);
        assert!(info.primary);
        assert!(!info.is_virtual);
        assert!((info.scale_factor - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rotation_swaps_resolution() {
        let out90 = make_output(0, 1, OutputRotation::Rotate90);
        assert_eq!(out90.effective_resolution(), (1080, 1920));

        let out270 = make_output(0, 2, OutputRotation::Rotate270);
        assert_eq!(out270.effective_resolution(), (1080, 1920));

        // Identity and 180° do NOT swap.
        let out180 = make_output(0, 3, OutputRotation::Rotate180);
        assert_eq!(out180.effective_resolution(), (1920, 1080));

        let out_id = make_output(0, 4, OutputRotation::Identity);
        assert_eq!(out_id.effective_resolution(), (1920, 1080));
    }

    #[test]
    fn display_id_encoding() {
        // adapter=0, output=0 → DisplayId(0)
        assert_eq!(make_output(0, 0, OutputRotation::Identity).display_id(), DisplayId(0));
        // adapter=0, output=3 → DisplayId(3)
        assert_eq!(make_output(0, 3, OutputRotation::Identity).display_id(), DisplayId(3));
        // adapter=1, output=0 → DisplayId(16)
        assert_eq!(make_output(1, 0, OutputRotation::Identity).display_id(), DisplayId(16));
        // adapter=2, output=5 → DisplayId(37)
        assert_eq!(make_output(2, 5, OutputRotation::Identity).display_id(), DisplayId(37));
    }

    #[test]
    fn identity_rotation_preserves_resolution_in_monitor_info() {
        let out = make_output(0, 0, OutputRotation::Identity);
        let info = out.to_monitor_info();
        assert_eq!(info.resolution, (1920, 1080));
    }

    #[test]
    fn secondary_monitor_is_not_primary() {
        let out = DxgiOutputInfo {
            adapter_index: 0,
            output_index: 1,
            name: "DISPLAY1".to_string(),
            resolution: (2560, 1440),
            position: (1920, 0),
            refresh_rate: 144,
            is_primary: false,
            rotation: OutputRotation::Identity,
        };
        let info = out.to_monitor_info();
        assert_eq!(info.display_id, DisplayId(1));
        assert_eq!(info.resolution, (2560, 1440));
        assert_eq!(info.position, (1920, 0));
        assert_eq!(info.refresh_rate, 144);
        assert!(!info.primary);

        // Round-trip the display id.
        let (adapter, output) = parse_display_id(info.display_id);
        assert_eq!(adapter, 0);
        assert_eq!(output, 1);
    }
}
