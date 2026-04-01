# Plan 7: Windows Platform — DDA Capture + NVENC Encoder

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-platform-windows` crate implementing `PlatformCapture` via Windows Desktop Duplication API (DDA) and an NVENC hardware encoder, wiring real GPU capture and encoding into the PRISM pipeline with zero-copy GPU texture sharing.

**Architecture:** `prism-platform-windows` is a `#[cfg(target_os = "windows")]` crate that implements the traits defined in `prism-display`. `DdaCapture` implements `PlatformCapture` using DXGI Output Duplication (DDA) for composited desktop capture, returning `CapturedFrame` with `SharedTexture` handles to GPU-resident textures. `NvencEncoder` wraps NVIDIA Video Codec SDK for hardware H.264/H.265 encoding with ultra-low-latency configuration. Both operate entirely on the GPU — no CPU pixel readback. The crate uses the `windows` crate for COM/DXGI/D3D11 bindings and `nvidia-video-codec-sdk` for NVENC. Safe wrappers around unsafe FFI are tested independently; integration tests require actual hardware.

**Tech Stack:** `windows` (COM/DXGI/D3D11), `nvidia-video-codec-sdk` (NVENC FFI), `prism-display` (PlatformCapture trait, frame types), `prism-protocol`, `bytes`

**Spec refs:**
- Display Engine: `docs/superpowers/specs/2026-03-30-display-engine-design.md` (Sections 2.1-2.6, 4.1-4.4)

**Note:** This crate requires Windows 10+ and an NVIDIA GPU to run. Tests are split into:
- **Unit tests:** Pure logic (texture pool management, config builders, format conversion) — run anywhere
- **Integration tests:** Require Windows + GPU — gated behind `#[cfg(test)]` + `#[ignore]` for CI, run manually with `cargo test -- --ignored`

---

## File Structure

```
PRISM/
  crates/
    prism-platform-windows/
      Cargo.toml
      src/
        lib.rs                      # re-exports, #[cfg(target_os = "windows")]
        error.rs                    # PlatformError (HRESULT wrapping)
        d3d.rs                      # D3D11 device/context creation, feature level check
        texture.rs                  # TexturePool, SharedTextureHandle, double-buffering
        dda.rs                      # DdaCapture (PlatformCapture impl), output duplication
        monitor.rs                  # Monitor enumeration via DXGI adapter/output
        damage.rs                   # DDA dirty rect extraction + merging
        nvenc/
          mod.rs                    # NvencEncoder, session management
          config.rs                 # Encoder config builder (ultra-low-latency presets)
          session.rs                # NvencSession (encode/flush lifecycle)
          format.rs                 # Texture format conversion (BGRA→NV12 if needed)
```

---

## Task 1: Crate Setup + PlatformError

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-platform-windows/Cargo.toml`
- Create: `crates/prism-platform-windows/src/lib.rs`
- Create: `crates/prism-platform-windows/src/error.rs`
- Create: all placeholder source files

- [ ] **Step 1: Update workspace root Cargo.toml**

Add `"crates/prism-platform-windows"` to members. Add new workspace dependencies:

```toml
[workspace.dependencies]
# ... existing deps unchanged, add:
prism-platform-windows = { path = "crates/prism-platform-windows" }
```

Do NOT add the `windows` crate to workspace deps — it's a platform-specific dep that only this crate uses.

- [ ] **Step 2: Create crates/prism-platform-windows/Cargo.toml**

```toml
[package]
name = "prism-platform-windows"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
prism-display = { workspace = true }
prism-protocol = { workspace = true }
bytes = { workspace = true }
thiserror = { workspace = true }

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Graphics_Dxgi",
    "Win32_Graphics_Dxgi_Common",
    "Win32_Graphics_Direct3D",
    "Win32_Graphics_Direct3D11",
    "Win32_System_Com",
    "Win32_Security",
    "Foundation",
] }
```

Note: We start WITHOUT nvenc — that's Task 8+. DDA capture is Tasks 2-7.

- [ ] **Step 3: Create lib.rs and all placeholders**

`lib.rs`:
```rust
pub mod error;
pub mod d3d;
pub mod texture;
pub mod dda;
pub mod monitor;
pub mod damage;
pub mod nvenc;
```

Create all placeholder source files including `nvenc/mod.rs`, `nvenc/config.rs`, `nvenc/session.rs`, `nvenc/format.rs`.

- [ ] **Step 4: Write tests + implement PlatformError**

`error.rs`:
```rust
use thiserror::Error;

/// Windows platform errors wrapping HRESULT and other failure modes.
#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("HRESULT error: 0x{0:08X}")]
    HResult(u32),
    #[error("DXGI device lost — desktop switch or driver update")]
    DeviceLost,
    #[error("DXGI access lost — need to recreate output duplication")]
    AccessLost,
    #[error("DXGI wait timeout — no new frame available")]
    WaitTimeout,
    #[error("no DXGI adapter found")]
    NoAdapter,
    #[error("no DXGI output found for display {0}")]
    NoOutput(u32),
    #[error("D3D11 device creation failed")]
    DeviceCreationFailed,
    #[error("output duplication not supported (requires Windows 8+, no RDP)")]
    DuplicationNotSupported,
    #[error("texture pool exhausted ({0} textures in flight)")]
    PoolExhausted(usize),
    #[error("NVENC error: {0}")]
    NvencError(String),
    #[error("NVENC not available — no NVIDIA GPU or driver too old")]
    NvencNotAvailable,
    #[error("unsupported texture format: {0:?}")]
    UnsupportedFormat(prism_display::TextureFormat),
    #[error("{0}")]
    Other(String),
}

impl PlatformError {
    /// Convert a Windows HRESULT to PlatformError.
    /// Recognizes DXGI-specific codes for device lost and access lost.
    pub fn from_hresult(hr: u32) -> Self {
        match hr {
            0x887A0005 => PlatformError::DeviceLost,       // DXGI_ERROR_DEVICE_REMOVED
            0x887A0026 => PlatformError::DeviceLost,       // DXGI_ERROR_DEVICE_RESET
            0x887A0021 => PlatformError::AccessLost,       // DXGI_ERROR_ACCESS_LOST
            0x887A0027 => PlatformError::WaitTimeout,      // DXGI_ERROR_WAIT_TIMEOUT
            0x887A0022 => PlatformError::DuplicationNotSupported, // DXGI_ERROR_UNSUPPORTED
            _ => PlatformError::HResult(hr),
        }
    }

