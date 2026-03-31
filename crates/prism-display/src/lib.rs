pub mod types;
pub mod frame;
pub mod ring;
pub mod capture;
pub mod classify;
pub mod damage;
pub mod degradation;
pub mod hysteresis;
pub mod protocol;
pub mod cursor;
pub mod pacing;
pub mod encode_config;
pub mod packet;
pub mod atlas;
pub mod encode_queue;
pub mod window_event;
pub mod encoder_backend;
pub mod keyframe;

// Re-export all public items from the two "flat" foundational modules.
pub use types::*;
pub use frame::*;

// Targeted re-exports from the remaining modules.
pub use ring::FrameRing;
pub use capture::{
    CaptureConfig, CaptureMode, CursorCapture, MonitorInfo, DisplayConfig,
    CaptureError, PlatformCapture,
};
pub use classify::RegionType;
pub use damage::{
    rects_within_threshold, merge_damage_rects,
    macroblock_snap, macroblock_snap_16, superblock_snap_64,
};
pub use degradation::{DegradationLevel, DegradationLadder};
pub use hysteresis::{Hysteresis, LevelChange, UserConstraints};
pub use protocol::{
    FrameGapDetector,
    MSG_REGION_MAP, MSG_SLICE, MSG_CURSOR_SHAPE,
    MSG_CURSOR_POSITION, MSG_IDR_REQUEST, MSG_QUALITY_HINT,
};
pub use cursor::{CursorShape, CursorPosition, CursorManager};
pub use pacing::{InputTriggerCoalescer, FramePacer};
pub use encode_config::{
    EncoderConfig, EncoderPreset, KeyframeInterval, SliceMode,
    RateControlHinter, ComplexityEstimate,
};
pub use packet::{SlicePayloadHeader, SLICE_HEADER_SIZE};
pub use atlas::{RegionKey, StaticDecision, StaticAtlasTracker};
// encode_queue, window_event, encoder_backend, keyframe re-exports added in Task 14
