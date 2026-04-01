# Plan 5: Display Engine Implementation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-display` crate providing the display pipeline types (capture/classify/encode/send frame types), lock-free SPSC ring buffer, platform capture trait, Tier 1 region classification, damage rect merging, profile-specific degradation ladders with hysteresis, display channel protocol with frame gap detection, cursor management, input-triggered capture coalescing, adaptive frame pacing, encoder configuration types, rate control hinting, and self-describing slice packet headers.

**Architecture:** `prism-display` defines the entire capture→classify→encode→send pipeline as types and pure algorithms. Platform-specific implementations (DDA, WGC, NVENC, etc.) are behind traits — actual platform code is deferred to integration. The `FrameRing<T>` is a lock-free SPSC ring buffer for capture→classify handoff. The `DegradationLadder` consumes `QualityRecommendation` from prism-transport and maps it to profile-specific levels with hysteresis. All classification, degradation, pacing, and gap detection logic is pure computation, fully unit-testable.

**Tech Stack:** `bytes` (packet data), `prism-protocol` (headers, channels), `prism-transport` (QualityRecommendation), `prism-metrics` (recording), `serde` (serializable configs)

**Spec refs:**
- Display Engine: `docs/superpowers/specs/2026-03-30-display-engine-design.md` (all sections)
- Architecture: `docs/superpowers/specs/2026-03-30-prism-architecture-design.md` (R20-R23, R31-R36, R40, R42-R46)

---

## File Structure

```
PRISM/
  crates/
    prism-display/
      Cargo.toml
      src/
        lib.rs                      # re-exports
        types.rs                    # Rect, DisplayId, CodecId, TextureFormat, QualityTier,
                                    # RegionEncoding, LosslessFormat, SharedTexture
        frame.rs                    # CapturedFrame, EncodeJob, EncodedRegion, EncodedSlice,
                                    # FrameMetadata
        ring.rs                     # FrameRing<T> (lock-free SPSC)
        capture.rs                  # PlatformCapture trait, CaptureConfig, MonitorInfo,
                                    # CaptureMode, CursorCapture
        classify.rs                 # RegionClassifier trait, ClassifiedRegion, RegionType,
                                    # Tier1Classifier, UpdateFrequency
        damage.rs                   # DamageRectMerger, macroblock_snap
        degradation.rs              # DegradationLevel, DegradationLadder, Gaming/Coding profiles
        hysteresis.rs               # Hysteresis, UserConstraints, LevelChange
        protocol.rs                 # Display channel message types, FrameGapDetector
        cursor.rs                   # CursorManager, CursorShape, CursorPosition
        pacing.rs                   # InputTriggerCoalescer, FramePacer
        encode_config.rs            # EncoderConfig, EncoderPreset, KeyframeInterval, SliceMode,
                                    # RateControlHinter, ComplexityEstimate
        packet.rs                   # SlicePayloadHeader (24B with cursor piggybacking)
        atlas.rs                    # StaticAtlasTracker, StaticDecision, LRU eviction
        encode_queue.rs             # EncodeQueue (priority: high before normal)
        window_event.rs             # WindowEvent enum, speculative IDR types
        encoder_backend.rs          # EncoderBackend enum, HwEncoder trait
        keyframe.rs                 # KeyframeDecider (content-adaptive IDR logic)
```

---

## Task 1: Crate Setup + Core Types

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-display/Cargo.toml`
- Create: `crates/prism-display/src/lib.rs`
- Create: `crates/prism-display/src/types.rs`
- Create: all placeholder source files

- [ ] **Step 1: Update workspace root Cargo.toml**

Add `"crates/prism-display"` to members. Add `prism-display = { path = "crates/prism-display" }` to workspace.dependencies.

- [ ] **Step 2: Create crates/prism-display/Cargo.toml**

```toml
[package]
name = "prism-display"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
prism-protocol = { workspace = true }
prism-transport = { workspace = true }
prism-metrics = { workspace = true }
bytes = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 3: Create lib.rs + all placeholder files**

`lib.rs`:
```rust
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
```

Create all 13 placeholder source files with just a comment each.

- [ ] **Step 4: Write failing tests for core types**

`types.rs`:
```rust
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_area() {
        let r = Rect { x: 10, y: 20, w: 100, h: 50 };
        assert_eq!(r.area(), 5000);
    }

    #[test]
    fn rect_contains_point() {
        let r = Rect { x: 10, y: 20, w: 100, h: 50 };
        assert!(r.contains(50, 40));
        assert!(!r.contains(5, 40));
        assert!(!r.contains(50, 80));
    }

    #[test]
    fn rect_intersects() {
        let a = Rect { x: 0, y: 0, w: 100, h: 100 };
        let b = Rect { x: 50, y: 50, w: 100, h: 100 };
        assert!(a.intersects(&b));
        let c = Rect { x: 200, y: 200, w: 10, h: 10 };
        assert!(!a.intersects(&c));
    }

    #[test]
    fn rect_merge() {
        let a = Rect { x: 0, y: 0, w: 50, h: 50 };
        let b = Rect { x: 30, y: 30, w: 50, h: 50 };
        let merged = a.merge(&b);
        assert_eq!(merged, Rect { x: 0, y: 0, w: 80, h: 80 });
    }

    #[test]
    fn display_id_newtype() {
        let id = DisplayId(1);
        assert_eq!(id.0, 1);
    }

    #[test]
    fn codec_id_variants() {
        assert_ne!(CodecId::H264, CodecId::H265);
    }

    #[test]
    fn quality_tier_default_is_normal() {
        let tier = QualityTier::Normal;
        assert_eq!(tier, QualityTier::Normal);
    }

    #[test]
    fn region_encoding_variants() {
        let enc = RegionEncoding::Video { codec: CodecId::H264, is_keyframe: true };
        assert!(matches!(enc, RegionEncoding::Video { is_keyframe: true, .. }));
    }
}
```

- [ ] **Step 5: Implement core types**

```rust
/// Rectangle in pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn area(&self) -> u64 { self.w as u64 * self.h as u64 }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w as i32
            && py >= self.y && py < self.y + self.h as i32
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w as i32
            && self.x + self.w as i32 > other.x
            && self.y < other.y + other.h as i32
            && self.y + self.h as i32 > other.y
    }

    pub fn merge(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.w as i32).max(other.x + other.w as i32);
        let bottom = (self.y + self.h as i32).max(other.y + other.h as i32);
        Rect { x, y, w: (right - x) as u32, h: (bottom - y) as u32 }
    }
}

/// Display identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayId(pub u32);

/// Video codec identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodecId {
    H264,
    H265,
    Av1,
}

/// GPU texture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureFormat {
    Bgra8,
    Nv12,
    P010,
}

/// Encode quality tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityTier {
    Normal,
    Preview,
}

/// How a region was encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionEncoding {
    Video { codec: CodecId, is_keyframe: bool },
    Lossless { format: LosslessFormat },
    DamageRect,
    Unchanged,
}

/// Lossless encoding format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LosslessFormat {
    HwH264Lossless,
    HwH265Lossless,
    CpuQoi,
    Delta,
}

/// Opaque shared GPU texture handle. Platform-specific internals are behind the handle.
#[derive(Debug, Clone)]
pub struct SharedTexture {
    pub handle: u64,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}
```

- [ ] **Step 6: Verify, commit**

```bash
cargo test -p prism-display
git add crates/prism-display/ Cargo.toml
git commit -m "feat(display): scaffold crate, Rect, DisplayId, CodecId, core types"
```

---

## Task 2: Frame Types

**Files:**
- Modify: `crates/prism-display/src/frame.rs`

- [ ] **Step 1: Write tests + implement frame types**

```rust
use bytes::Bytes;
use crate::types::*;

/// Stage 1 output: captured frame with damage information.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub texture: SharedTexture,
    pub damage_rects: Vec<Rect>,
    pub display_id: DisplayId,
    pub capture_time_us: u64,
    pub frame_seq: u32,
    pub is_input_triggered: bool,
    pub is_speculative: bool,
}

/// Stage 2→3: encode job for a single classified region.
#[derive(Debug, Clone)]
pub struct EncodeJob {
    pub frame_seq: u32,
    pub display_id: DisplayId,
    pub region_rect: Rect,
    pub region_type: RegionType,
    pub texture: SharedTexture,
    pub target_bitrate: u64,
    pub force_keyframe: bool,
    pub quality_tier: QualityTier,
    pub expected_regions: usize,
    pub frame_meta: FrameMetadata,
}

/// Region classification type (used by EncodeJob).
pub use crate::classify::RegionType;

/// Stage 3 output: encoded region with slices.
#[derive(Debug, Clone)]
pub struct EncodedRegion {
    pub rect: Rect,
    pub encoding: RegionEncoding,
    pub decoder_slot: u8,
    pub slices: Vec<EncodedSlice>,
}

/// A single independently decodable slice.
#[derive(Debug, Clone)]
pub struct EncodedSlice {
    pub slice_index: u8,
    pub total_slices: u8,
    pub data: Bytes,
}

/// Per-frame metadata carried through the pipeline.
#[derive(Debug, Clone)]
pub struct FrameMetadata {
    pub display_id: DisplayId,
    pub capture_time_us: u64,
    pub is_preview: bool,
    pub replaces_seq: Option<u32>,
    pub total_regions: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_slice_data() {
        let slice = EncodedSlice {
            slice_index: 0, total_slices: 4,
            data: Bytes::from_static(b"encoded_data"),
        };
        assert_eq!(slice.data.len(), 12);
        assert_eq!(slice.total_slices, 4);
    }

    #[test]
    fn frame_metadata_preview() {
        let meta = FrameMetadata {
            display_id: DisplayId(0),
            capture_time_us: 1000,
            is_preview: true,
            replaces_seq: None,
            total_regions: 3,
        };
        assert!(meta.is_preview);
    }

    #[test]
    fn encoded_region_keyframe() {
        let region = EncodedRegion {
            rect: Rect { x: 0, y: 0, w: 1920, h: 1080 },
            encoding: RegionEncoding::Video { codec: CodecId::H264, is_keyframe: true },
            decoder_slot: 0,
            slices: vec![],
        };
        assert!(matches!(region.encoding, RegionEncoding::Video { is_keyframe: true, .. }));
    }
}
```

