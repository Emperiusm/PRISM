// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use serde::{Deserialize, Serialize};

/// An axis-aligned rectangle in display coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    /// Area in pixels.
    #[inline]
    pub fn area(&self) -> u64 {
        self.w as u64 * self.h as u64
    }

    /// True if pixel (px, py) falls inside this rect.
    #[inline]
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w as i32 && py < self.y + self.h as i32
    }

    /// True if this rect overlaps with `other` (touching edges are NOT an intersection).
    #[inline]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w as i32
            && other.x < self.x + self.w as i32
            && self.y < other.y + other.h as i32
            && other.y < self.y + self.h as i32
    }

    /// Smallest bounding rect that contains both `self` and `other`.
    #[inline]
    pub fn merge(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let x2 = (self.x + self.w as i32).max(other.x + other.w as i32);
        let y2 = (self.y + self.h as i32).max(other.y + other.h as i32);
        Rect {
            x,
            y,
            w: (x2 - x) as u32,
            h: (y2 - y) as u32,
        }
    }
}

/// Opaque identifier for a display/monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayId(pub u32);

/// Video codec identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodecId {
    H264,
    H265,
    Av1,
}

/// Pixel format of a captured texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureFormat {
    /// 32-bit BGRA, 8 bits per channel.
    Bgra8,
    /// YUV 4:2:0, NV12 layout (8-bit luma plane + interleaved chroma).
    Nv12,
    /// YUV 4:2:0, P010 layout (10-bit, packed in 16-bit words).
    P010,
}

/// Encode quality tier controlling bitrate and encoder settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityTier {
    /// Full-quality encode for active displays.
    Normal,
    /// Reduced-quality encode for preview/thumbnail contexts.
    Preview,
}

/// Lossless sub-format used in `RegionEncoding::Lossless`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LosslessFormat {
    /// Hardware H.264 lossless profile.
    HwH264Lossless,
    /// Hardware H.265 lossless profile.
    HwH265Lossless,
    /// CPU-side QOI image codec.
    CpuQoi,
    /// Delta / XOR encode against previous frame.
    Delta,
}

/// Encoding strategy chosen for a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionEncoding {
    /// Lossy video encode.
    Video { codec: CodecId, is_keyframe: bool },
    /// Lossless encode (various formats).
    Lossless { format: LosslessFormat },
    /// Send only the damage bounding rect; client reconstructs from cache.
    DamageRect,
    /// Region is identical to previous frame; skip transmission.
    Unchanged,
}

/// A GPU texture shared across process boundaries (e.g. via DXGI handle).
#[derive(Debug, Clone)]
pub struct SharedTexture {
    /// Platform-specific handle (e.g. NT HANDLE cast to u64 on Windows).
    pub handle: u64,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_area() {
        let r = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert_eq!(r.area(), 1920 * 1080);
    }

    #[test]
    fn rect_area_zero() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 0,
            h: 100,
        };
        assert_eq!(r.area(), 0);
    }

    #[test]
    fn rect_contains_point() {
        let r = Rect {
            x: 10,
            y: 20,
            w: 100,
            h: 50,
        };
        assert!(r.contains(10, 20)); // top-left corner (inclusive)
        assert!(r.contains(50, 40)); // interior
        assert!(!r.contains(110, 40)); // right edge (exclusive)
        assert!(!r.contains(50, 70)); // bottom edge (exclusive)
        assert!(!r.contains(9, 20)); // just outside left
    }

    #[test]
    fn rect_intersects() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let b = Rect {
            x: 50,
            y: 50,
            w: 100,
            h: 100,
        };
        assert!(a.intersects(&b));

        let c = Rect {
            x: 100,
            y: 0,
            w: 50,
            h: 50,
        }; // touching right edge, no overlap
        assert!(!a.intersects(&c));

        let d = Rect {
            x: 200,
            y: 200,
            w: 10,
            h: 10,
        }; // completely separate
        assert!(!a.intersects(&d));
    }

    #[test]
    fn rect_merge() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let b = Rect {
            x: 50,
            y: 50,
            w: 100,
            h: 100,
        };
        let m = a.merge(&b);
        assert_eq!(
            m,
            Rect {
                x: 0,
                y: 0,
                w: 150,
                h: 150
            }
        );
    }

    #[test]
    fn rect_merge_disjoint() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let b = Rect {
            x: 90,
            y: 90,
            w: 10,
            h: 10,
        };
        let m = a.merge(&b);
        assert_eq!(
            m,
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100
            }
        );
    }

    #[test]
    fn display_id_newtype() {
        let id = DisplayId(42);
        assert_eq!(id.0, 42);
        assert_eq!(id, DisplayId(42));
        assert_ne!(id, DisplayId(0));
    }

    #[test]
    fn codec_id_variants() {
        let codecs = [CodecId::H264, CodecId::H265, CodecId::Av1];
        for c in codecs {
            // Each variant is distinct and copyable.
            let c2 = c;
            assert_eq!(c, c2);
        }
        assert_ne!(CodecId::H264, CodecId::H265);
        assert_ne!(CodecId::H265, CodecId::Av1);
    }

    #[test]
    fn quality_tier() {
        assert_ne!(QualityTier::Normal, QualityTier::Preview);
        let t = QualityTier::Preview;
        let t2 = t; // Copy
        assert_eq!(t, t2);
    }

    #[test]
    fn region_encoding_variants() {
        let video = RegionEncoding::Video {
            codec: CodecId::H264,
            is_keyframe: true,
        };
        let lossless = RegionEncoding::Lossless {
            format: LosslessFormat::CpuQoi,
        };
        let damage = RegionEncoding::DamageRect;
        let unchanged = RegionEncoding::Unchanged;

        assert_ne!(video, lossless);
        assert_ne!(damage, unchanged);

        // Pattern-match to verify fields survive Clone.
        if let RegionEncoding::Video { codec, is_keyframe } = video.clone() {
            assert_eq!(codec, CodecId::H264);
            assert!(is_keyframe);
        } else {
            panic!("expected Video variant");
        }
    }
}
