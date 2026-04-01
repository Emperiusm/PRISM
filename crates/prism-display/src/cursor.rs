// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use bytes::Bytes;

/// A cursor image and its hotspot.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorShape {
    pub width: u32,
    pub height: u32,
    /// X offset from the left edge to the hotspot pixel.
    pub hotspot_x: u32,
    /// Y offset from the top edge to the hotspot pixel.
    pub hotspot_y: u32,
    /// RGBA pixel data, row-major, `width * height * 4` bytes.
    pub data: Bytes,
    /// Fast-compare hash of the pixel data (e.g. FNV-1a or xxHash).
    pub hash: u64,
}

/// A cursor position sample from the client.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorPosition {
    /// Horizontal position in display coordinates.
    pub x: f32,
    /// Vertical position in display coordinates.
    pub y: f32,
    /// Whether the cursor is currently visible.
    pub visible: bool,
    /// Capture timestamp in microseconds.
    pub timestamp_us: u64,
}

/// Tracks the current cursor shape and avoids redundant transmissions.
#[derive(Debug, Default)]
pub struct CursorManager {
    current_shape: Option<CursorShape>,
    /// Hash of the shape most recently sent to the remote end.
    last_sent_hash: u64,
}

impl CursorManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the current shape.
    ///
    /// Returns `true` if the new shape differs from the previously stored one
    /// (i.e. it needs to be transmitted).
    pub fn update_shape(&mut self, shape: CursorShape) -> bool {
        let changed = shape.hash != self.last_sent_hash;
        if changed {
            self.last_sent_hash = shape.hash;
        }
        self.current_shape = Some(shape);
        changed
    }

    /// Borrow the current cursor shape, if any.
    pub fn current_shape(&self) -> Option<&CursorShape> {
        self.current_shape.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_shape(hash: u64) -> CursorShape {
        CursorShape {
            width: 32,
            height: 32,
            hotspot_x: 0,
            hotspot_y: 0,
            data: Bytes::from(vec![0u8; 32 * 32 * 4]),
            hash,
        }
    }

    #[test]
    fn shape_change_detected() {
        let mut mgr = CursorManager::new();

        // First shape — always considered changed (nothing was sent before).
        assert!(mgr.update_shape(make_shape(0xDEAD_BEEF)));

        // Same hash again — not changed.
        assert!(!mgr.update_shape(make_shape(0xDEAD_BEEF)));

        // Different hash — changed.
        assert!(mgr.update_shape(make_shape(0xCAFE_BABE)));
    }

    #[test]
    fn cursor_position_normalized() {
        // CursorPosition accepts floating-point coordinates and preserves them.
        let pos = CursorPosition {
            x: 0.5,
            y: 0.75,
            visible: true,
            timestamp_us: 123_456,
        };
        assert!((pos.x - 0.5).abs() < f32::EPSILON);
        assert!((pos.y - 0.75).abs() < f32::EPSILON);
        assert!(pos.visible);
        assert_eq!(pos.timestamp_us, 123_456);
    }
}