Note: `RegionType` is re-exported from `classify.rs`. Add a temporary `pub enum RegionType { Text, Video, Static, Uncertain }` in classify.rs for this task to compile. It will be properly implemented in Task 5.

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-display/src/frame.rs crates/prism-display/src/classify.rs
git commit -m "feat(display): frame pipeline types (CapturedFrame, EncodeJob, EncodedRegion)"
```

---

## Task 3: FrameRing (Lock-Free SPSC)

**Files:**
- Modify: `crates/prism-display/src/ring.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_single() {
        let ring = FrameRing::new(4);
        assert!(ring.try_push(42).is_some());
        assert_eq!(ring.try_pop(), Some(42));
    }

    #[test]
    fn empty_pop_returns_none() {
        let ring: FrameRing<i32> = FrameRing::new(4);
        assert_eq!(ring.try_pop(), None);
    }

    #[test]
    fn full_ring_rejects_push() {
        let ring = FrameRing::new(2);
        assert!(ring.try_push(1).is_some());
        assert!(ring.try_push(2).is_some());
        assert!(ring.try_push(3).is_none()); // full
    }

    #[test]
    fn fifo_ordering() {
        let ring = FrameRing::new(4);
        ring.try_push(1);
        ring.try_push(2);
        ring.try_push(3);
        assert_eq!(ring.try_pop(), Some(1));
        assert_eq!(ring.try_pop(), Some(2));
        assert_eq!(ring.try_pop(), Some(3));
    }

    #[test]
    fn wraparound() {
        let ring = FrameRing::new(2);
        ring.try_push(1);
        ring.try_push(2);
        ring.try_pop(); // free slot
        assert!(ring.try_push(3).is_some()); // wraps around
        assert_eq!(ring.try_pop(), Some(2));
        assert_eq!(ring.try_pop(), Some(3));
    }

    #[test]
    fn len_tracking() {
        let ring = FrameRing::new(4);
        assert_eq!(ring.len(), 0);
        assert!(ring.is_empty());
        ring.try_push(1);
        ring.try_push(2);
        assert_eq!(ring.len(), 2);
        ring.try_pop();
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn concurrent_producer_consumer() {
        use std::sync::Arc;
        use std::thread;

        let ring = Arc::new(FrameRing::new(64));
        let producer = ring.clone();
        let consumer = ring.clone();

        let prod_handle = thread::spawn(move || {
            let mut pushed = 0;
            for i in 0..1000 {
                while producer.try_push(i).is_none() {
                    std::hint::spin_loop();
                }
                pushed += 1;
            }
            pushed
        });

        let cons_handle = thread::spawn(move || {
            let mut received = Vec::new();
            while received.len() < 1000 {
                if let Some(v) = consumer.try_pop() {
                    received.push(v);
                } else {
                    std::hint::spin_loop();
                }
            }
            received
        });

        let pushed = prod_handle.join().unwrap();
        let received = cons_handle.join().unwrap();
        assert_eq!(pushed, 1000);
        assert_eq!(received.len(), 1000);
        // Verify FIFO ordering
        for (i, &v) in received.iter().enumerate() {
            assert_eq!(v, i as i32);
        }
    }
}
```

- [ ] **Step 2: Implement FrameRing**

```rust
/// Cache-line padding to prevent false sharing between producer and consumer.
#[repr(align(64))]
struct CacheAligned(AtomicUsize);

impl CacheAligned {
    fn new(v: usize) -> Self { Self(AtomicUsize::new(v)) }
}

/// Lock-free SPSC ring buffer. Pre-allocated, zero-allocation during operation.
/// Single producer calls try_push, single consumer calls try_pop.
/// write_pos and read_pos are on separate cache lines (64B aligned) to eliminate
/// false sharing — producer and consumer never contend on the same cache line.
pub struct FrameRing<T> {
    slots: Box<[UnsafeCell<Option<T>>]>,
    capacity: usize,
    write_pos: CacheAligned,  // producer-owned, on its own cache line
    read_pos: CacheAligned,   // consumer-owned, on its own cache line
}

// SAFETY: Single producer + single consumer. Atomic indices prevent data races.
// Each slot is accessed by at most one thread at a time:
// - Producer writes to slot at write_pos (after verifying it's empty)
// - Consumer reads from slot at read_pos (after verifying it's occupied)
unsafe impl<T: Send> Send for FrameRing<T> {}
unsafe impl<T: Send> Sync for FrameRing<T> {}

impl<T> FrameRing<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        let slots: Vec<UnsafeCell<Option<T>>> = (0..capacity)
            .map(|_| UnsafeCell::new(None))
            .collect();
        Self {
            slots: slots.into_boxed_slice(),
            capacity,
            write_pos: CacheAligned::new(0),
            read_pos: CacheAligned::new(0),
        }
    }

    /// Try to push. Returns None if full (consumer slow — caller should DROP the frame).
    pub fn try_push(&self, item: T) -> Option<()> {
        let write = self.write_pos.0.load(Ordering::Relaxed);
        let read = self.read_pos.0.load(Ordering::Acquire);
        if write.wrapping_sub(read) >= self.capacity {
            return None; // full
        }
        let idx = write % self.capacity;
        // SAFETY: Only producer writes to this slot. Read barrier above ensures
        // consumer has finished reading before we overwrite.
        unsafe { *self.slots[idx].get() = Some(item); }
        self.write_pos.0.store(write.wrapping_add(1), Ordering::Release);
        Some(())
    }

    /// Try to pop. Returns None if empty.
    pub fn try_pop(&self) -> Option<T> {
        let read = self.read_pos.0.load(Ordering::Relaxed);
        let write = self.write_pos.0.load(Ordering::Acquire);
        if read == write {
            return None; // empty
        }
        let idx = read % self.capacity;
        // SAFETY: Only consumer reads from this slot. Write barrier above ensures
        // producer has finished writing before we read.
        let item = unsafe { (*self.slots[idx].get()).take() };
        self.read_pos.0.store(read.wrapping_add(1), Ordering::Release);
        item
    }

    pub fn len(&self) -> usize {
        let write = self.write_pos.0.load(Ordering::Relaxed);
        let read = self.read_pos.0.load(Ordering::Relaxed);
        write.wrapping_sub(read)
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }
    pub fn is_full(&self) -> bool { self.len() >= self.capacity }
}
```

- [ ] **Step 3: Verify, commit**

```bash
cargo test -p prism-display -- ring
git add crates/prism-display/src/ring.rs
git commit -m "feat(display): FrameRing lock-free SPSC ring buffer"
```

---

## Task 4: PlatformCapture Trait + Types

**Files:**
- Modify: `crates/prism-display/src/capture.rs`

- [ ] **Step 1: Implement capture trait and types**

```rust
use crate::types::{DisplayId, Rect};

/// How to capture.
#[derive(Debug, Clone)]
pub enum CaptureMode {
    FullDesktop,
    Window { hwnd: u64 },
    Virtual { resolution: (u32, u32), refresh_rate: u8 },
}

/// Whether cursor is embedded in frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorCapture {
    Embedded,
    Separate,
    None,
}

/// Capture configuration.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub display_id: DisplayId,
    pub capture_mode: CaptureMode,
    pub cursor: CursorCapture,
}

/// Information about a connected monitor.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub display_id: DisplayId,
    pub name: String,
    pub resolution: (u32, u32),
    pub position: (i32, i32),
    pub scale_factor: f32,
    pub refresh_rate: u8,
    pub primary: bool,
    pub is_virtual: bool,
}

/// Display configuration for virtual displays.
#[derive(Debug, Clone)]
pub struct DisplayConfig {
    pub resolution: (u32, u32),
    pub refresh_rate: u8,
}

/// Capture errors.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture not supported on this platform")]
    Unsupported,
    #[error("display not found: {0:?}")]
    DisplayNotFound(DisplayId),
    #[error("capture failed: {0}")]
    Failed(String),
}

/// Platform-specific capture abstraction. Implementations are platform-specific (DDA, WGC, etc.).
pub trait PlatformCapture: Send + 'static {
    fn start(&mut self, config: CaptureConfig) -> Result<(), CaptureError>;
    fn stop(&mut self);
    fn trigger_capture(&self);
    fn next_frame(&mut self) -> Option<crate::frame::CapturedFrame>;
    fn enumerate_monitors(&self) -> Vec<MonitorInfo>;
    fn create_virtual_display(&mut self, config: DisplayConfig) -> Result<DisplayId, CaptureError>;
    fn destroy_virtual_display(&mut self, id: DisplayId) -> Result<(), CaptureError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_mode_variants() {
        let mode = CaptureMode::FullDesktop;
        assert!(matches!(mode, CaptureMode::FullDesktop));
    }

    #[test]
    fn monitor_info_defaults() {
        let info = MonitorInfo {
            display_id: DisplayId(0), name: "Primary".into(),
            resolution: (1920, 1080), position: (0, 0),
            scale_factor: 1.0, refresh_rate: 60, primary: true, is_virtual: false,
        };
        assert!(info.primary);
        assert_eq!(info.refresh_rate, 60);
    }

    #[test]
    fn capture_error_display() {
        let err = CaptureError::DisplayNotFound(DisplayId(5));
        assert!(format!("{err}").contains("5"));
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-display/src/capture.rs
git commit -m "feat(display): PlatformCapture trait, CaptureConfig, MonitorInfo"
```

---

## Task 5: RegionClassifier + Tier1Classifier

**Files:**
- Modify: `crates/prism-display/src/classify.rs`

- [ ] **Step 1: Write failing tests**

```rust
use crate::types::{Rect, DisplayId};
use std::collections::HashMap;
use std::time::Instant;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_frequency_to_region_type() {
        assert_eq!(RegionType::from_frequency(UpdateFrequency::Static), RegionType::Static);
        assert_eq!(RegionType::from_frequency(UpdateFrequency::Low), RegionType::Text);
        assert_eq!(RegionType::from_frequency(UpdateFrequency::High), RegionType::Video);
        assert_eq!(RegionType::from_frequency(UpdateFrequency::Unknown), RegionType::Uncertain);
    }

    #[test]
    fn tier1_single_window_classifies() {
        let mut classifier = Tier1Classifier::new();
        let windows = vec![
            WindowActivity { hwnd: 1, rect: Rect { x: 0, y: 0, w: 800, h: 600 }, frequency: UpdateFrequency::Low },
        ];
        let regions = classifier.classify(&windows, 1920, 1080);
        // Should have the window region + background region
        assert!(regions.iter().any(|r| r.classification == RegionType::Text));
    }

    #[test]
    fn tier1_video_window_classifies_as_video() {
        let mut classifier = Tier1Classifier::new();
        let windows = vec![
            WindowActivity { hwnd: 1, rect: Rect { x: 0, y: 0, w: 1920, h: 1080 }, frequency: UpdateFrequency::High },
        ];
        let regions = classifier.classify(&windows, 1920, 1080);
        assert!(regions.iter().any(|r| r.classification == RegionType::Video));
    }

    #[test]
    fn tier1_uncovered_area_is_static() {
        let mut classifier = Tier1Classifier::new();
        let windows = vec![
            WindowActivity { hwnd: 1, rect: Rect { x: 0, y: 0, w: 100, h: 100 }, frequency: UpdateFrequency::Low },
        ];
        let regions = classifier.classify(&windows, 1920, 1080);
        // Should have a static region for uncovered desktop
        assert!(regions.iter().any(|r| r.classification == RegionType::Static));
    }

    #[test]
    fn decoder_slot_assignment() {
        assert_eq!(RegionType::Video.decoder_slot(), 0);
        assert_eq!(RegionType::Text.decoder_slot(), 1);
        assert_eq!(RegionType::Uncertain.decoder_slot(), 1);
        assert_eq!(RegionType::Static.decoder_slot(), 2);
    }

    #[test]
    fn classified_region_confidence() {
        let region = ClassifiedRegion {
            rect: Rect { x: 0, y: 0, w: 100, h: 100 },
            classification: RegionType::Text,
            confidence: 0.8,
            decoder_slot: 1,
        };
        assert!(region.confidence > 0.5);
    }
}
```

- [ ] **Step 2: Implement classify module**

Replace the temporary classify.rs with the full implementation:

```rust
/// Region content classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegionType {
    Text,
    Video,
    Static,
    Uncertain,
}

