// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Desktop Duplication API (DDA) capture back-end.

use std::time::Instant;

use prism_display::{CapturedFrame, DisplayId, Rect};

use crate::error::PlatformError;
use crate::monitor::DxgiOutputInfo;
use crate::texture::{TexturePool, TexturePoolConfig, TextureSlot};

// ── DdaCaptureState ───────────────────────────────────────────────────────────

/// Lifecycle state of a [`DdaCapture`] instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdaCaptureState {
    /// Capture is not running and no resources are allocated.
    Stopped,
    /// Initialisation in progress (opening the duplication output).
    Starting,
    /// Actively duplicating frames from the display.
    Capturing,
    /// Desktop access was lost (e.g. UAC prompt); recovering without
    /// recreating the D3D device.
    RecoveringAccess,
    /// The D3D device was lost; full device teardown and recreation required.
    RecoveringDevice,
    /// An unrecoverable error occurred.
    Error,
}

// ── DdaCaptureConfig ──────────────────────────────────────────────────────────

/// Configuration for a [`DdaCapture`] session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdaCaptureConfig {
    /// Zero-based DXGI adapter index to use.
    pub adapter_index: u32,
    /// Zero-based index of the DXGI output (monitor) on the chosen adapter.
    pub output_index: u32,
    /// Timeout in milliseconds for `AcquireNextFrame`.  `0` means return
    /// immediately without blocking.
    pub frame_timeout_ms: u32,
    /// Number of texture slots to pre-allocate in the texture pool.
    pub pool_size: usize,
}

impl Default for DdaCaptureConfig {
    /// Sensible defaults: adapter 0, output 0, 100 ms timeout, 4-slot pool.
    fn default() -> Self {
        Self {
            adapter_index: 0,
            output_index: 0,
            frame_timeout_ms: 100,
            pool_size: 4,
        }
    }
}

// ── DdaCapture ────────────────────────────────────────────────────────────────

/// State machine for a Desktop Duplication API capture session.
///
/// `DdaCapture` tracks the lifecycle of a DDA session and coordinates the
/// texture pool, frame sequencing, and error recovery — without holding any
/// actual GPU objects (those live in the caller's device layer).
pub struct DdaCapture {
    state: DdaCaptureState,
    config: DdaCaptureConfig,
    texture_pool: Option<TexturePool>,
    frame_seq: u32,
    capture_start: Option<Instant>,
    /// Enumerated monitor metadata for this adapter/output.
    /// Populated when the caller enumerates outputs; reserved for future use.
    #[allow(dead_code)]
    monitors: Vec<DxgiOutputInfo>,
}

impl DdaCapture {
    /// Create a new `DdaCapture` in the [`DdaCaptureState::Stopped`] state.
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

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Current lifecycle state.
    pub fn state(&self) -> DdaCaptureState {
        self.state
    }

    /// Total number of frames produced since the session started.
    pub fn frame_count(&self) -> u32 {
        self.frame_seq
    }

    // ── State transitions ─────────────────────────────────────────────────────

    /// Move to `new_state`, returning the previous state.
    fn transition(&mut self, new_state: DdaCaptureState) -> DdaCaptureState {
        let old = self.state;
        self.state = new_state;
        old
    }

    /// Inspect a [`PlatformError`] and move to the appropriate recovery state.
    ///
    /// - `DeviceLost`  → [`DdaCaptureState::RecoveringDevice`]
    /// - recoverable (access lost / timeout) → [`DdaCaptureState::RecoveringAccess`]
    /// - anything else → [`DdaCaptureState::Error`]
    ///
    /// Returns the new state.
    pub fn handle_error(&mut self, err: &PlatformError) -> DdaCaptureState {
        let next = if err.is_device_lost() {
            DdaCaptureState::RecoveringDevice
        } else if err.is_recoverable() {
            DdaCaptureState::RecoveringAccess
        } else {
            DdaCaptureState::Error
        };
        self.transition(next);
        next
    }

    // ── Resource management ───────────────────────────────────────────────────

    /// Allocate a [`TexturePool`] sized to the given `width × height` and
    /// configured with `config.pool_size` slots.
    ///
    /// Any previously allocated pool is dropped.
    pub fn init_pool(&mut self, width: u32, height: u32) {
        let cfg = TexturePoolConfig {
            width,
            height,
            format: prism_display::TextureFormat::Bgra8,
            pool_size: self.config.pool_size,
        };
        self.texture_pool = Some(TexturePool::new(cfg));
    }

