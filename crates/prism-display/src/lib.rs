// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod atlas;
pub mod capture;
pub mod classify;
pub mod cursor;
pub mod damage;
pub mod degradation;
pub mod encode_config;
pub mod encode_queue;
pub mod encoder_backend;
pub mod frame;
pub mod hysteresis;
pub mod keyframe;
pub mod pacing;
pub mod packet;
pub mod protocol;
pub mod ring;
pub mod types;
pub mod window_event;

// Re-export all public items from the two "flat" foundational modules.
pub use frame::*;
pub use types::*;

// Targeted re-exports from the remaining modules.
pub use atlas::{RegionKey, StaticAtlasTracker, StaticDecision};
pub use capture::{
    CaptureConfig, CaptureError, CaptureMode, CursorCapture, DisplayConfig, MonitorInfo,
    PlatformCapture,
};
pub use classify::RegionType;
pub use cursor::{CursorManager, CursorPosition, CursorShape};
pub use damage::{
    macroblock_snap, macroblock_snap_16, merge_damage_rects, rects_within_threshold,
    superblock_snap_64,
};
pub use degradation::{DegradationLadder, DegradationLevel};
pub use encode_config::{
    ComplexityEstimate, EncoderConfig, EncoderPreset, KeyframeInterval, RateControlHinter,
    SliceMode,
};
pub use encode_queue::EncodeQueue;
pub use encoder_backend::{EncoderBackend, select_best_encoder};
pub use hysteresis::{Hysteresis, LevelChange, UserConstraints};
pub use keyframe::KeyframeDecider;
pub use pacing::{FramePacer, InputTriggerCoalescer};
pub use packet::{SLICE_HEADER_SIZE, SlicePayloadHeader};
pub use protocol::{
    FrameGapDetector, MSG_CURSOR_POSITION, MSG_CURSOR_SHAPE, MSG_IDR_REQUEST, MSG_QUALITY_HINT,
    MSG_REGION_MAP, MSG_SLICE,
};
pub use ring::FrameRing;
pub use window_event::WindowEvent;