impl RegionType {
    pub fn from_frequency(freq: UpdateFrequency) -> Self {
        match freq {
            UpdateFrequency::Static => RegionType::Static,
            UpdateFrequency::Low => RegionType::Text,
            UpdateFrequency::High => RegionType::Video,
            UpdateFrequency::Unknown => RegionType::Uncertain,
        }
    }

    pub fn decoder_slot(&self) -> u8 {
        match self {
            RegionType::Video => 0,
            RegionType::Text | RegionType::Uncertain => 1,
            RegionType::Static => 2,
        }
    }
}

/// Window update frequency (from WGC metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFrequency {
    Static,
    Low,
    High,
    Unknown,
}

/// A classified screen region.
#[derive(Debug, Clone)]
pub struct ClassifiedRegion {
    pub rect: Rect,
    pub classification: RegionType,
    pub confidence: f32,
    pub decoder_slot: u8,
}

/// Window activity data (from WGC tracker).
#[derive(Debug, Clone)]
pub struct WindowActivity {
    pub hwnd: u64,
    pub rect: Rect,
    pub frequency: UpdateFrequency,
}

/// Tier 1 classifier: window-level classification using WGC metadata.
pub struct Tier1Classifier {
    history: HashMap<u64, (RegionType, Instant)>,
}

impl Tier1Classifier {
    pub fn new() -> Self {
        Self { history: HashMap::new() }
    }

    /// Classify the screen into regions based on window activity.
    pub fn classify(&mut self, windows: &[WindowActivity], screen_w: u32, screen_h: u32) -> Vec<ClassifiedRegion> {
        let mut regions = Vec::new();

        for window in windows {
            let classification = RegionType::from_frequency(window.frequency);
            let confidence = self.compute_confidence(window.hwnd, classification);

            regions.push(ClassifiedRegion {
                rect: window.rect,
                classification,
                confidence,
                decoder_slot: classification.decoder_slot(),
            });

            self.history.insert(window.hwnd, (classification, Instant::now()));
        }

        // Add static background for uncovered areas
        // Simplified: just add a full-screen Static region at lowest z-order
        // Actual implementation would compute the uncovered area
        if windows.is_empty() || !self.covers_full_screen(windows, screen_w, screen_h) {
            regions.push(ClassifiedRegion {
                rect: Rect { x: 0, y: 0, w: screen_w, h: screen_h },
                classification: RegionType::Static,
                confidence: 1.0,
                decoder_slot: RegionType::Static.decoder_slot(),
            });
        }

        regions
    }

    fn compute_confidence(&self, hwnd: u64, current: RegionType) -> f32 {
        match self.history.get(&hwnd) {
            Some((prev, since)) if *prev == current => {
                let stable_secs = since.elapsed().as_secs_f32();
                (stable_secs / 5.0).min(1.0) // full confidence after 5s
            }
            _ => 0.5, // new or changed classification
        }
    }

    fn covers_full_screen(&self, windows: &[WindowActivity], w: u32, h: u32) -> bool {
        // Simplified check: does any window cover the full screen?
        windows.iter().any(|win| {
            win.rect.x <= 0 && win.rect.y <= 0
                && win.rect.w >= w && win.rect.h >= h
        })
    }
}

impl Default for Tier1Classifier {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-display/src/classify.rs
git commit -m "feat(display): RegionType, Tier1Classifier with window-level classification"
```

---

## Task 6: Damage Rect Merging + Macroblock Snapping

**Files:**
- Modify: `crates/prism-display/src/damage.rs`

- [ ] **Step 1: Write failing tests**

```rust
use crate::types::Rect;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_rects_returns_empty() {
        assert!(merge_damage_rects(&[], 64).is_empty());
    }