    /// Acquire a free texture slot from the pool, if one is available.
    ///
    /// Returns `None` when the pool is not yet initialised or all slots are
    /// occupied.
    pub fn acquire_texture(&mut self) -> Option<TextureSlot> {
        self.texture_pool.as_mut()?.acquire_write()
    }

    // ── Frame construction ────────────────────────────────────────────────────

    /// Build a [`CapturedFrame`] from an acquired texture slot.
    ///
    /// - Constructs a [`SharedTexture`] from the pool for `slot` + `handle`.
    /// - Increments the internal frame sequence counter.
    /// - Computes `capture_time_us` from the elapsed time since
    ///   `capture_start` (0 when no start time has been recorded).
    /// - Sets `display_id` from `config.adapter_index` and
    ///   `config.output_index` using the same encoding as [`DxgiOutputInfo`]:
    ///   `adapter * 16 + output`.
    pub fn build_frame(
        &mut self,
        slot: TextureSlot,
        handle: u64,
        damage_rects: Vec<Rect>,
        is_input_triggered: bool,
    ) -> CapturedFrame {
        let texture = self
            .texture_pool
            .as_ref()
            .expect("build_frame called before init_pool")
            .shared_texture(slot, handle);

        self.frame_seq += 1;

        let capture_time_us = self
            .capture_start
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0);

        let display_id = DisplayId(
            self.config.adapter_index * 16 + self.config.output_index,
        );

        CapturedFrame {
            texture,
            damage_rects,
            display_id,
            capture_time_us,
            frame_seq: self.frame_seq,
            is_input_triggered,
            is_speculative: false,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_capture() -> DdaCapture {
        DdaCapture::new(DdaCaptureConfig::default())
    }

    // ── State machine ─────────────────────────────────────────────────────────

    #[test]
    fn initial_stopped() {
        let cap = default_capture();
        assert_eq!(cap.state(), DdaCaptureState::Stopped);
        assert_eq!(cap.frame_count(), 0);
    }

    #[test]
    fn handle_device_lost() {
        let mut cap = default_capture();
        let new_state = cap.handle_error(&PlatformError::DeviceLost);
        assert_eq!(new_state, DdaCaptureState::RecoveringDevice);
        assert_eq!(cap.state(), DdaCaptureState::RecoveringDevice);
    }

    #[test]
    fn handle_access_lost() {
        let mut cap = default_capture();
        let new_state = cap.handle_error(&PlatformError::AccessLost);
        assert_eq!(new_state, DdaCaptureState::RecoveringAccess);
        assert_eq!(cap.state(), DdaCaptureState::RecoveringAccess);
    }

    #[test]
    fn handle_fatal_error() {
        let mut cap = default_capture();
        let new_state = cap.handle_error(&PlatformError::NvencNotAvailable);
        assert_eq!(new_state, DdaCaptureState::Error);
        assert_eq!(cap.state(), DdaCaptureState::Error);
    }

    // ── Texture pool ──────────────────────────────────────────────────────────

    #[test]
    fn texture_pool_init() {
        let mut cap = default_capture();
        // No pool yet → acquire returns None.
        assert!(cap.acquire_texture().is_none());

        cap.init_pool(1920, 1080);
        // Pool has pool_size=4 free slots.
        let slot = cap.acquire_texture();
        assert!(slot.is_some());
    }

    // ── Frame building ────────────────────────────────────────────────────────

    #[test]
    fn build_frame_increments_seq() {
        let mut cap = default_capture();
        cap.init_pool(1920, 1080);

        let slot0 = cap.acquire_texture().unwrap();
        let frame0 = cap.build_frame(slot0, 0xABCD, vec![], false);
        assert_eq!(frame0.frame_seq, 1);

        let slot1 = cap.acquire_texture().unwrap();
        let frame1 = cap.build_frame(slot1, 0x1234, vec![], true);
        assert_eq!(frame1.frame_seq, 2);
        assert!(frame1.is_input_triggered);
    }

    #[test]
    fn build_frame_display_id() {
        let cfg = DdaCaptureConfig {
            adapter_index: 1,
            output_index: 3,
            ..DdaCaptureConfig::default()
        };
        let mut cap = DdaCapture::new(cfg);
        cap.init_pool(1920, 1080);

        let slot = cap.acquire_texture().unwrap();
        let frame = cap.build_frame(slot, 0, vec![], false);
        // adapter=1, output=3 → DisplayId(1*16+3 = 19)
        assert_eq!(frame.display_id, DisplayId(19));
    }
}
