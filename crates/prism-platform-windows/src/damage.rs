// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! DXGI damage rect extraction and conversion to PRISM `Rect`.
//!
//! DXGI reports dirty regions in `DXGI_OUTDUPL_MOVE_RECT` / dirty rect arrays
//! using the Windows `RECT` layout `{left, top, right, bottom}`.  PRISM uses
//! `{x, y, w, h}`.  This module bridges the two.

use prism_display::{Rect, merge_damage_rects};

// ── DxgiRect ──────────────────────────────────────────────────────────────────

/// A Windows `RECT`-compatible rectangle as provided by the Desktop Duplication
/// API dirty-rect buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DxgiRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl DxgiRect {
    /// Convert to a PRISM `Rect`.
    ///
    /// Width and height are clamped to zero for malformed (inverted) rects.
    pub fn to_prism_rect(self) -> Rect {
        let w = (self.right - self.left).max(0) as u32;
        let h = (self.bottom - self.top).max(0) as u32;
        Rect {
            x: self.left,
            y: self.top,
            w,
            h,
        }
    }
}

// ── extract_damage ────────────────────────────────────────────────────────────

/// Convert a slice of DXGI dirty rects to PRISM rects and merge nearby ones.
///
/// Uses a proximity threshold of 64 pixels, matching the H.264/H.265
/// superblock alignment used elsewhere in PRISM.
pub fn extract_damage(dxgi_rects: &[DxgiRect]) -> Vec<Rect> {
    let prism: Vec<Rect> = dxgi_rects.iter().map(|r| r.to_prism_rect()).collect();
    merge_damage_rects(&prism, 64)
}

// ── is_full_damage ────────────────────────────────────────────────────────────

/// Returns `true` when the damage set should be treated as a full-frame
/// redraw.
///
/// Two cases trigger a full-damage signal:
/// 1. The `dxgi_rects` slice is empty — the Desktop Duplication API sometimes
///    delivers no dirty rects when the entire framebuffer changed (e.g. on
///    mode switch or first frame).
/// 2. A single rect that exactly covers the entire screen.
pub fn is_full_damage(dxgi_rects: &[DxgiRect], screen_w: u32, screen_h: u32) -> bool {
    match dxgi_rects {
        [] => true,
        [single] => {
            let r = single.to_prism_rect();
            r.x == 0 && r.y == 0 && r.w == screen_w && r.h == screen_h
        }
        _ => false,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxgi_to_prism_rect() {
        let r = DxgiRect {
            left: 10,
            top: 20,
            right: 110,
            bottom: 70,
        };
        let p = r.to_prism_rect();
        assert_eq!(p.x, 10);
        assert_eq!(p.y, 20);
        assert_eq!(p.w, 100);
        assert_eq!(p.h, 50);
    }

    #[test]
    fn empty_rect_clamps_to_zero() {
        // Inverted rect (right < left) → w = 0.
        let r = DxgiRect {
            left: 100,
            top: 50,
            right: 50,
            bottom: 100,
        };
        let p = r.to_prism_rect();
        assert_eq!(p.w, 0);
        assert_eq!(p.h, 50);

        // Zero-size rect.
        let r2 = DxgiRect {
            left: 5,
            top: 5,
            right: 5,
            bottom: 5,
        };
        let p2 = r2.to_prism_rect();
        assert_eq!(p2.w, 0);
        assert_eq!(p2.h, 0);
    }

    #[test]
    fn extract_and_merge_nearby_rects() {
        // Two rects within 64 px of each other should be merged into one.
        let rects = vec![
            DxgiRect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            DxgiRect {
                left: 120,
                top: 0,
                right: 220,
                bottom: 100,
            },
        ];
        let merged = extract_damage(&rects);
        // Gap between x=100 and x=120 is 20 px < 64 threshold → merged.
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].x, 0);
        assert_eq!(merged[0].w, 220);
    }

    #[test]
    fn full_damage_empty_rects() {
        assert!(is_full_damage(&[], 1920, 1080));
    }

    #[test]
    fn full_damage_single_fullscreen_rect() {
        let full = DxgiRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(is_full_damage(&[full], 1920, 1080));

        // A rect that only covers part of the screen is NOT full damage.
        let partial = DxgiRect {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 1080,
        };
        assert!(!is_full_damage(&[partial], 1920, 1080));
    }

    #[test]
    fn partial_damage_not_full() {
        // Two separate rects — never matches the single-fullscreen case.
        let r1 = DxgiRect {
            left: 0,
            top: 0,
            right: 960,
            bottom: 540,
        };
        let r2 = DxgiRect {
            left: 960,
            top: 540,
            right: 1920,
            bottom: 1080,
        };
        assert!(!is_full_damage(&[r1, r2], 1920, 1080));
    }
}