    #[test]
    fn single_rect_unchanged() {
        let rects = vec![Rect { x: 10, y: 20, w: 100, h: 50 }];
        let merged = merge_damage_rects(&rects, 64);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], rects[0]);
    }

    #[test]
    fn adjacent_rects_merge() {
        let rects = vec![
            Rect { x: 0, y: 0, w: 50, h: 50 },
            Rect { x: 40, y: 0, w: 50, h: 50 }, // overlaps by 10px (< threshold)
        ];
        let merged = merge_damage_rects(&rects, 64);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], Rect { x: 0, y: 0, w: 90, h: 50 });
    }

    #[test]
    fn distant_rects_stay_separate() {
        let rects = vec![
            Rect { x: 0, y: 0, w: 50, h: 50 },
            Rect { x: 500, y: 500, w: 50, h: 50 }, // far away
        ];
        let merged = merge_damage_rects(&rects, 64);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn macroblock_snap_aligns_to_16() {
        let r = Rect { x: 5, y: 7, w: 100, h: 50 };
        let snapped = macroblock_snap(r, 16);
        assert_eq!(snapped.x % 16, 0);
        assert_eq!(snapped.y % 16, 0);
        assert_eq!((snapped.x + snapped.w as i32) % 16, 0);
        assert_eq!((snapped.y + snapped.h as i32) % 16, 0);
        assert!(snapped.x <= r.x);
        assert!(snapped.y <= r.y);
        assert!(snapped.x + snapped.w as i32 >= r.x + r.w as i32);
        assert!(snapped.y + snapped.h as i32 >= r.y + r.h as i32);
    }

    #[test]
    fn macroblock_snap_already_aligned() {
        let r = Rect { x: 16, y: 32, w: 64, h: 48 };
        let snapped = macroblock_snap(r, 16);
        assert_eq!(snapped, r);
    }

    #[test]
    fn superblock_snap_64_for_av1() {
        let r = Rect { x: 10, y: 10, w: 100, h: 100 };
        let snapped = macroblock_snap(r, 64);
        assert_eq!(snapped.x % 64, 0);
        assert_eq!(snapped.y % 64, 0);
        assert_eq!((snapped.x + snapped.w as i32) % 64, 0);
        assert!(snapped.x <= r.x);
    }

    #[test]
    fn many_small_rects_merge_to_few() {
        // Simulate 20 small adjacent rects in a row
        let rects: Vec<Rect> = (0..20).map(|i| Rect { x: i * 30, y: 0, w: 32, h: 32 }).collect();
        let merged = merge_damage_rects(&rects, 64);
        assert!(merged.len() < 5, "20 small rects should merge to <5, got {}", merged.len());
    }
}
```

- [ ] **Step 2: Implement damage module**

```rust
/// Merge adjacent damage rects that are within `threshold` pixels of each other.
pub fn merge_damage_rects(rects: &[Rect], threshold: i32) -> Vec<Rect> {
    if rects.is_empty() { return Vec::new(); }

    let mut merged: Vec<Rect> = Vec::new();

    for &rect in rects {
        let mut was_merged = false;
        for existing in &mut merged {
            if rects_within_threshold(existing, &rect, threshold) {
                *existing = existing.merge(&rect);
                was_merged = true;
                break;
            }
        }
        if !was_merged {
            merged.push(rect);
        }
    }

    // Second pass: merge any overlapping results from first pass
    loop {
        let mut changed = false;
        let mut i = 0;
        while i < merged.len() {
            let mut j = i + 1;
            while j < merged.len() {
                if rects_within_threshold(&merged[i], &merged[j], threshold) {
                    let other = merged.remove(j);
                    merged[i] = merged[i].merge(&other);
                    changed = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
        if !changed { break; }
    }

    merged
}

fn rects_within_threshold(a: &Rect, b: &Rect, threshold: i32) -> bool {
    let a_right = a.x + a.w as i32;
    let a_bottom = a.y + a.h as i32;
    let b_right = b.x + b.w as i32;
    let b_bottom = b.y + b.h as i32;

    let gap_x = if a_right < b.x { b.x - a_right }
        else if b_right < a.x { a.x - b_right }
        else { 0 };
    let gap_y = if a_bottom < b.y { b.y - a_bottom }
        else if b_bottom < a.y { a.y - b_bottom }
        else { 0 };

    gap_x <= threshold && gap_y <= threshold
}

/// Snap a rect to codec block boundaries (expand outward).
/// `alignment`: 16 for H.264/H.265 macroblocks, 64 for AV1 superblocks.
pub fn macroblock_snap(rect: Rect, alignment: i32) -> Rect {
    let mask = !(alignment - 1); // alignment must be power of 2
    let x = rect.x & mask;
    let y = rect.y & mask;
    let right = (rect.x + rect.w as i32 + alignment - 1) & mask;
    let bottom = (rect.y + rect.h as i32 + alignment - 1) & mask;
    Rect { x, y, w: (right - x) as u32, h: (bottom - y) as u32 }
}

/// Convenience: snap to H.264/H.265 16px macroblocks.
pub fn macroblock_snap_16(rect: Rect) -> Rect { macroblock_snap(rect, 16) }

/// Convenience: snap to AV1 64px superblocks.
pub fn superblock_snap_64(rect: Rect) -> Rect { macroblock_snap(rect, 64) }
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-display/src/damage.rs
git commit -m "feat(display): damage rect merging + macroblock snapping"
```

---

## Task 7: DegradationLadder + Profiles

**Files:**
- Modify: `crates/prism-display/src/degradation.rs`

- [ ] **Step 1: Write failing tests**

```rust
use crate::types::CodecId;
use prism_transport::QualityRecommendation;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaming_profile_level0_is_optimal() {
        let ladder = DegradationLadder::gaming();
        let level = &ladder.levels[0];
        assert_eq!(level.max_fps, 120);
        assert!(!level.region_detection);
    }

    #[test]
    fn coding_profile_always_has_region_detection() {
        let ladder = DegradationLadder::coding();
        for level in &ladder.levels {
            assert!(level.region_detection, "Coding level {} should have region detection", level.name);
        }
    }

    #[test]
    fn gaming_drops_resolution_before_fps() {
        let ladder = DegradationLadder::gaming();
        // Level 1 reduces resolution, keeps FPS
        assert_eq!(ladder.levels[1].max_fps, 120);
        assert!(ladder.levels[1].resolution.0 < ladder.levels[0].resolution.0);
    }

    #[test]
    fn coding_drops_fps_before_resolution() {
        let ladder = DegradationLadder::coding();
        // Level 1 keeps resolution similar, may reduce later
        // Level 2 drops FPS
        assert!(ladder.levels[2].max_fps < ladder.levels[0].max_fps);
    }

    #[test]
    fn target_level_for_recommendation() {
        let ladder = DegradationLadder::gaming();
        assert_eq!(ladder.target_level(&QualityRecommendation::Optimal), 0);
        assert!(ladder.target_level(&QualityRecommendation::ConnectionUnusable) > 0);
    }

    #[test]
    fn all_profiles_have_at_least_3_levels() {
        assert!(DegradationLadder::gaming().levels.len() >= 3);
        assert!(DegradationLadder::coding().levels.len() >= 3);
    }

    #[test]
    fn bitrate_decreases_with_level() {
        let ladder = DegradationLadder::gaming();
        for i in 1..ladder.levels.len() {
            assert!(ladder.levels[i].max_bitrate_mbps <= ladder.levels[i-1].max_bitrate_mbps,
                "Level {} bitrate should be <= level {}", i, i-1);
        }
    }
}
```

- [ ] **Step 2: Implement DegradationLadder**

```rust
/// A single degradation level's parameters.
#[derive(Debug, Clone)]
pub struct DegradationLevel {
    pub name: String,
    pub max_bitrate_mbps: u64,
    pub resolution: (u32, u32),
    pub max_fps: u8,
    pub codec: CodecId,
    pub region_detection: bool,
    pub fec_ratio: f32,
}

/// Profile-specific degradation ladder.
pub struct DegradationLadder {
    pub profile_name: String,
    pub levels: Vec<DegradationLevel>,
}

impl DegradationLadder {
    pub fn gaming() -> Self {
        Self {
            profile_name: "Gaming".into(),
            levels: vec![
                DegradationLevel { name: "optimal".into(), max_bitrate_mbps: 80, resolution: (3840, 2160), max_fps: 120, codec: CodecId::H265, region_detection: false, fec_ratio: 0.0 },
                DegradationLevel { name: "reduced_res".into(), max_bitrate_mbps: 40, resolution: (2560, 1440), max_fps: 120, codec: CodecId::H265, region_detection: false, fec_ratio: 0.0 },
                DegradationLevel { name: "reduced_fps".into(), max_bitrate_mbps: 20, resolution: (1920, 1080), max_fps: 60, codec: CodecId::H264, region_detection: false, fec_ratio: 0.0 },
                DegradationLevel { name: "minimum".into(), max_bitrate_mbps: 8, resolution: (1280, 720), max_fps: 30, codec: CodecId::H264, region_detection: false, fec_ratio: 0.10 },
            ],
        }
    }

    pub fn coding() -> Self {
        Self {
            profile_name: "Coding".into(),
            levels: vec![
                DegradationLevel { name: "optimal".into(), max_bitrate_mbps: 20, resolution: (3840, 2160), max_fps: 60, codec: CodecId::H265, region_detection: true, fec_ratio: 0.0 },
                DegradationLevel { name: "reduced_bw".into(), max_bitrate_mbps: 8, resolution: (2560, 1440), max_fps: 60, codec: CodecId::H264, region_detection: true, fec_ratio: 0.0 },
                DegradationLevel { name: "reduced_fps".into(), max_bitrate_mbps: 4, resolution: (1920, 1080), max_fps: 30, codec: CodecId::H264, region_detection: true, fec_ratio: 0.0 },
                DegradationLevel { name: "minimum".into(), max_bitrate_mbps: 1, resolution: (1280, 720), max_fps: 15, codec: CodecId::H264, region_detection: true, fec_ratio: 0.15 },
            ],
        }
    }

    /// Map a quality recommendation to a target degradation level index.
    pub fn target_level(&self, recommendation: &QualityRecommendation) -> usize {
        match recommendation {
            QualityRecommendation::Optimal => 0,
            QualityRecommendation::ReduceBitrate { target_bps } => {
                let target_mbps = target_bps / 1_000_000;
                self.levels.iter().position(|l| l.max_bitrate_mbps <= target_mbps)
                    .unwrap_or(self.levels.len() - 1)
            }
            QualityRecommendation::ReduceResolution => {
                1.min(self.levels.len() - 1)
            }
            QualityRecommendation::ReduceFramerate => {
                self.levels.iter().position(|l| l.max_fps < self.levels[0].max_fps)
                    .unwrap_or(self.levels.len() - 1)
            }
            QualityRecommendation::EnableFec { .. } => {
                self.levels.iter().position(|l| l.fec_ratio > 0.0)
                    .unwrap_or(self.levels.len() - 1)
            }
            QualityRecommendation::SwitchToStreamOnly
            | QualityRecommendation::PauseNonEssential => {
                self.levels.len() - 1
            }
            QualityRecommendation::ConnectionUnusable => {
                self.levels.len() - 1
            }
        }
    }
}
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-display/src/degradation.rs
git commit -m "feat(display): DegradationLadder with Gaming/Coding profiles"
```

---

## Task 8: Hysteresis + UserConstraints + LevelChange

**Files:**
- Modify: `crates/prism-display/src/hysteresis.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hysteresis_allows_immediate_first_change() {
        let mut h = Hysteresis::new(Duration::from_secs(2), Duration::from_secs(10));
        assert!(h.should_change(0, 2)); // downgrade allowed immediately
    }

    #[test]
    fn hysteresis_holds_upgrade() {
        let mut h = Hysteresis::new(Duration::from_millis(50), Duration::from_millis(200));
        h.should_change(2, 2); // set current level
        // Try upgrade (lower level number = better)
        assert!(!h.should_change(2, 0)); // not yet — upgrade hold
        std::thread::sleep(Duration::from_millis(220));
        assert!(h.should_change(2, 0)); // now OK
    }

    #[test]
    fn hysteresis_fast_downgrade() {
        let mut h = Hysteresis::new(Duration::from_millis(30), Duration::from_millis(200));
        h.should_change(0, 0); // set current
        std::thread::sleep(Duration::from_millis(35));
        assert!(h.should_change(0, 2)); // downgrade after 30ms hold
    }

    #[test]
    fn level_change_detects_resolution_change() {
        let change = LevelChange::compute(0, 1, (3840, 2160), (2560, 1440));
        assert!(change.resolution_changed);
        assert!(change.needs_encoder_reinit);
        assert!(change.needs_idr);
    }

    #[test]
    fn level_change_bitrate_only() {
        let change = LevelChange::compute(0, 0, (1920, 1080), (1920, 1080));
        assert!(!change.resolution_changed);
        assert!(!change.needs_encoder_reinit);
    }

    #[test]
    fn user_constraints_clamp_level() {
        let constraints = UserConstraints {
            min_resolution: Some((1920, 1080)),
            pin_resolution: None,
            pin_fps: None,
            min_fps: None,
        };
        // Level with 720p should be rejected
        let level = crate::degradation::DegradationLevel {
            name: "min".into(), max_bitrate_mbps: 8, resolution: (1280, 720),
            max_fps: 30, codec: crate::types::CodecId::H264, region_detection: false, fec_ratio: 0.0,
        };
        assert!(!constraints.allows(&level));
        // Level with 1080p should be allowed
        let level2 = crate::degradation::DegradationLevel {
            name: "ok".into(), max_bitrate_mbps: 20, resolution: (1920, 1080),
            max_fps: 60, codec: crate::types::CodecId::H264, region_detection: false, fec_ratio: 0.0,
        };
        assert!(constraints.allows(&level2));
    }
}
```

- [ ] **Step 2: Implement Hysteresis + UserConstraints + LevelChange**

```rust
/// Prevents flapping between degradation levels.
pub struct Hysteresis {
    downgrade_hold: Duration,
    upgrade_hold: Duration,
    last_change: Option<Instant>,
    last_target: Option<usize>,
}

impl Hysteresis {
    pub fn new(downgrade_hold: Duration, upgrade_hold: Duration) -> Self {
        Self { downgrade_hold, upgrade_hold, last_change: None, last_target: None }
    }

    /// Returns true if the level change should be applied.
    pub fn should_change(&mut self, current_level: usize, target_level: usize) -> bool {
        if current_level == target_level {
            self.last_target = Some(target_level);
            return false;
        }

        let hold = if target_level > current_level {
            self.downgrade_hold // downgrading (higher level = worse)
        } else {
            self.upgrade_hold // upgrading (lower level = better)
        };

        // Check if we've been targeting this level long enough
        match (self.last_target, self.last_change) {
            (Some(prev_target), Some(since)) if prev_target == target_level => {
                if since.elapsed() >= hold {
                    self.last_change = Some(Instant::now());
                    self.last_target = Some(target_level);
                    true
                } else {
                    false
                }
            }
            _ => {
                // New target — start the hold timer
                self.last_change = Some(Instant::now());
                self.last_target = Some(target_level);
                // Allow first change immediately for downgrades (protect UX)
                if target_level > current_level && self.downgrade_hold.is_zero() {
                    true
                } else {
                    false
                }
            }
        }
    }
}

/// What changed between degradation levels.
#[derive(Debug, Clone)]
pub struct LevelChange {
    pub old_level: u8,
    pub new_level: u8,
    pub resolution_changed: bool,
    pub needs_encoder_reinit: bool,
    pub needs_idr: bool,
}

impl LevelChange {
    pub fn compute(old_level: u8, new_level: u8, old_res: (u32, u32), new_res: (u32, u32)) -> Self {
        let resolution_changed = old_res != new_res;
        Self {
            old_level, new_level,
            resolution_changed,
            needs_encoder_reinit: resolution_changed,
            needs_idr: old_level != new_level,
        }
    }
}

/// User constraints that the degradation ladder must respect.
#[derive(Debug, Clone, Default)]
pub struct UserConstraints {
    pub min_resolution: Option<(u32, u32)>,
    pub pin_resolution: Option<(u32, u32)>,
    pub pin_fps: Option<u8>,
    pub min_fps: Option<u8>,
}

impl UserConstraints {
    pub fn allows(&self, level: &crate::degradation::DegradationLevel) -> bool {
        if let Some((min_w, min_h)) = self.min_resolution {
            if level.resolution.0 < min_w || level.resolution.1 < min_h {
                return false;
            }
        }
        if let Some((pin_w, pin_h)) = self.pin_resolution {
            if level.resolution != (pin_w, pin_h) {
                return false;
            }
        }
        if let Some(min_fps) = self.min_fps {
            if level.max_fps < min_fps { return false; }
        }
        if let Some(pin_fps) = self.pin_fps {
            if level.max_fps != pin_fps { return false; }
        }
        true
    }
}
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-display/src/hysteresis.rs
git commit -m "feat(display): Hysteresis + UserConstraints + LevelChange"
```

---

## Task 9: Display Protocol + FrameGapDetector

**Files:**
- Modify: `crates/prism-display/src/protocol.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_display_msg_types_distinct() {
        let types = [REGION_MAP, SLICE, CURSOR_SHAPE, CURSOR_POSITION, IDR_REQUEST, QUALITY_HINT];
        let set: std::collections::HashSet<_> = types.iter().collect();
        assert_eq!(set.len(), types.len());
    }

    #[test]
    fn gap_detector_no_gap() {
        let mut det = FrameGapDetector::new();
        det.receive_seq(1);
        det.receive_seq(2);
        det.receive_seq(3);
        assert!(!det.should_request_idr());
    }

    #[test]
    fn gap_detector_detects_gap() {
        let mut det = FrameGapDetector::new();
        det.receive_seq(1);
        det.receive_seq(2);
        det.receive_seq(5); // gap: 3,4 missing
        assert!(det.should_request_idr());
    }

    #[test]
    fn gap_detector_cooldown() {
        let mut det = FrameGapDetector::with_cooldown(Duration::from_millis(100));
        det.receive_seq(1);
        det.receive_seq(5); // gap
        assert!(det.should_request_idr());
        det.receive_seq(10); // another gap
        assert!(!det.should_request_idr()); // still in cooldown
        std::thread::sleep(Duration::from_millis(110));
        det.receive_seq(20); // gap after cooldown
        assert!(det.should_request_idr());
    }

    #[test]
    fn gap_detector_first_frame_no_gap() {
        let mut det = FrameGapDetector::new();
        det.receive_seq(42); // first frame at seq 42 — no gap
        assert!(!det.should_request_idr());
    }
}
```

- [ ] **Step 2: Implement protocol module**

```rust
// Display channel message types
pub const REGION_MAP: u8 = 0x01;
pub const SLICE: u8 = 0x02;
pub const CURSOR_SHAPE: u8 = 0x03;
pub const CURSOR_POSITION: u8 = 0x04;
pub const IDR_REQUEST: u8 = 0x05;
pub const QUALITY_HINT: u8 = 0x06;

