// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use bytes::Bytes;

use crate::types::{DisplayId, QualityTier, Rect, RegionEncoding, SharedTexture};

// Re-export RegionType so callers can use crate::frame::RegionType.
pub use crate::classify::RegionType;

/// A raw frame captured from a display, before any encode work.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// GPU texture containing the captured pixels.
    pub texture: SharedTexture,
    /// Changed regions since the previous frame (may be empty for full-frame captures).
    pub damage_rects: Vec<Rect>,
    /// Which display this frame came from.
    pub display_id: DisplayId,
    /// Monotonic capture timestamp in microseconds.
    pub capture_time_us: u64,
    /// Sequence number assigned by the capture pipeline; monotonically increasing.
    pub frame_seq: u32,
    /// True when capture was triggered by user input (mouse / keyboard event).
    pub is_input_triggered: bool,
    /// True when this frame was captured speculatively ahead of a scheduled interval.
    pub is_speculative: bool,
}

/// Metadata attached to an encoded frame for the network layer.
#[derive(Debug, Clone)]
pub struct FrameMetadata {
    pub display_id: DisplayId,
    /// Capture timestamp of the source frame (microseconds).
    pub capture_time_us: u64,
    /// True when this is a preview-quality encode.
    pub is_preview: bool,
    /// If set, this frame supersedes an earlier frame with the given sequence number.
    pub replaces_seq: Option<u32>,
    /// Total number of regions included in this frame.
    pub total_regions: u8,
}

/// A unit of encode work dispatched to an encoder backend.
#[derive(Debug, Clone)]
pub struct EncodeJob {
    pub frame_seq: u32,
    pub display_id: DisplayId,
    /// Subrect of the display being encoded.
    pub region_rect: Rect,
    /// Content classification used to pick encode parameters.
    pub region_type: RegionType,
    /// Source texture for this region.
    pub texture: SharedTexture,
    /// Target output bitrate in bits per second.
    pub target_bitrate: u64,
    /// Force an IDR/keyframe regardless of the normal keyframe interval.
    pub force_keyframe: bool,
    pub quality_tier: QualityTier,
    /// Number of regions expected in this frame (for reassembly).
    pub expected_regions: usize,
    pub frame_meta: FrameMetadata,
}

/// A single contiguous block of encoded bitstream belonging to one region.
#[derive(Debug, Clone)]
pub struct EncodedSlice {
    /// Zero-based index of this slice within the region.
    pub slice_index: u8,
    /// Total slice count for this region.
    pub total_slices: u8,
    /// Raw encoded bytes.
    pub data: Bytes,
}

/// The fully encoded result for one display region.
#[derive(Debug, Clone)]
pub struct EncodedRegion {
    /// Coordinates of this region within the display.
    pub rect: Rect,
    /// Encoding strategy that was applied.
    pub encoding: RegionEncoding,
    /// Decoder slot index on the remote side for state continuity.
    pub decoder_slot: u8,
    /// Ordered list of slices making up the bitstream.
    pub slices: Vec<EncodedSlice>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodecId, DisplayId, RegionEncoding, TextureFormat};

    fn make_texture() -> SharedTexture {
        SharedTexture {
            handle: 0xDEAD_BEEF,
            width: 1920,
            height: 1080,
            format: TextureFormat::Nv12,
        }
    }

    fn make_meta(is_preview: bool) -> FrameMetadata {
        FrameMetadata {
            display_id: DisplayId(1),
            capture_time_us: 1_000_000,
            is_preview,
            replaces_seq: None,
            total_regions: 3,
        }
    }

    #[test]
    fn encoded_slice_data() {
        let raw: &[u8] = &[0x00, 0x00, 0x00, 0x01, 0x65]; // NAL start + IDR type byte
        let slice = EncodedSlice {
            slice_index: 0,
            total_slices: 1,
            data: Bytes::copy_from_slice(raw),
        };
        assert_eq!(slice.slice_index, 0);
        assert_eq!(slice.total_slices, 1);
        assert_eq!(&slice.data[..], raw);

        // Cheap clone — Bytes shares the allocation.
        let clone = slice.clone();
        assert_eq!(clone.data, slice.data);
    }

    #[test]
    fn frame_metadata_preview() {
        let meta = make_meta(true);
        assert!(meta.is_preview);
        assert_eq!(meta.total_regions, 3);
        assert!(meta.replaces_seq.is_none());

        let with_replacement = FrameMetadata { replaces_seq: Some(7), ..meta.clone() };
        assert_eq!(with_replacement.replaces_seq, Some(7));
    }

    #[test]
    fn encoded_region_keyframe() {
        let region = EncodedRegion {
            rect: Rect { x: 0, y: 0, w: 960, h: 540 },
            encoding: RegionEncoding::Video { codec: CodecId::H265, is_keyframe: true },
            decoder_slot: 2,
            slices: vec![EncodedSlice {
                slice_index: 0,
                total_slices: 1,
                data: Bytes::from_static(b"\x00\x00\x00\x01"),
            }],
        };

        assert_eq!(region.decoder_slot, 2);
        assert_eq!(region.slices.len(), 1);

        match &region.encoding {
            RegionEncoding::Video { codec, is_keyframe } => {
                assert_eq!(*codec, CodecId::H265);
                assert!(*is_keyframe);
            }
            other => panic!("expected Video encoding, got {:?}", other),
        }

        // EncodeJob round-trip check
        let job = EncodeJob {
            frame_seq: 42,
            display_id: DisplayId(1),
            region_rect: region.rect,
            region_type: RegionType::Video,
            texture: make_texture(),
            target_bitrate: 10_000_000,
            force_keyframe: true,
            quality_tier: QualityTier::Normal,
            expected_regions: 1,
            frame_meta: make_meta(false),
        };
        assert_eq!(job.frame_seq, 42);
        assert!(job.force_keyframe);
        assert_eq!(job.region_type, RegionType::Video);
    }
}