    /// Whether this error is recoverable by recreating the output duplication.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, PlatformError::AccessLost | PlatformError::WaitTimeout)
    }

    /// Whether this error requires full device recreation.
    pub fn is_device_lost(&self) -> bool {
        matches!(self, PlatformError::DeviceLost)
    }
}

impl From<PlatformError> for prism_display::CaptureError {
    fn from(e: PlatformError) -> Self {
        match e {
            PlatformError::DuplicationNotSupported => prism_display::CaptureError::Unsupported,
            PlatformError::NoOutput(id) => prism_display::CaptureError::DisplayNotFound(
                prism_display::DisplayId(id),
            ),
            other => prism_display::CaptureError::Failed(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hresult_device_removed() {
        let err = PlatformError::from_hresult(0x887A0005);
        assert!(matches!(err, PlatformError::DeviceLost));
        assert!(err.is_device_lost());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn hresult_access_lost() {
        let err = PlatformError::from_hresult(0x887A0021);
        assert!(matches!(err, PlatformError::AccessLost));
        assert!(err.is_recoverable());
    }

    #[test]
    fn hresult_wait_timeout() {
        let err = PlatformError::from_hresult(0x887A0027);
        assert!(matches!(err, PlatformError::WaitTimeout));
        assert!(err.is_recoverable());
    }

    #[test]
    fn hresult_unknown_code() {
        let err = PlatformError::from_hresult(0x80004005);
        assert!(matches!(err, PlatformError::HResult(0x80004005)));
        assert!(!err.is_recoverable());
        assert!(!err.is_device_lost());
    }

    #[test]
    fn converts_to_capture_error() {
        let platform_err = PlatformError::DuplicationNotSupported;
        let capture_err: prism_display::CaptureError = platform_err.into();
        assert!(matches!(capture_err, prism_display::CaptureError::Unsupported));
    }

    #[test]
    fn error_display_messages() {
        assert_eq!(
            PlatformError::DeviceLost.to_string(),
            "DXGI device lost — desktop switch or driver update"
        );
        assert_eq!(
            PlatformError::PoolExhausted(4).to_string(),
            "texture pool exhausted (4 textures in flight)"
        );
    }
}
```

- [ ] **Step 5: Verify, commit**

```bash
cargo check -p prism-platform-windows
cargo test -p prism-platform-windows
git add crates/prism-platform-windows/ Cargo.toml
git commit -m "feat(windows): scaffold prism-platform-windows crate with PlatformError"
```

---

## Task 2: TexturePool (Double-Buffered GPU Textures)

**Files:**
- Modify: `crates/prism-platform-windows/src/texture.rs`

The texture pool manages pre-allocated GPU textures for zero-allocation capture. This is testable without a GPU using the pool management logic in isolation.

- [ ] **Step 1: Write tests + implement TexturePool**

```rust
use prism_display::{SharedTexture, TextureFormat, DisplayId};

/// Configuration for the texture pool.
#[derive(Debug, Clone)]
pub struct TexturePoolConfig {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub pool_size: usize,
}

impl TexturePoolConfig {
    pub fn for_display(width: u32, height: u32) -> Self {
        Self { width, height, format: TextureFormat::Bgra8, pool_size: 4 }
    }
}

/// Index into the texture pool's ring buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlot(pub usize);

/// Manages pre-allocated texture slots for double-buffered capture.
/// Pure management logic — actual GPU texture creation is platform-specific.
pub struct TexturePool {
    slots: Vec<TextureSlotState>,
    write_idx: usize,
    read_idx: usize,
    config: TexturePoolConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextureSlotState {
    Free,
    Writing,
    Ready,
    Reading,
}

impl TexturePool {
    pub fn new(config: TexturePoolConfig) -> Self {
        let pool_size = config.pool_size;
        Self {
            slots: vec![TextureSlotState::Free; pool_size],
            write_idx: 0,
            read_idx: 0,
            config,
        }
    }

    /// Acquire a slot for writing (capture). Returns None if all slots are busy.
    pub fn acquire_write(&mut self) -> Option<TextureSlot> {
        for i in 0..self.slots.len() {
            let idx = (self.write_idx + i) % self.slots.len();
            if self.slots[idx] == TextureSlotState::Free {
                self.slots[idx] = TextureSlotState::Writing;
                self.write_idx = (idx + 1) % self.slots.len();
                return Some(TextureSlot(idx));
            }
        }
        None
    }

    /// Mark a write slot as ready for reading (capture complete).
    pub fn commit_write(&mut self, slot: TextureSlot) {
        assert_eq!(self.slots[slot.0], TextureSlotState::Writing);
        self.slots[slot.0] = TextureSlotState::Ready;
    }

    /// Acquire a slot for reading (encoder). Returns None if no ready slots.
    pub fn acquire_read(&mut self) -> Option<TextureSlot> {
        for i in 0..self.slots.len() {
            let idx = (self.read_idx + i) % self.slots.len();
            if self.slots[idx] == TextureSlotState::Ready {
                self.slots[idx] = TextureSlotState::Reading;
                self.read_idx = (idx + 1) % self.slots.len();
                return Some(TextureSlot(idx));
            }
        }
        None
    }

    /// Release a read slot back to the pool.
    pub fn release_read(&mut self, slot: TextureSlot) {
        assert_eq!(self.slots[slot.0], TextureSlotState::Reading);
        self.slots[slot.0] = TextureSlotState::Free;
    }

    /// Drop a write slot without committing (capture failed/skipped).
    pub fn abandon_write(&mut self, slot: TextureSlot) {
        assert_eq!(self.slots[slot.0], TextureSlotState::Writing);
        self.slots[slot.0] = TextureSlotState::Free;
    }

    /// Build a SharedTexture for a given slot (uses config dimensions + handle placeholder).
    pub fn shared_texture(&self, slot: TextureSlot, handle: u64) -> SharedTexture {
        SharedTexture {
            handle,
            width: self.config.width,
            height: self.config.height,
            format: self.config.format,
        }
    }

    pub fn pool_size(&self) -> usize { self.config.pool_size }

    pub fn free_count(&self) -> usize {
        self.slots.iter().filter(|s| **s == TextureSlotState::Free).count()
    }

    pub fn ready_count(&self) -> usize {
        self.slots.iter().filter(|s| **s == TextureSlotState::Ready).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_write_from_empty_pool() {
        let mut pool = TexturePool::new(TexturePoolConfig::for_display(1920, 1080));
        let slot = pool.acquire_write();
        assert!(slot.is_some());
        assert_eq!(pool.free_count(), 3); // 4 - 1 writing
    }

    #[test]
    fn commit_makes_ready() {
        let mut pool = TexturePool::new(TexturePoolConfig::for_display(1920, 1080));
        let slot = pool.acquire_write().unwrap();
        pool.commit_write(slot);
        assert_eq!(pool.ready_count(), 1);
    }

    #[test]
    fn read_after_commit() {
        let mut pool = TexturePool::new(TexturePoolConfig::for_display(1920, 1080));
        let slot = pool.acquire_write().unwrap();
        pool.commit_write(slot);
        let read_slot = pool.acquire_read().unwrap();
        assert_eq!(read_slot.0, slot.0);
    }

    #[test]
    fn release_read_frees_slot() {
        let mut pool = TexturePool::new(TexturePoolConfig::for_display(1920, 1080));
        let slot = pool.acquire_write().unwrap();
        pool.commit_write(slot);
        let read_slot = pool.acquire_read().unwrap();
        pool.release_read(read_slot);
        assert_eq!(pool.free_count(), 4);
    }

    #[test]
    fn pool_exhaustion() {
        let config = TexturePoolConfig { pool_size: 2, ..TexturePoolConfig::for_display(1920, 1080) };
        let mut pool = TexturePool::new(config);
        let _s1 = pool.acquire_write().unwrap();
        let _s2 = pool.acquire_write().unwrap();
        assert!(pool.acquire_write().is_none()); // exhausted
    }

    #[test]
    fn abandon_write_frees_slot() {
        let mut pool = TexturePool::new(TexturePoolConfig::for_display(1920, 1080));
        let slot = pool.acquire_write().unwrap();
        pool.abandon_write(slot);
        assert_eq!(pool.free_count(), 4);
    }

    #[test]
    fn double_buffer_flow() {
        // Simulate capture→encode double-buffering
        let config = TexturePoolConfig { pool_size: 2, ..TexturePoolConfig::for_display(1920, 1080) };
        let mut pool = TexturePool::new(config);

        // Frame 1: capture to slot 0
        let w0 = pool.acquire_write().unwrap();
        pool.commit_write(w0);

        // Frame 2: capture to slot 1 while encoder reads slot 0
        let w1 = pool.acquire_write().unwrap();
        let r0 = pool.acquire_read().unwrap();
        assert_eq!(r0.0, 0);
        assert_eq!(w1.0, 1);

        // Release slot 0 after encoding, commit slot 1
        pool.release_read(r0);
        pool.commit_write(w1);

        // Slot 0 is free again, slot 1 is ready
        assert_eq!(pool.free_count(), 1);
        assert_eq!(pool.ready_count(), 1);
    }

    #[test]
    fn shared_texture_from_slot() {
        let pool = TexturePool::new(TexturePoolConfig::for_display(2560, 1440));
        let tex = pool.shared_texture(TextureSlot(0), 0xDEADBEEF);
        assert_eq!(tex.width, 2560);
        assert_eq!(tex.height, 1440);
        assert_eq!(tex.handle, 0xDEADBEEF);
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
cargo test -p prism-platform-windows
git add crates/prism-platform-windows/src/texture.rs
git commit -m "feat(windows): TexturePool double-buffered GPU texture management"
```

---

## Task 3: Monitor Enumeration

**Files:**
- Modify: `crates/prism-platform-windows/src/monitor.rs`

Monitor enumeration logic with a platform-independent test using mock adapter data.

- [ ] **Step 1: Write tests + implement monitor types**

```rust
use prism_display::{DisplayId, MonitorInfo, Rect};

/// Raw adapter/output data extracted from DXGI enumeration.
/// This struct holds the platform-independent data; actual DXGI calls
/// populate it on Windows.
#[derive(Debug, Clone)]
pub struct DxgiOutputInfo {
    pub adapter_index: u32,
    pub output_index: u32,
    pub name: String,
    pub resolution: (u32, u32),
    pub position: (i32, i32),
    pub refresh_rate: u8,
    pub is_primary: bool,
    pub rotation: OutputRotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputRotation {
    Identity,
    Rotate90,
    Rotate180,
    Rotate270,
}

impl DxgiOutputInfo {
    /// Convert to PRISM MonitorInfo.
    pub fn to_monitor_info(&self) -> MonitorInfo {
        MonitorInfo {
            display_id: DisplayId(self.adapter_index * 16 + self.output_index),
            name: self.name.clone(),
            resolution: self.effective_resolution(),
            position: self.position,
            scale_factor: 1.0,
            refresh_rate: self.refresh_rate,
            primary: self.is_primary,
            is_virtual: false,
        }
    }

    /// Resolution accounting for rotation.
    pub fn effective_resolution(&self) -> (u32, u32) {
        match self.rotation {
            OutputRotation::Rotate90 | OutputRotation::Rotate270 => {
                (self.resolution.1, self.resolution.0)
            }
            _ => self.resolution,
        }
    }

    /// Encode adapter_index + output_index into a DisplayId.
    pub fn display_id(&self) -> DisplayId {
        DisplayId(self.adapter_index * 16 + self.output_index)
    }
}

/// Parse adapter and output index from a DisplayId.
pub fn parse_display_id(id: DisplayId) -> (u32, u32) {
    (id.0 / 16, id.0 % 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_output() -> DxgiOutputInfo {
        DxgiOutputInfo {
            adapter_index: 0,
            output_index: 0,
            name: "DELL U2723QE".to_string(),
            resolution: (3840, 2160),
            position: (0, 0),
            refresh_rate: 60,
            is_primary: true,
            rotation: OutputRotation::Identity,
        }
    }

    #[test]
    fn to_monitor_info() {
        let output = sample_output();
        let info = output.to_monitor_info();
        assert_eq!(info.display_id, DisplayId(0));
        assert_eq!(info.resolution, (3840, 2160));
        assert!(info.primary);
        assert_eq!(info.refresh_rate, 60);
    }

    #[test]
    fn rotation_swaps_dimensions() {
        let mut output = sample_output();
        output.rotation = OutputRotation::Rotate90;
        assert_eq!(output.effective_resolution(), (2160, 3840));
    }

    #[test]
    fn display_id_encoding() {
        let output = DxgiOutputInfo {
            adapter_index: 1, output_index: 2, ..sample_output()
        };
        assert_eq!(output.display_id(), DisplayId(18)); // 1*16 + 2
        let (adapter, out) = parse_display_id(DisplayId(18));
        assert_eq!((adapter, out), (1, 2));
    }

    #[test]
    fn identity_rotation_preserves_dimensions() {
        let output = sample_output();
        assert_eq!(output.effective_resolution(), (3840, 2160));
    }

    #[test]
    fn secondary_monitor() {
        let output = DxgiOutputInfo {
            adapter_index: 0,
            output_index: 1,
            name: "LG 27GP850".to_string(),
            resolution: (2560, 1440),
            position: (3840, 0),
            refresh_rate: 165,
            is_primary: false,
            rotation: OutputRotation::Identity,
        };
        let info = output.to_monitor_info();
        assert_eq!(info.display_id, DisplayId(1));
        assert!(!info.primary);
        assert_eq!(info.position, (3840, 0));
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-platform-windows/src/monitor.rs
git commit -m "feat(windows): monitor enumeration with rotation and display ID encoding"
```

---

## Task 4: DDA Damage Rect Extraction

**Files:**
- Modify: `crates/prism-platform-windows/src/damage.rs`

DDA returns dirty rects as `RECT` arrays. This module converts them to PRISM `Rect` and merges with the existing `merge_damage_rects` from prism-display.

- [ ] **Step 1: Write tests + implement DDA damage extraction**

```rust
use prism_display::{Rect, merge_damage_rects};

/// A raw DXGI dirty rect (same layout as Win32 RECT: left, top, right, bottom).
#[derive(Debug, Clone, Copy)]
pub struct DxgiRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl DxgiRect {
    /// Convert DXGI RECT (left/top/right/bottom) to PRISM Rect (x/y/w/h).
    pub fn to_prism_rect(&self) -> Rect {
        Rect {
            x: self.left,
            y: self.top,
            w: (self.right - self.left).max(0) as u32,
            h: (self.bottom - self.top).max(0) as u32,
        }
    }
}

/// Convert a slice of DXGI dirty rects to merged PRISM rects.
/// Uses a 64px merge threshold to match the classifier block size.
pub fn extract_damage(dxgi_rects: &[DxgiRect]) -> Vec<Rect> {
    let rects: Vec<Rect> = dxgi_rects.iter().map(|r| r.to_prism_rect()).collect();
    merge_damage_rects(&rects, 64)
}

/// Determine if the frame is a "full damage" frame (entire screen changed).
/// DDA sometimes reports no rects when the entire desktop composited.
pub fn is_full_damage(rects: &[DxgiRect], screen_width: u32, screen_height: u32) -> bool {
    if rects.is_empty() {
        return true; // DDA: empty rects = full frame
    }
    // Single rect covering the full screen
    if rects.len() == 1 {
        let r = &rects[0];
        return r.left == 0 && r.top == 0
            && r.right as u32 >= screen_width
            && r.bottom as u32 >= screen_height;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxgi_rect_to_prism() {
        let dxgi = DxgiRect { left: 10, top: 20, right: 110, bottom: 70 };
        let rect = dxgi.to_prism_rect();
        assert_eq!(rect, Rect { x: 10, y: 20, w: 100, h: 50 });
    }

    #[test]
    fn empty_rect_zero_size() {
        let dxgi = DxgiRect { left: 50, top: 50, right: 50, bottom: 50 };
        let rect = dxgi.to_prism_rect();
        assert_eq!(rect.w, 0);
        assert_eq!(rect.h, 0);
    }

    #[test]
    fn extract_and_merge() {
        let rects = vec![
            DxgiRect { left: 0, top: 0, right: 100, bottom: 50 },
            DxgiRect { left: 80, top: 0, right: 200, bottom: 50 }, // overlapping
            DxgiRect { left: 500, top: 500, right: 600, bottom: 600 }, // distant
        ];
        let merged = extract_damage(&rects);
        assert_eq!(merged.len(), 2); // first two merge, third stays
    }

    #[test]
    fn full_damage_empty_rects() {
        assert!(is_full_damage(&[], 1920, 1080));
    }

    #[test]
    fn full_damage_single_fullscreen() {
        let rects = vec![DxgiRect { left: 0, top: 0, right: 1920, bottom: 1080 }];
        assert!(is_full_damage(&rects, 1920, 1080));
    }

    #[test]
    fn partial_damage_not_full() {
        let rects = vec![DxgiRect { left: 10, top: 10, right: 100, bottom: 100 }];
        assert!(!is_full_damage(&rects, 1920, 1080));
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-platform-windows/src/damage.rs
git commit -m "feat(windows): DDA damage rect extraction with DXGI→PRISM conversion"
```

---

## Task 5: D3D11 Device Wrapper

**Files:**
- Modify: `crates/prism-platform-windows/src/d3d.rs`

Safe wrapper around D3D11 device creation. The actual COM calls are `#[cfg(windows)]`; the configuration and feature-level logic is testable.

- [ ] **Step 1: Write tests + implement D3D config types**

```rust
/// D3D11 feature level required for DDA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum D3DFeatureLevel {
    Level9_1,
    Level10_0,
    Level10_1,
    Level11_0,
    Level11_1,
    Level12_0,
    Level12_1,
}

impl D3DFeatureLevel {
    pub fn supports_dda(&self) -> bool {
        *self >= D3DFeatureLevel::Level11_0
    }

    pub fn supports_compute_shader(&self) -> bool {
        *self >= D3DFeatureLevel::Level11_0
    }

    /// Convert to the D3D11 SDK constant value.
    pub fn to_d3d_value(&self) -> u32 {
        match self {
            D3DFeatureLevel::Level9_1 => 0x9100,
            D3DFeatureLevel::Level10_0 => 0xa000,
            D3DFeatureLevel::Level10_1 => 0xa100,
            D3DFeatureLevel::Level11_0 => 0xb000,
            D3DFeatureLevel::Level11_1 => 0xb100,
            D3DFeatureLevel::Level12_0 => 0xc000,
            D3DFeatureLevel::Level12_1 => 0xc100,
        }
    }
}

/// Configuration for D3D11 device creation.
#[derive(Debug, Clone)]
pub struct D3DDeviceConfig {
    pub adapter_index: u32,
    pub min_feature_level: D3DFeatureLevel,
    pub debug_layer: bool,
}

impl Default for D3DDeviceConfig {
    fn default() -> Self {
        Self {
            adapter_index: 0,
            min_feature_level: D3DFeatureLevel::Level11_0,
            debug_layer: false,
        }
    }
}

impl D3DDeviceConfig {
    pub fn with_adapter(adapter_index: u32) -> Self {
        Self { adapter_index, ..Self::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_level_ordering() {
        assert!(D3DFeatureLevel::Level12_1 > D3DFeatureLevel::Level11_0);
        assert!(D3DFeatureLevel::Level11_0 > D3DFeatureLevel::Level10_1);
    }

    #[test]
    fn dda_requires_11_0() {
        assert!(!D3DFeatureLevel::Level10_1.supports_dda());
        assert!(D3DFeatureLevel::Level11_0.supports_dda());
        assert!(D3DFeatureLevel::Level12_0.supports_dda());
    }

    #[test]
    fn d3d_value_encoding() {
        assert_eq!(D3DFeatureLevel::Level11_0.to_d3d_value(), 0xb000);
        assert_eq!(D3DFeatureLevel::Level12_0.to_d3d_value(), 0xc000);
    }

    #[test]
    fn default_config() {
        let config = D3DDeviceConfig::default();
        assert_eq!(config.adapter_index, 0);
        assert_eq!(config.min_feature_level, D3DFeatureLevel::Level11_0);
        assert!(!config.debug_layer);
    }

    #[test]
    fn config_with_adapter() {
        let config = D3DDeviceConfig::with_adapter(1);
        assert_eq!(config.adapter_index, 1);
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-platform-windows/src/d3d.rs
git commit -m "feat(windows): D3D11 feature level types and device config"
```

---

## Task 6: NVENC Config Builder

**Files:**
- Modify: `crates/prism-platform-windows/src/nvenc/config.rs`
- Modify: `crates/prism-platform-windows/src/nvenc/mod.rs`

NVENC configuration with ultra-low-latency presets. Pure data — no FFI.

- [ ] **Step 1: Write tests + implement NvencConfig**

`nvenc/mod.rs`:
```rust
pub mod config;
pub mod session;
pub mod format;
```

`nvenc/config.rs`:
```rust
use prism_display::{CodecId, TextureFormat};

/// NVENC rate control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateControlMode {
    Cbr,
    Vbr,
    ConstQp,
    Lossless,
}

/// NVENC tuning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningMode {
    UltraLowLatency,
    LowLatency,
    HighQuality,
    Lossless,
}

/// NVENC encoder configuration.
#[derive(Debug, Clone)]
pub struct NvencConfig {
    pub codec: CodecId,
    pub width: u32,
    pub height: u32,
    pub max_fps: u8,
    pub bitrate_bps: u64,
    pub max_bitrate_bps: u64,
    pub tuning: TuningMode,
    pub rate_control: RateControlMode,
    pub b_frames: u8,
    pub lookahead: u8,
    pub gop_length: u32,
    pub min_qp: u8,
    pub max_qp: u8,
    pub slice_count: u8,
    pub input_format: TextureFormat,
}

impl NvencConfig {
    /// Ultra-low-latency preset matching spec Section 4.4.
    /// No B-frames, no lookahead, infinite GOP, CBR, 1 slice.
    pub fn ultra_low_latency(codec: CodecId, width: u32, height: u32, bitrate_bps: u64) -> Self {
        Self {
            codec,
            width,
            height,
            max_fps: 60,
            bitrate_bps,
            max_bitrate_bps: bitrate_bps * 2,
            tuning: TuningMode::UltraLowLatency,
            rate_control: RateControlMode::Cbr,
            b_frames: 0,
            lookahead: 0,
            gop_length: u32::MAX, // infinite
            min_qp: 18,
            max_qp: 51,
            slice_count: 1,
            input_format: TextureFormat::Nv12,
        }
    }

    /// Lossless preset for text regions.
    pub fn lossless(codec: CodecId, width: u32, height: u32) -> Self {
        Self {
            codec,
            width,
            height,
            max_fps: 60,
            bitrate_bps: 0,
            max_bitrate_bps: 0,
            tuning: TuningMode::Lossless,
            rate_control: RateControlMode::Lossless,
            b_frames: 0,
            lookahead: 0,
            gop_length: u32::MAX,
            min_qp: 0,
            max_qp: 0,
            slice_count: 1,
            input_format: TextureFormat::Bgra8,
        }
    }

    /// Multi-slice configuration for streaming (Section 4.6).
    pub fn with_slices(mut self, count: u8) -> Self {
        self.slice_count = count;
        self
    }

    /// Override max FPS (for profile/degradation changes).
    pub fn with_max_fps(mut self, fps: u8) -> Self {
        self.max_fps = fps;
        self
    }

    /// Whether this config requires encoder reinitialization vs. dynamic parameter change.
    pub fn needs_reinit(&self, other: &NvencConfig) -> bool {
        self.codec != other.codec
            || self.width != other.width
            || self.height != other.height
            || self.input_format != other.input_format
            || self.slice_count != other.slice_count
    }

    /// Whether this is just a bitrate/QP change (cheap, no reinit).
    pub fn is_bitrate_change_only(&self, other: &NvencConfig) -> bool {
        !self.needs_reinit(other)
            && (self.bitrate_bps != other.bitrate_bps
                || self.max_bitrate_bps != other.max_bitrate_bps
                || self.min_qp != other.min_qp
                || self.max_qp != other.max_qp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ultra_low_latency_preset() {
        let config = NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 20_000_000);
        assert_eq!(config.b_frames, 0);
        assert_eq!(config.lookahead, 0);
        assert_eq!(config.gop_length, u32::MAX);
        assert_eq!(config.rate_control, RateControlMode::Cbr);
        assert_eq!(config.min_qp, 18);
        assert_eq!(config.tuning, TuningMode::UltraLowLatency);
        assert_eq!(config.input_format, TextureFormat::Nv12);
    }

    #[test]
    fn lossless_preset() {
        let config = NvencConfig::lossless(CodecId::H264, 1920, 1080);
        assert_eq!(config.rate_control, RateControlMode::Lossless);
        assert_eq!(config.bitrate_bps, 0);
        assert_eq!(config.input_format, TextureFormat::Bgra8);
    }

    #[test]
    fn with_slices() {
        let config = NvencConfig::ultra_low_latency(CodecId::H265, 3840, 2160, 80_000_000)
            .with_slices(4);
        assert_eq!(config.slice_count, 4);
    }

    #[test]
    fn needs_reinit_on_resolution_change() {
        let a = NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 20_000_000);
        let b = NvencConfig::ultra_low_latency(CodecId::H264, 1280, 720, 20_000_000);
        assert!(a.needs_reinit(&b));
    }

    #[test]
    fn no_reinit_on_bitrate_change() {
        let a = NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 20_000_000);
        let b = NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 10_000_000);
        assert!(!a.needs_reinit(&b));
        assert!(a.is_bitrate_change_only(&b));
    }

    #[test]
    fn reinit_on_codec_change() {
        let a = NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 20_000_000);
        let b = NvencConfig::ultra_low_latency(CodecId::H265, 1920, 1080, 20_000_000);
        assert!(a.needs_reinit(&b));
    }

    #[test]
    fn reinit_on_format_change() {
        let a = NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 20_000_000);
        let mut b = a.clone();
        b.input_format = TextureFormat::Bgra8;
        assert!(a.needs_reinit(&b));
    }

    #[test]
    fn with_max_fps() {
        let config = NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 20_000_000)
            .with_max_fps(120);
        assert_eq!(config.max_fps, 120);
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-platform-windows/src/nvenc/
git commit -m "feat(windows): NvencConfig with ultra-low-latency and lossless presets"
```

---

## Task 7: NVENC Format Conversion Types + Session Lifecycle Types

**Files:**
- Modify: `crates/prism-platform-windows/src/nvenc/format.rs`
- Modify: `crates/prism-platform-windows/src/nvenc/session.rs`

- [ ] **Step 1: Write tests + implement format conversion**

`nvenc/format.rs`:
```rust
use prism_display::TextureFormat;

/// NVENC buffer format (maps to NV_ENC_BUFFER_FORMAT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvencBufferFormat {
    Nv12,
    Argb,
    Abgr,
    P010,
}

impl NvencBufferFormat {
    /// Convert from PRISM TextureFormat.
    pub fn from_texture_format(format: TextureFormat) -> Option<Self> {
        match format {
            TextureFormat::Nv12 => Some(NvencBufferFormat::Nv12),
            TextureFormat::Bgra8 => Some(NvencBufferFormat::Argb), // BGRA → ARGB for NVENC
            TextureFormat::P010 => Some(NvencBufferFormat::P010),
        }
    }

    /// NVENC SDK buffer format constant.
    pub fn to_nvenc_value(&self) -> u32 {
        match self {
            NvencBufferFormat::Nv12 => 1,   // NV_ENC_BUFFER_FORMAT_NV12
            NvencBufferFormat::Argb => 10,  // NV_ENC_BUFFER_FORMAT_ARGB
            NvencBufferFormat::Abgr => 16,  // NV_ENC_BUFFER_FORMAT_ABGR
            NvencBufferFormat::P010 => 24,  // NV_ENC_BUFFER_FORMAT_YUV420_10BIT
        }
    }

    /// Bytes per pixel (for buffer size calculation).
    pub fn bytes_per_pixel(&self) -> f32 {
        match self {
            NvencBufferFormat::Nv12 => 1.5,
            NvencBufferFormat::Argb | NvencBufferFormat::Abgr => 4.0,
            NvencBufferFormat::P010 => 3.0,
        }
    }

    /// Calculate buffer size for a given resolution.
    pub fn buffer_size(&self, width: u32, height: u32) -> usize {
        (width as f64 * height as f64 * self.bytes_per_pixel() as f64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nv12_from_texture_format() {
        assert_eq!(
            NvencBufferFormat::from_texture_format(TextureFormat::Nv12),
            Some(NvencBufferFormat::Nv12)
        );
    }

    #[test]
    fn bgra_maps_to_argb() {
        assert_eq!(
            NvencBufferFormat::from_texture_format(TextureFormat::Bgra8),
            Some(NvencBufferFormat::Argb)
        );
    }

    #[test]
    fn buffer_size_nv12_1080p() {
        let size = NvencBufferFormat::Nv12.buffer_size(1920, 1080);
        assert_eq!(size, 3_110_400); // 1920 * 1080 * 1.5
    }

    #[test]
    fn buffer_size_argb_4k() {
        let size = NvencBufferFormat::Argb.buffer_size(3840, 2160);
        assert_eq!(size, 33_177_600); // 3840 * 2160 * 4
    }

    #[test]
    fn nvenc_format_values() {
        assert_eq!(NvencBufferFormat::Nv12.to_nvenc_value(), 1);
        assert_eq!(NvencBufferFormat::Argb.to_nvenc_value(), 10);
    }
}
```

- [ ] **Step 2: Write tests + implement session lifecycle types**

`nvenc/session.rs`:
```rust
use super::config::NvencConfig;
use prism_display::{CodecId, EncodedSlice};
use bytes::Bytes;

/// NVENC session state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvencSessionState {
    Uninitialized,
    Ready,
    Encoding,
    Flushing,
    Error,
}

/// Result of an encode operation.
#[derive(Debug)]
pub enum EncodeResult {
    /// Frame encoded successfully. Contains output slices.
    Encoded { slices: Vec<EncodedSlice>, is_keyframe: bool },
    /// Encoder needs more input before producing output (pipeline fill).
    NeedsMoreInput,
    /// Encoder flushed, no more output.
    Flushed,
}

/// Tracks NVENC session statistics.
#[derive(Debug, Clone, Default)]
pub struct NvencStats {
    pub frames_encoded: u64,
    pub keyframes_encoded: u64,
    pub total_bytes_out: u64,
    pub avg_encode_time_us: u64,
    pub last_encode_time_us: u64,
}

impl NvencStats {
    pub fn record_frame(&mut self, bytes: usize, is_keyframe: bool, encode_time_us: u64) {
        self.frames_encoded += 1;
        if is_keyframe { self.keyframes_encoded += 1; }
        self.total_bytes_out += bytes as u64;
        self.last_encode_time_us = encode_time_us;
        // Running average
        if self.frames_encoded > 1 {
            self.avg_encode_time_us = (self.avg_encode_time_us * (self.frames_encoded - 1) + encode_time_us) / self.frames_encoded;
        } else {
            self.avg_encode_time_us = encode_time_us;
        }
    }

    pub fn avg_bitrate_bps(&self) -> u64 {
        if self.frames_encoded == 0 { return 0; }
        // Approximate: total_bytes * 8 / frames * fps
        // Caller should use actual timing, this is just a rough estimate
        self.total_bytes_out * 8 / self.frames_encoded.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_states() {
        assert_eq!(NvencSessionState::Uninitialized, NvencSessionState::Uninitialized);
        assert_ne!(NvencSessionState::Ready, NvencSessionState::Encoding);
    }

    #[test]
    fn stats_record_frame() {
        let mut stats = NvencStats::default();
        stats.record_frame(10_000, false, 2000);
        stats.record_frame(8_000, false, 1500);
        stats.record_frame(50_000, true, 5000);
        assert_eq!(stats.frames_encoded, 3);
        assert_eq!(stats.keyframes_encoded, 1);
        assert_eq!(stats.total_bytes_out, 68_000);
        assert_eq!(stats.last_encode_time_us, 5000);
    }

    #[test]
    fn stats_avg_encode_time() {
        let mut stats = NvencStats::default();
        stats.record_frame(1000, false, 1000);
        assert_eq!(stats.avg_encode_time_us, 1000);
        stats.record_frame(1000, false, 3000);
        assert_eq!(stats.avg_encode_time_us, 2000); // (1000 + 3000) / 2
    }

    #[test]
    fn stats_default_is_zero() {
        let stats = NvencStats::default();
        assert_eq!(stats.frames_encoded, 0);
        assert_eq!(stats.avg_bitrate_bps(), 0);
    }
}
```

- [ ] **Step 3: Update lib.rs re-exports**

```rust
pub mod error;
pub mod d3d;
pub mod texture;
pub mod dda;
pub mod monitor;
pub mod damage;
pub mod nvenc;

pub use error::PlatformError;
pub use d3d::{D3DFeatureLevel, D3DDeviceConfig};
pub use texture::{TexturePool, TexturePoolConfig, TextureSlot};
pub use monitor::{DxgiOutputInfo, OutputRotation, parse_display_id};
pub use damage::{DxgiRect, extract_damage, is_full_damage};
pub use nvenc::config::NvencConfig;
pub use nvenc::session::{NvencSessionState, EncodeResult, NvencStats};
pub use nvenc::format::NvencBufferFormat;
```

- [ ] **Step 4: Run all tests, verify workspace**

Run: `cargo test -p prism-platform-windows`
Run: `cargo test --workspace`

- [ ] **Step 5: Commit**

```bash
git add crates/prism-platform-windows/
git commit -m "feat(windows): NVENC format conversion, session lifecycle types, stats tracking"
```

---

## Task 8: DDA Capture Skeleton + Full lib.rs

**Files:**
- Modify: `crates/prism-platform-windows/src/dda.rs`
- Modify: `crates/prism-platform-windows/src/lib.rs`

The actual DDA COM calls require Windows. This task creates the `DdaCapture` struct that implements `PlatformCapture` with the state machine and configuration, plus `#[cfg(windows)]` stubs for the COM methods that will be filled in when building on Windows.

- [ ] **Step 1: Write tests + implement DdaCapture state machine**

```rust
use std::time::{Duration, Instant};
use prism_display::*;
use crate::error::PlatformError;
use crate::texture::{TexturePool, TexturePoolConfig, TextureSlot};
use crate::monitor::DxgiOutputInfo;

/// DDA capture session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdaCaptureState {
    Stopped,
    Starting,
    Capturing,
    RecoveringAccess,
    RecoveringDevice,
    Error,
}

/// Configuration specific to DDA capture.
#[derive(Debug, Clone)]
pub struct DdaCaptureConfig {
    pub adapter_index: u32,
    pub output_index: u32,
    pub frame_timeout_ms: u32,
    pub pool_size: usize,
}

impl Default for DdaCaptureConfig {
    fn default() -> Self {
        Self {
            adapter_index: 0,
            output_index: 0,
            frame_timeout_ms: 100,
            pool_size: 4,
        }
    }
}

/// DDA capture implementation. State machine manages the DXGI output
/// duplication lifecycle including recovery from device lost / access lost.
pub struct DdaCapture {
    state: DdaCaptureState,
    config: DdaCaptureConfig,
    texture_pool: Option<TexturePool>,
    frame_seq: u32,
    capture_start: Option<Instant>,
    monitors: Vec<DxgiOutputInfo>,
    // Platform-specific handles would go here:
    // device: Option<ID3D11Device>,
    // context: Option<ID3D11DeviceContext>,
    // duplication: Option<IDXGIOutputDuplication>,
}

impl DdaCapture {
    pub fn new(config: DdaCaptureConfig) -> Self {
        Self {
            state: DdaCaptureState::Stopped,
            config,
            texture_pool: None,
            frame_seq: 0,
            capture_start: None,
            monitors: Vec::new(),
        }
    }

    pub fn state(&self) -> DdaCaptureState { self.state }
    pub fn frame_count(&self) -> u32 { self.frame_seq }

    /// Transition to a new state. Returns the previous state.
    fn transition(&mut self, new_state: DdaCaptureState) -> DdaCaptureState {
        let old = self.state;
        self.state = new_state;
        old
    }

    /// Handle a DDA error by transitioning to the appropriate recovery state.
    pub fn handle_error(&mut self, err: &PlatformError) -> DdaCaptureState {
        if err.is_device_lost() {
            self.transition(DdaCaptureState::RecoveringDevice)
        } else if err.is_recoverable() {
            self.transition(DdaCaptureState::RecoveringAccess)
        } else {
            self.transition(DdaCaptureState::Error)
        }
    }

    /// Initialize the texture pool for a given resolution.
    pub fn init_pool(&mut self, width: u32, height: u32) {
        let pool_config = TexturePoolConfig {
            width,
            height,
            format: TextureFormat::Bgra8,
            pool_size: self.config.pool_size,
        };
        self.texture_pool = Some(TexturePool::new(pool_config));
    }

    /// Acquire a texture slot for capture.
    pub fn acquire_texture(&mut self) -> Option<TextureSlot> {
        self.texture_pool.as_mut()?.acquire_write()
    }

    /// Build a CapturedFrame from a completed capture.
    pub fn build_frame(
        &mut self,
        slot: TextureSlot,
        handle: u64,
        damage_rects: Vec<Rect>,
        is_input_triggered: bool,
    ) -> CapturedFrame {
        let pool = self.texture_pool.as_ref().unwrap();
        let texture = pool.shared_texture(slot, handle);
        let capture_time_us = self.capture_start
            .map(|s| s.elapsed().as_micros() as u64)
            .unwrap_or(0);
        let seq = self.frame_seq;
        self.frame_seq += 1;
        CapturedFrame {
            texture,
            damage_rects,
            display_id: DisplayId(self.config.adapter_index * 16 + self.config.output_index),
            capture_time_us,
            frame_seq: seq,
            is_input_triggered,
            is_speculative: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_stopped() {
        let cap = DdaCapture::new(DdaCaptureConfig::default());
        assert_eq!(cap.state(), DdaCaptureState::Stopped);
        assert_eq!(cap.frame_count(), 0);
    }

    #[test]
    fn handle_device_lost_transitions() {
        let mut cap = DdaCapture::new(DdaCaptureConfig::default());
        cap.transition(DdaCaptureState::Capturing);
        let new = cap.handle_error(&PlatformError::DeviceLost);
        assert_eq!(new, DdaCaptureState::RecoveringDevice);
    }

    #[test]
    fn handle_access_lost_transitions() {
        let mut cap = DdaCapture::new(DdaCaptureConfig::default());
        cap.transition(DdaCaptureState::Capturing);
        let new = cap.handle_error(&PlatformError::AccessLost);
        assert_eq!(new, DdaCaptureState::RecoveringAccess);
    }

    #[test]
    fn handle_fatal_error_transitions() {
        let mut cap = DdaCapture::new(DdaCaptureConfig::default());
        let new = cap.handle_error(&PlatformError::Other("fatal".into()));
        assert_eq!(new, DdaCaptureState::Error);
    }

    #[test]
    fn texture_pool_initialization() {
        let mut cap = DdaCapture::new(DdaCaptureConfig::default());
        cap.init_pool(1920, 1080);
        let slot = cap.acquire_texture();
        assert!(slot.is_some());
    }

    #[test]
    fn build_frame_increments_sequence() {
        let mut cap = DdaCapture::new(DdaCaptureConfig::default());
        cap.init_pool(1920, 1080);
        cap.capture_start = Some(Instant::now());

        let slot = cap.acquire_texture().unwrap();
        cap.texture_pool.as_mut().unwrap().commit_write(slot);
        let frame1 = cap.build_frame(slot, 0x100, vec![], false);
        assert_eq!(frame1.frame_seq, 0);

        let slot2 = cap.acquire_texture().unwrap();
        cap.texture_pool.as_mut().unwrap().commit_write(slot2);
        let frame2 = cap.build_frame(slot2, 0x200, vec![], false);
        assert_eq!(frame2.frame_seq, 1);
    }

    #[test]
    fn build_frame_display_id() {
        let config = DdaCaptureConfig { adapter_index: 1, output_index: 2, ..DdaCaptureConfig::default() };
        let mut cap = DdaCapture::new(config);
        cap.init_pool(1920, 1080);
        cap.capture_start = Some(Instant::now());
        let slot = cap.acquire_texture().unwrap();
        cap.texture_pool.as_mut().unwrap().commit_write(slot);
        let frame = cap.build_frame(slot, 0, vec![], false);
        assert_eq!(frame.display_id, DisplayId(18)); // 1*16 + 2
    }
}
```

- [ ] **Step 2: Update lib.rs with full re-exports**

Add to existing re-exports:

```rust
pub use dda::{DdaCapture, DdaCaptureConfig, DdaCaptureState};
```

- [ ] **Step 3: Run all tests, verify workspace**

Run: `cargo test -p prism-platform-windows`
Run: `cargo test --workspace`

- [ ] **Step 4: Commit**

```bash
git add crates/prism-platform-windows/
git commit -m "feat(windows): DdaCapture state machine with texture pool + error recovery"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Crate setup + PlatformError (HRESULT wrapping, recovery classification) | 6 |
| 2 | TexturePool (double-buffered GPU texture management) | 8 |
| 3 | Monitor enumeration (DxgiOutputInfo, rotation, DisplayId encoding) | 5 |
| 4 | DDA damage rect extraction (DXGI→PRISM conversion, full-damage detection) | 6 |
| 5 | D3D11 device config (feature levels, DDA requirements) | 5 |
| 6 | NvencConfig (ultra-low-latency, lossless, reinit detection) | 8 |
| 7 | NVENC format conversion + session lifecycle types + stats | 9 |
| 8 | DdaCapture state machine (error recovery, texture pool, frame building) | 7 |
| **Total** | | **~54** |

**What this plan delivers:**
- All safe Rust wrappers and pure logic for Windows DDA + NVENC
- Testable without Windows/GPU (all tests are pure computation)
- PlatformError with HRESULT recognition and recovery classification
- TexturePool double-buffering for zero-allocation capture
- NvencConfig with ultra-low-latency presets matching spec Section 4.4
- DdaCapture state machine with device-lost / access-lost recovery
- NVENC session lifecycle types and encode statistics

**What requires Windows hardware (next steps, not in this plan):**
- Actual COM calls: `CreateDXGIFactory1`, `D3D11CreateDevice`, `IDXGIOutput1::DuplicateOutput`
- Actual NVENC FFI: `NvEncCreateEncoder`, `NvEncEncodePicture`, `NvEncDestroyEncoder`
- GPU texture sharing via `CreateSharedHandle`
- `PlatformCapture` trait implementation wiring DdaCapture to the trait methods

These are `#[cfg(windows)]` blocks that call into the `windows` crate COM interfaces and NVENC SDK. They compile only on Windows and are tested with `cargo test -- --ignored` on actual hardware.