/// Detects sequence gaps in received frames and triggers IDR requests.
pub struct FrameGapDetector {
    last_received_seq: Option<u32>,
    has_gap: bool,
    cooldown: Duration,
    last_idr_request: Option<Instant>,
}

impl FrameGapDetector {
    pub fn new() -> Self {
        Self::with_cooldown(Duration::from_secs(1))
    }

    pub fn with_cooldown(cooldown: Duration) -> Self {
        Self {
            last_received_seq: None,
            has_gap: false,
            cooldown,
            last_idr_request: None,
        }
    }

    pub fn receive_seq(&mut self, seq: u32) {
        if let Some(last) = self.last_received_seq {
            if seq > last + 1 {
                self.has_gap = true;
            }
        }
        self.last_received_seq = Some(seq);
    }

    pub fn should_request_idr(&mut self) -> bool {
        if !self.has_gap { return false; }
        if let Some(last_req) = self.last_idr_request {
            if last_req.elapsed() < self.cooldown {
                return false;
            }
        }
        self.has_gap = false;
        self.last_idr_request = Some(Instant::now());
        true
    }
}

impl Default for FrameGapDetector {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-display/src/protocol.rs
git commit -m "feat(display): display channel protocol message types + FrameGapDetector"
```

---

## Task 10: CursorManager + InputTriggerCoalescer + FramePacer

**Files:**
- Modify: `crates/prism-display/src/cursor.rs`
- Modify: `crates/prism-display/src/pacing.rs`

- [ ] **Step 1: Write tests + implement cursor module**

`cursor.rs`:
```rust
use bytes::Bytes;

/// Cursor shape data (sent on shape change via reliable stream).
#[derive(Debug, Clone)]
pub struct CursorShape {
    pub width: u32,
    pub height: u32,
    pub hotspot_x: u32,
    pub hotspot_y: u32,
    pub data: Bytes,
    pub hash: u64,
}

/// Cursor position (piggybacked on display frame slices).
#[derive(Debug, Clone, Copy)]
pub struct CursorPosition {
    pub x: f32,
    pub y: f32,
    pub visible: bool,
    pub timestamp_us: u64,
}

/// Tracks cursor state and detects shape changes.
pub struct CursorManager {
    current_shape: Option<CursorShape>,
    last_sent_hash: u64,
}

impl CursorManager {
    pub fn new() -> Self {
        Self { current_shape: None, last_sent_hash: 0 }
    }

    pub fn update_shape(&mut self, shape: CursorShape) -> bool {
        let changed = shape.hash != self.last_sent_hash;
        self.last_sent_hash = shape.hash;
        self.current_shape = Some(shape);
        changed
    }

    pub fn current_shape(&self) -> Option<&CursorShape> {
        self.current_shape.as_ref()
    }
}

impl Default for CursorManager {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_change_detected() {
        let mut mgr = CursorManager::new();
        let shape1 = CursorShape { width: 32, height: 32, hotspot_x: 0, hotspot_y: 0, data: Bytes::new(), hash: 1 };
        assert!(mgr.update_shape(shape1));
        let shape2 = CursorShape { width: 32, height: 32, hotspot_x: 0, hotspot_y: 0, data: Bytes::new(), hash: 1 };
        assert!(!mgr.update_shape(shape2)); // same hash
        let shape3 = CursorShape { width: 16, height: 16, hotspot_x: 0, hotspot_y: 0, data: Bytes::new(), hash: 2 };
        assert!(mgr.update_shape(shape3)); // different hash
    }

    #[test]
    fn cursor_position_normalized() {
        let pos = CursorPosition { x: 0.5, y: 0.5, visible: true, timestamp_us: 1000 };
        assert!(pos.x >= 0.0 && pos.x <= 1.0);
    }
}
```

- [ ] **Step 2: Write tests + implement pacing module**

`pacing.rs`:
```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Coalesces rapid input events to prevent excessive capture triggers.
/// Debounces to min_interval (8ms = 125Hz max).
pub struct InputTriggerCoalescer {
    min_interval: Duration,
    last_trigger: AtomicU64,
    pending: AtomicBool,
}

impl InputTriggerCoalescer {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last_trigger: AtomicU64::new(0),
            pending: AtomicBool::new(false),
        }
    }

    /// Signal an input event. Returns true if a capture should be triggered.
    pub fn trigger(&self, now_us: u64) -> bool {
        let last = self.last_trigger.load(Ordering::Relaxed);
        let elapsed_us = now_us.saturating_sub(last);
        if elapsed_us >= self.min_interval.as_micros() as u64 {
            self.last_trigger.store(now_us, Ordering::Relaxed);
            self.pending.store(false, Ordering::Relaxed);
            true
        } else {
            self.pending.store(true, Ordering::Relaxed);
            false
        }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed)
    }
}

/// Adaptive frame pacer that matches capture rate to content update rate.
pub struct FramePacer {
    target_fps: u8,
    content_fps: f32,
    min_interval: Duration,
    last_capture: Instant,
}

impl FramePacer {
    pub fn new(target_fps: u8) -> Self {
        let interval = Duration::from_micros(1_000_000 / target_fps.max(1) as u64);
        Self {
            target_fps,
            content_fps: target_fps as f32,
            min_interval: interval,
            last_capture: Instant::now(),
        }
    }

    /// Update observed content update rate.
    pub fn set_content_fps(&mut self, fps: f32) {
        self.content_fps = fps;
        // Capture at 1.2x content rate, capped at target
        let adaptive_fps = (fps * 1.2).min(self.target_fps as f32).max(1.0);
        self.min_interval = Duration::from_micros((1_000_000.0 / adaptive_fps) as u64);
    }

    /// Returns true if enough time has passed for the next capture.
    pub fn should_capture(&mut self) -> bool {
        if self.last_capture.elapsed() >= self.min_interval {
            self.last_capture = Instant::now();
            true
        } else {
            false
        }
    }

    pub fn current_interval(&self) -> Duration {
        self.min_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_trigger_debounce() {
        let coalescer = InputTriggerCoalescer::new(Duration::from_millis(8));
        assert!(coalescer.trigger(0)); // first trigger always fires
        assert!(!coalescer.trigger(1_000)); // 1ms later — too soon
        assert!(coalescer.has_pending()); // pending flag set
        assert!(coalescer.trigger(10_000)); // 10ms later — OK
    }

    #[test]
    fn frame_pacer_respects_interval() {
        let mut pacer = FramePacer::new(60);
        assert!(pacer.should_capture()); // first capture always
        assert!(!pacer.should_capture()); // too soon
    }

    #[test]
    fn frame_pacer_adapts_to_content() {
        let mut pacer = FramePacer::new(60);
        pacer.set_content_fps(10.0); // content at 10fps
        // Adaptive: 10 * 1.2 = 12fps → ~83ms interval
        let interval = pacer.current_interval();
        assert!(interval.as_millis() > 70 && interval.as_millis() < 100,
            "expected ~83ms, got {}ms", interval.as_millis());
    }

    #[test]
    fn frame_pacer_caps_at_target() {
        let mut pacer = FramePacer::new(60);
        pacer.set_content_fps(100.0); // content faster than target
        // Should cap at 60fps → ~16ms
        let interval = pacer.current_interval();
        assert!(interval.as_millis() <= 17, "should cap at 60fps, got {}ms", interval.as_millis());
    }
}
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-display/src/cursor.rs crates/prism-display/src/pacing.rs
git commit -m "feat(display): CursorManager, InputTriggerCoalescer, FramePacer"
```

---

## Task 11: EncoderConfig + RateControlHinter + SlicePayloadHeader

**Files:**
- Modify: `crates/prism-display/src/encode_config.rs`
- Modify: `crates/prism-display/src/packet.rs`

- [ ] **Step 1: Write tests + implement encode_config**

`encode_config.rs`:
```rust
use std::collections::HashMap;
use crate::types::CodecId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderPreset {
    UltraLowLatency,
    Quality,
    Balanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyframeInterval {
    Fixed(u32),
    Adaptive { min_frames: u32, max_frames: u32 },
    OnDemand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SliceMode {
    Single,
    Fixed(u8),
    Adaptive { min: u8, max: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub codec: CodecId,
    pub preset: EncoderPreset,
    pub bitrate_bps: u64,
    pub max_fps: u8,
    pub resolution: (u32, u32),
    pub keyframe_interval: KeyframeInterval,
    pub slice_mode: SliceMode,
}

/// Per-window content complexity for rate control warm-start.
#[derive(Debug, Clone)]
pub struct ComplexityEstimate {
    pub avg_qp: f32,
    pub avg_bitrate_bps: u64,
    pub frame_count: u32,
}

/// Rate control warm-start hints based on window content complexity.
pub struct RateControlHinter {
    window_complexity: HashMap<u64, ComplexityEstimate>,
}

impl RateControlHinter {
    pub fn new() -> Self { Self { window_complexity: HashMap::new() } }

    pub fn record(&mut self, hwnd: u64, qp: f32, bitrate_bps: u64) {
        let entry = self.window_complexity.entry(hwnd).or_insert(ComplexityEstimate {
            avg_qp: qp, avg_bitrate_bps: bitrate_bps, frame_count: 0,
        });
        let n = entry.frame_count as f32;
        entry.avg_qp = (entry.avg_qp * n + qp) / (n + 1.0);
        entry.avg_bitrate_bps = ((entry.avg_bitrate_bps as f64 * n as f64 + bitrate_bps as f64) / (n as f64 + 1.0)) as u64;
        entry.frame_count += 1;
    }

    pub fn hint(&self, hwnd: u64) -> Option<&ComplexityEstimate> {
        self.window_complexity.get(&hwnd)
    }
}

impl Default for RateControlHinter {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_hinter_records_and_hints() {
        let mut hinter = RateControlHinter::new();
        hinter.record(1, 20.0, 5_000_000);
        hinter.record(1, 22.0, 6_000_000);
        let hint = hinter.hint(1).unwrap();
        assert_eq!(hint.frame_count, 2);
        assert!((hint.avg_qp - 21.0).abs() < 0.1);
    }

    #[test]
    fn rate_hinter_unknown_window() {
        let hinter = RateControlHinter::new();
        assert!(hinter.hint(999).is_none());
    }

    #[test]
    fn encoder_config_defaults() {
        let config = EncoderConfig {
            codec: CodecId::H264,
            preset: EncoderPreset::UltraLowLatency,
            bitrate_bps: 20_000_000,
            max_fps: 60,
            resolution: (1920, 1080),
            keyframe_interval: KeyframeInterval::Adaptive { min_frames: 120, max_frames: 1800 },
            slice_mode: SliceMode::Single,
        };
        assert_eq!(config.max_fps, 60);
    }
}
```

- [ ] **Step 2: Write tests + implement packet module**

`packet.rs`:
```rust
/// Self-describing slice payload header (24 bytes per slice).
/// Includes cursor piggybacking fields on first slice of each frame.
/// Overhead: 24 bytes × 4 slices × 60fps = 5.76 KB/sec. Negligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlicePayloadHeader {
    pub decoder_slot: u8,
    pub slice_index: u8,
    pub total_slices: u8,
    pub encoding_type: u8,
    pub rect_x: i16,
    pub rect_y: i16,
    pub rect_w: u16,
    pub rect_h: u16,
    pub region_count: u8,
    pub is_preview: u8,
    pub replaces_seq: u32,          // full 4 bytes — no truncation
    // Cursor piggybacking (first slice only, zeroed on subsequent slices)
    pub cursor_x: u16,              // normalized 0-65535
    pub cursor_y: u16,              // normalized 0-65535
    pub cursor_flags: u8,           // bit 0: visible, bit 1: shape_changed
    pub _reserved: u8,              // pad to 24 bytes
}

pub const SLICE_HEADER_SIZE: usize = 24;

impl SlicePayloadHeader {
    pub fn to_bytes(&self) -> [u8; SLICE_HEADER_SIZE] {
        let mut buf = [0u8; SLICE_HEADER_SIZE];
        buf[0] = self.decoder_slot;
        buf[1] = self.slice_index;
        buf[2] = self.total_slices;
        buf[3] = self.encoding_type;
        buf[4..6].copy_from_slice(&self.rect_x.to_le_bytes());
        buf[6..8].copy_from_slice(&self.rect_y.to_le_bytes());
        buf[8..10].copy_from_slice(&self.rect_w.to_le_bytes());
        buf[10..12].copy_from_slice(&self.rect_h.to_le_bytes());
        buf[12] = self.region_count;
        buf[13] = self.is_preview;
        buf[14..18].copy_from_slice(&self.replaces_seq.to_le_bytes());
        buf[18..20].copy_from_slice(&self.cursor_x.to_le_bytes());
        buf[20..22].copy_from_slice(&self.cursor_y.to_le_bytes());
        buf[22] = self.cursor_flags;
        buf[23] = 0; // reserved
        buf
    }

    /// Zero-copy: write directly into a pre-existing buffer.
    /// Same pattern as PrismHeader::encode_to_slice.
    #[inline(always)]
    pub fn encode_to_slice(&self, buf: &mut [u8]) -> usize {
        buf[0] = self.decoder_slot;
        buf[1] = self.slice_index;
        buf[2] = self.total_slices;
        buf[3] = self.encoding_type;
        buf[4..6].copy_from_slice(&self.rect_x.to_le_bytes());
        buf[6..8].copy_from_slice(&self.rect_y.to_le_bytes());
        buf[8..10].copy_from_slice(&self.rect_w.to_le_bytes());
        buf[10..12].copy_from_slice(&self.rect_h.to_le_bytes());
        buf[12] = self.region_count;
        buf[13] = self.is_preview;
        buf[14..18].copy_from_slice(&self.replaces_seq.to_le_bytes());
        buf[18..20].copy_from_slice(&self.cursor_x.to_le_bytes());
        buf[20..22].copy_from_slice(&self.cursor_y.to_le_bytes());
        buf[22] = self.cursor_flags;
        buf[23] = 0;
        SLICE_HEADER_SIZE
    }

    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < SLICE_HEADER_SIZE { return None; }
        Some(Self {
            decoder_slot: buf[0],
            slice_index: buf[1],
            total_slices: buf[2],
            encoding_type: buf[3],
            rect_x: i16::from_le_bytes([buf[4], buf[5]]),
            rect_y: i16::from_le_bytes([buf[6], buf[7]]),
            rect_w: u16::from_le_bytes([buf[8], buf[9]]),
            rect_h: u16::from_le_bytes([buf[10], buf[11]]),
            region_count: buf[12],
            is_preview: buf[13],
            replaces_seq: u32::from_le_bytes([buf[14], buf[15], buf[16], buf[17]]),
            cursor_x: u16::from_le_bytes([buf[18], buf[19]]),
            cursor_y: u16::from_le_bytes([buf[20], buf[21]]),
            cursor_flags: buf[22],
            _reserved: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_header_roundtrip() {
        let header = SlicePayloadHeader {
            decoder_slot: 0, slice_index: 1, total_slices: 4, encoding_type: 0x01,
            rect_x: 100, rect_y: 200, rect_w: 1920, rect_h: 1080,
            region_count: 3, is_preview: 0, replaces_seq: 70000,
            cursor_x: 32768, cursor_y: 16384, cursor_flags: 0x03, _reserved: 0,
        };
        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), SLICE_HEADER_SIZE);
        let decoded = SlicePayloadHeader::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.replaces_seq, 70000); // full u32, no truncation
        assert_eq!(decoded.cursor_x, 32768);
        assert_eq!(decoded.cursor_flags, 0x03);
    }

    #[test]
    fn slice_header_size_is_24() {
        assert_eq!(SLICE_HEADER_SIZE, 24);
    }

    #[test]
    fn encode_to_slice_matches_to_bytes() {
        let header = SlicePayloadHeader {
            decoder_slot: 2, slice_index: 0, total_slices: 1, encoding_type: 0x02,
            rect_x: 0, rect_y: 0, rect_w: 3840, rect_h: 2160,
            region_count: 1, is_preview: 1, replaces_seq: 42,
            cursor_x: 0, cursor_y: 0, cursor_flags: 0, _reserved: 0,
        };
        let a = header.to_bytes();
        let mut b = [0u8; SLICE_HEADER_SIZE];
        header.encode_to_slice(&mut b);
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-display/src/encode_config.rs crates/prism-display/src/packet.rs
git commit -m "feat(display): EncoderConfig, RateControlHinter, SlicePayloadHeader"
```

---

## Task 12: StaticAtlasTracker + lib.rs Re-exports

**Files:**
- Modify: `crates/prism-display/src/atlas.rs`
- Modify: `crates/prism-display/src/lib.rs`

- [ ] **Step 1: Write failing tests**

`atlas.rs`:
```rust
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_region_is_not_cached() {
        let mut tracker = StaticAtlasTracker::new(30);
        let decision = tracker.check(RegionKey(0x001, 0), 12345);
        assert_eq!(decision, StaticDecision::EncodeNormally);
    }

    #[test]
    fn region_becomes_cached_after_threshold() {
        let mut tracker = StaticAtlasTracker::new(3);
        let key = RegionKey(0x001, 0);
        for _ in 0..3 {
            tracker.check(key, 100);
        }
        let decision = tracker.check(key, 100);
        assert_eq!(decision, StaticDecision::SendAndCache);
        // After caching, should be Unchanged
        let decision = tracker.check(key, 100);
        assert_eq!(decision, StaticDecision::Unchanged);
    }

    #[test]
    fn hash_change_invalidates_cache() {
        let mut tracker = StaticAtlasTracker::new(2);
        let key = RegionKey(0x001, 0);
        for _ in 0..3 {
            tracker.check(key, 100);
        }
        // Now cached. Change hash → invalidate
        let decision = tracker.check(key, 200);
        assert_eq!(decision, StaticDecision::EncodeNormally);
    }

    #[test]
    fn different_regions_tracked_independently() {
        let mut tracker = StaticAtlasTracker::new(2);
        let key1 = RegionKey(0x001, 0);
        let key2 = RegionKey(0x001, 1);
        for _ in 0..3 { tracker.check(key1, 100); }
        for _ in 0..3 { tracker.check(key2, 200); }
        assert_eq!(tracker.check(key1, 100), StaticDecision::Unchanged);
        assert_eq!(tracker.check(key2, 200), StaticDecision::Unchanged);
    }
}
```

- [ ] **Step 2: Implement StaticAtlasTracker**

```rust
/// Key identifying a region: (display_id_channel, region_index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionKey(pub u16, pub u8);

/// Decision for a static region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticDecision {
    Unchanged,
    SendAndCache,
    EncodeNormally,
}

/// Tracks static regions for client-side atlas caching.
pub struct StaticAtlasTracker {
    region_hashes: HashMap<RegionKey, u64>,
    static_frame_count: HashMap<RegionKey, u32>,
    cached_regions: HashMap<RegionKey, u64>,
    cache_threshold: u32,
}

impl StaticAtlasTracker {
    pub fn new(cache_threshold: u32) -> Self {
        Self {
            region_hashes: HashMap::new(),
            static_frame_count: HashMap::new(),
            cached_regions: HashMap::new(),
            cache_threshold,
        }
    }

    /// Check a region. Returns what to do with it.
    pub fn check(&mut self, key: RegionKey, content_hash: u64) -> StaticDecision {
        // Check if hash changed → invalidate
        if let Some(&prev_hash) = self.region_hashes.get(&key) {
            if prev_hash != content_hash {
                self.static_frame_count.remove(&key);
                self.cached_regions.remove(&key);
                self.region_hashes.insert(key, content_hash);
                return StaticDecision::EncodeNormally;
            }
        } else {
            self.region_hashes.insert(key, content_hash);
            return StaticDecision::EncodeNormally;
        }

        // Already cached?
        if self.cached_regions.contains_key(&key) {
            return StaticDecision::Unchanged;
        }

        // Increment static counter
        let count = self.static_frame_count.entry(key).or_insert(0);
        *count += 1;

        if *count >= self.cache_threshold {
            self.cached_regions.insert(key, content_hash);
            StaticDecision::SendAndCache
        } else {
            StaticDecision::EncodeNormally
        }
    }
}

impl Default for StaticAtlasTracker {
    fn default() -> Self { Self::new(30) }
}
```

- [ ] **Step 3: Update lib.rs with full re-exports**

```rust
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

pub use types::*;
pub use frame::*;
pub use ring::FrameRing;
pub use capture::{PlatformCapture, CaptureConfig, CaptureMode, CursorCapture, MonitorInfo, DisplayConfig, CaptureError};
pub use classify::{RegionType, UpdateFrequency, ClassifiedRegion, WindowActivity, Tier1Classifier};
pub use damage::{merge_damage_rects, macroblock_snap};
pub use degradation::{DegradationLevel, DegradationLadder};
pub use hysteresis::{Hysteresis, UserConstraints, LevelChange};
pub use protocol::{FrameGapDetector, REGION_MAP, SLICE, CURSOR_SHAPE, CURSOR_POSITION, IDR_REQUEST, QUALITY_HINT};
pub use cursor::{CursorManager, CursorShape, CursorPosition};
pub use pacing::{InputTriggerCoalescer, FramePacer};
pub use encode_config::{EncoderConfig, EncoderPreset, KeyframeInterval, SliceMode, RateControlHinter};
pub use packet::{SlicePayloadHeader, SLICE_HEADER_SIZE};
pub use atlas::{StaticAtlasTracker, StaticDecision, RegionKey};
```

- [ ] **Step 4: Run all tests**

Run: `cargo test -p prism-display`
Expected: all tests pass

Run: `cargo test --workspace`
Expected: all crates pass

- [ ] **Step 5: Commit**

```bash
git add crates/prism-display/src/atlas.rs crates/prism-display/src/lib.rs
git commit -m "feat(display): StaticAtlasTracker + full crate re-exports"
```

---

## Task 13: EncodeQueue + WindowEvent + EncoderBackend

**Files:**
- Create: `crates/prism-display/src/encode_queue.rs`
- Create: `crates/prism-display/src/window_event.rs`
- Create: `crates/prism-display/src/encoder_backend.rs`

- [ ] **Step 1: Write tests + implement EncodeQueue**

`encode_queue.rs`:
```rust
use std::collections::VecDeque;
use crate::frame::EncodeJob;

/// Priority-aware encode queue. High-priority jobs (video, keyframes) drain before normal (text).
pub struct EncodeQueue {
    high: VecDeque<EncodeJob>,
    normal: VecDeque<EncodeJob>,
}

impl EncodeQueue {
    pub fn new() -> Self { Self { high: VecDeque::new(), normal: VecDeque::new() } }

    pub fn push_high(&mut self, job: EncodeJob) { self.high.push_back(job); }
    pub fn push_normal(&mut self, job: EncodeJob) { self.normal.push_back(job); }

    /// Steal next job. Drains high queue first, then normal.
    pub fn steal(&mut self) -> Option<EncodeJob> {
        self.high.pop_front().or_else(|| self.normal.pop_front())
    }

    pub fn len(&self) -> usize { self.high.len() + self.normal.len() }
    pub fn is_empty(&self) -> bool { self.high.is_empty() && self.normal.is_empty() }
}

impl Default for EncodeQueue { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use crate::classify::RegionType;
    use bytes::Bytes;

    fn dummy_job(seq: u32, is_high: bool) -> EncodeJob {
        EncodeJob {
            frame_seq: seq, display_id: DisplayId(0),
            region_rect: Rect { x: 0, y: 0, w: 100, h: 100 },
            region_type: if is_high { RegionType::Video } else { RegionType::Text },
            texture: SharedTexture { handle: 0, width: 100, height: 100, format: TextureFormat::Bgra8 },
            target_bitrate: 5_000_000, force_keyframe: false,
            quality_tier: QualityTier::Normal, expected_regions: 1,
            frame_meta: crate::frame::FrameMetadata {
                display_id: DisplayId(0), capture_time_us: 0,
                is_preview: false, replaces_seq: None, total_regions: 1,
            },
        }
    }

    #[test]
    fn high_priority_drains_first() {
        let mut q = EncodeQueue::new();
        q.push_normal(dummy_job(1, false));
        q.push_high(dummy_job(2, true));
        q.push_normal(dummy_job(3, false));
        assert_eq!(q.steal().unwrap().frame_seq, 2); // high first
        assert_eq!(q.steal().unwrap().frame_seq, 1); // then normal
        assert_eq!(q.steal().unwrap().frame_seq, 3);
    }

    #[test]
    fn empty_queue_returns_none() {
        let mut q = EncodeQueue::new();
        assert!(q.steal().is_none());
    }

    #[test]
    fn len_tracks_both_queues() {
        let mut q = EncodeQueue::new();
        q.push_high(dummy_job(1, true));
        q.push_normal(dummy_job(2, false));
        assert_eq!(q.len(), 2);
    }
}
```

- [ ] **Step 2: Write tests + implement WindowEvent**

`window_event.rs`:
```rust
/// Window events for speculative IDR (Win32 hooks on Windows, equivalents on other platforms).
#[derive(Debug, Clone)]
pub enum WindowEvent {
    ForegroundChanged { hwnd: u64 },
    MinimizeStart { hwnd: u64 },
    MinimizeEnd { hwnd: u64 },
    MoveSizeEnd { hwnd: u64 },
    WindowDestroyed { hwnd: u64 },
}

impl WindowEvent {
    /// Whether this event should trigger a speculative capture + IDR.
    pub fn triggers_speculative_idr(&self) -> bool {
        matches!(self, WindowEvent::ForegroundChanged { .. } | WindowEvent::MinimizeEnd { .. })
    }

    pub fn hwnd(&self) -> u64 {
        match self {
            WindowEvent::ForegroundChanged { hwnd }
            | WindowEvent::MinimizeStart { hwnd }
            | WindowEvent::MinimizeEnd { hwnd }
            | WindowEvent::MoveSizeEnd { hwnd }
            | WindowEvent::WindowDestroyed { hwnd } => *hwnd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_change_triggers_speculative() {
        let event = WindowEvent::ForegroundChanged { hwnd: 42 };
        assert!(event.triggers_speculative_idr());
    }

    #[test]
    fn move_size_does_not_trigger_speculative() {
        let event = WindowEvent::MoveSizeEnd { hwnd: 42 };
        assert!(!event.triggers_speculative_idr());
    }

    #[test]
    fn hwnd_extraction() {
        let event = WindowEvent::WindowDestroyed { hwnd: 999 };
        assert_eq!(event.hwnd(), 999);
    }
}
```

- [ ] **Step 3: Write tests + implement EncoderBackend**

`encoder_backend.rs`:
```rust
use crate::types::CodecId;

/// GPU encoder backend. Platform-specific implementations behind this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderBackend {
    Nvenc,
    Amf,
    Qsv,
    VideoToolbox,
    Vaapi,
    Software,
}

impl EncoderBackend {
    /// Priority order for encoder selection.
    pub fn priority(&self) -> u8 {
        match self {
            EncoderBackend::Nvenc => 0,       // highest
            EncoderBackend::Amf => 1,
            EncoderBackend::Qsv => 2,
            EncoderBackend::VideoToolbox => 3,
            EncoderBackend::Vaapi => 4,
            EncoderBackend::Software => 255,  // lowest
        }
    }

    /// Whether this backend supports hardware lossless encoding.
    pub fn supports_hw_lossless(&self) -> bool {
        matches!(self, EncoderBackend::Nvenc | EncoderBackend::Qsv)
    }

    /// Supported codecs for this backend.
    pub fn supported_codecs(&self) -> Vec<CodecId> {
        match self {
            EncoderBackend::Nvenc => vec![CodecId::H264, CodecId::H265, CodecId::Av1],
            EncoderBackend::Amf => vec![CodecId::H264, CodecId::H265],
            EncoderBackend::Qsv => vec![CodecId::H264, CodecId::H265, CodecId::Av1],
            EncoderBackend::VideoToolbox => vec![CodecId::H264, CodecId::H265],
            EncoderBackend::Vaapi => vec![CodecId::H264, CodecId::H265],
            EncoderBackend::Software => vec![CodecId::H264],
        }
    }
}

/// Select the best available encoder from a list of detected backends.
pub fn select_best_encoder(available: &[EncoderBackend], required_codec: CodecId) -> Option<EncoderBackend> {
    let mut candidates: Vec<_> = available.iter()
        .filter(|b| b.supported_codecs().contains(&required_codec))
        .collect();
    candidates.sort_by_key(|b| b.priority());
    candidates.first().copied().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvenc_highest_priority() {
        let backends = vec![EncoderBackend::Software, EncoderBackend::Nvenc, EncoderBackend::Amf];
        let best = select_best_encoder(&backends, CodecId::H264).unwrap();
        assert_eq!(best, EncoderBackend::Nvenc);
    }

    #[test]
    fn falls_back_to_software() {
        let backends = vec![EncoderBackend::Software];
        let best = select_best_encoder(&backends, CodecId::H264).unwrap();
        assert_eq!(best, EncoderBackend::Software);
    }

    #[test]
    fn no_av1_software() {
        let backends = vec![EncoderBackend::Software];
        assert!(select_best_encoder(&backends, CodecId::Av1).is_none());
    }

    #[test]
    fn nvenc_supports_hw_lossless() {
        assert!(EncoderBackend::Nvenc.supports_hw_lossless());
        assert!(!EncoderBackend::Amf.supports_hw_lossless());
        assert!(!EncoderBackend::Software.supports_hw_lossless());
    }
}
```

- [ ] **Step 4: Verify, commit**

```bash
git add crates/prism-display/src/encode_queue.rs crates/prism-display/src/window_event.rs crates/prism-display/src/encoder_backend.rs
git commit -m "feat(display): EncodeQueue, WindowEvent, EncoderBackend with selection"
```

---

## Task 14: KeyframeDecider + StaticAtlasTracker LRU + Classification Caching

**Files:**
- Create: `crates/prism-display/src/keyframe.rs`
- Modify: `crates/prism-display/src/atlas.rs` (add LRU eviction)
- Modify: `crates/prism-display/src/classify.rs` (add classification caching)
- Modify: `crates/prism-display/src/lib.rs` (update re-exports for all new modules)

- [ ] **Step 1: Write tests + implement KeyframeDecider**

`keyframe.rs`:
```rust
use crate::encode_config::KeyframeInterval;

/// Content-adaptive keyframe decision logic.
/// Takes loss rate, scene change, elapsed frames → returns whether to force IDR.
pub struct KeyframeDecider {
    interval: KeyframeInterval,
    frames_since_idr: u32,
}

impl KeyframeDecider {
    pub fn new(interval: KeyframeInterval) -> Self {
        Self { interval, frames_since_idr: 0 }
    }

    /// Call after each encoded frame. Returns true if next frame should be IDR.
    pub fn should_force_idr(&mut self, loss_rate: f32, is_scene_change: bool) -> bool {
        self.frames_since_idr += 1;

        // Scene change always triggers IDR
        if is_scene_change {
            self.frames_since_idr = 0;
            return true;
        }

        match self.interval {
            KeyframeInterval::Fixed(n) => {
                if self.frames_since_idr >= n {
                    self.frames_since_idr = 0;
                    return true;
                }
            }
            KeyframeInterval::Adaptive { min_frames, max_frames } => {
                // High loss → more frequent keyframes
                let target = if loss_rate > 0.02 {
                    min_frames // lossy: frequent IDR
                } else if loss_rate > 0.005 {
                    (min_frames + max_frames) / 2
                } else {
                    max_frames // clean: infrequent IDR
                };
                if self.frames_since_idr >= target {
                    self.frames_since_idr = 0;
                    return true;
                }
            }
            KeyframeInterval::OnDemand => {
                // Only on explicit request or scene change (handled above)
            }
        }
        false
    }

    pub fn reset(&mut self) { self.frames_since_idr = 0; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_interval_triggers_at_count() {
        let mut kd = KeyframeDecider::new(KeyframeInterval::Fixed(30));
        for _ in 0..29 { assert!(!kd.should_force_idr(0.0, false)); }
        assert!(kd.should_force_idr(0.0, false)); // 30th frame
    }

    #[test]
    fn scene_change_always_triggers() {
        let mut kd = KeyframeDecider::new(KeyframeInterval::Fixed(1000));
        assert!(kd.should_force_idr(0.0, true));
    }

    #[test]
    fn adaptive_shortens_on_loss() {
        let mut kd = KeyframeDecider::new(KeyframeInterval::Adaptive { min_frames: 60, max_frames: 1800 });
        // High loss: should trigger at min_frames (60)
        for _ in 0..59 { kd.should_force_idr(0.05, false); }
        assert!(kd.should_force_idr(0.05, false));
    }

    #[test]
    fn adaptive_extends_on_clean() {
        let mut kd = KeyframeDecider::new(KeyframeInterval::Adaptive { min_frames: 60, max_frames: 1800 });
        // No loss: should NOT trigger at 60
        for _ in 0..60 { assert!(!kd.should_force_idr(0.0, false)); }
        // Should trigger at 1800
        for _ in 0..1739 { kd.should_force_idr(0.0, false); }
        assert!(kd.should_force_idr(0.0, false));
    }

    #[test]
    fn on_demand_never_auto_triggers() {
        let mut kd = KeyframeDecider::new(KeyframeInterval::OnDemand);
        for _ in 0..10000 { assert!(!kd.should_force_idr(0.0, false)); }
    }
}
```

- [ ] **Step 2: Add LRU eviction to StaticAtlasTracker**

Add to `atlas.rs` — modify StaticAtlasTracker to accept `max_regions` and evict LRU:

```rust
pub struct StaticAtlasTracker {
    region_hashes: HashMap<RegionKey, u64>,
    static_frame_count: HashMap<RegionKey, u32>,
    cached_regions: HashMap<RegionKey, u64>,
    last_seen: HashMap<RegionKey, u64>,   // frame counter for LRU
    cache_threshold: u32,
    max_regions: usize,
    frame_counter: u64,
}
```

Add `evict_lru()` method that removes the least-recently-seen cached region when at capacity. Call it in `check()` before inserting new cache entries.

Test: create tracker with max_regions=2, cache 3 regions, verify oldest is evicted.

- [ ] **Step 3: Add classification caching to Tier1Classifier**

Add to `classify.rs` — cache the previous classification result:

```rust
pub struct Tier1Classifier {
    history: HashMap<u64, (RegionType, Instant)>,
    cached_result: Option<(u64, Vec<ClassifiedRegion>)>, // (window_set_hash, cached regions)
}
```

Add a `window_set_hash()` method that hashes the window activity set. If the hash matches the cached result, return the cached regions immediately. This avoids recomputing classifications when the window layout hasn't changed (most frames).

Test: classify same windows twice, verify second call returns cached (can verify by checking the confidence doesn't restart).

- [ ] **Step 4: Update lib.rs with all new module declarations and re-exports**

Add to module declarations: `pub mod encode_queue; pub mod window_event; pub mod encoder_backend; pub mod keyframe;`

Add to re-exports:
```rust
pub use encode_queue::EncodeQueue;
pub use window_event::WindowEvent;
pub use encoder_backend::{EncoderBackend, select_best_encoder};
pub use keyframe::KeyframeDecider;
```

- [ ] **Step 5: Run all tests, verify workspace**

Run: `cargo test -p prism-display`
Run: `cargo test --workspace`

- [ ] **Step 6: Commit**

```bash
git add crates/prism-display/
git commit -m "feat(display): KeyframeDecider, atlas LRU eviction, classification caching, encoder backend"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Crate setup + Rect, DisplayId, CodecId, core types | 8 |
| 2 | CapturedFrame, EncodeJob, EncodedRegion, FrameMetadata | 3 |
| 3 | FrameRing lock-free SPSC (cache-line padded) | 7 |
| 4 | PlatformCapture trait, CaptureConfig, MonitorInfo | 3 |
| 5 | RegionType, Tier1Classifier (with caching), UpdateFrequency | 6 |
| 6 | Damage rect merging + configurable macroblock/superblock snapping | 8 |
| 7 | DegradationLadder + Gaming/Coding profiles | 7 |
| 8 | Hysteresis + UserConstraints + LevelChange | 6 |
| 9 | Display protocol messages + FrameGapDetector | 4 |
| 10 | CursorManager + InputTriggerCoalescer + FramePacer | 6 |
| 11 | EncoderConfig + RateControlHinter + SlicePayloadHeader (24B, cursor piggyback) | 6 |
| 12 | StaticAtlasTracker (with LRU eviction) + re-exports | 5 |
| 13 | EncodeQueue + WindowEvent + EncoderBackend + select_best_encoder | 10 |
| 14 | KeyframeDecider + atlas LRU + classification caching + full re-exports | 7 |
| **Total** | | **~86** |

**Deferred to Plan 6 (Integration):** DisplayEngine orchestrator, actual platform capture (DDA, WGC, NVENC), GPU texture sharing, encoder pool, streaming packetizer with routing table, multi-client pipelines, end-to-end pipeline tests.

**Phase 2 only:** Tier 2 GPU compute classifier, static atlas full tracking, multi-encoder pool, multi-slice streaming, HW lossless encoding, delta compression.

**Phase 3 only:** macOS/Linux capture, multi-client active pipelines.
