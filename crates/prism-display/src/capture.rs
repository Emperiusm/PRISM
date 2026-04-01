// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Screen capture abstraction layer.
//!
//! Provides a platform-agnostic trait (`PlatformCapture`) plus configuration
//! and informational types needed by all capture back-ends.

use thiserror::Error;

use crate::types::{DisplayId, Rect};

// ── CaptureMode ──────────────────────────────────────────────────────────────

/// Selects the source to capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureMode {
    /// Capture the entire desktop for a given display.
    FullDesktop,
    /// Capture a specific application window identified by its HWND.
    Window { hwnd: u64 },
    /// Capture a virtual (software-only) display with the given resolution and
    /// refresh rate.
    Virtual {
        resolution: (u32, u32),
        refresh_rate: u8,
    },
}

// ── CursorCapture ─────────────────────────────────────────────────────────────

/// Controls how the hardware or OS cursor is handled during capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorCapture {
    /// Cursor pixels are blit directly into the captured frame.
    Embedded,
    /// Cursor metadata is delivered in a separate channel so the client can
    /// composite it independently.
    Separate,
    /// Cursor is suppressed; frames never contain cursor pixels.
    None,
}

// ── CaptureConfig ─────────────────────────────────────────────────────────────

/// Full configuration for a single capture session.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Which physical or virtual display to capture.
    pub display_id: DisplayId,
    /// What to capture on that display.
    pub capture_mode: CaptureMode,
    /// How the cursor should be handled.
    pub cursor: CursorCapture,
}

impl CaptureConfig {
    /// Create a config that captures the full desktop of `display_id`.
    pub fn new(display_id: DisplayId) -> Self {
        Self {
            display_id,
            capture_mode: CaptureMode::FullDesktop,
            cursor: CursorCapture::Embedded,
        }
    }
}

// ── MonitorInfo ───────────────────────────────────────────────────────────────

/// Metadata about a physical or virtual monitor returned by
/// [`PlatformCapture::enumerate_monitors`].
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Unique identifier for this display.
    pub display_id: DisplayId,
    /// Human-readable name (e.g. `"DELL U2720Q"`).
    pub name: String,
    /// Native resolution in pixels `(width, height)`.
    pub resolution: (u32, u32),
    /// Desktop position of the top-left corner of the monitor.
    pub position: (i32, i32),
    /// DPI scale factor (1.0 = 100 %, 1.5 = 150 %, …).
    pub scale_factor: f32,
    /// Vertical refresh rate in Hz.
    pub refresh_rate: u8,
    /// Whether this is the primary monitor.
    pub primary: bool,
    /// Whether this monitor is a virtual (software-defined) display.
    pub is_virtual: bool,
}

impl MonitorInfo {
    /// Convenience: return the monitor bounds as a [`Rect`].
    pub fn bounds(&self) -> Rect {
        Rect {
            x: self.position.0,
            y: self.position.1,
            w: self.resolution.0,
            h: self.resolution.1,
        }
    }
}

// ── DisplayConfig ─────────────────────────────────────────────────────────────

/// Desired configuration when creating or reconfiguring a virtual display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayConfig {
    /// Desired resolution `(width, height)` in pixels.
    pub resolution: (u32, u32),
    /// Desired refresh rate in Hz.
    pub refresh_rate: u8,
}

// ── CaptureError ──────────────────────────────────────────────────────────────

/// Errors that can be returned by a [`PlatformCapture`] implementation.
#[derive(Debug, Error)]
pub enum CaptureError {
    /// The requested operation is not supported on this platform.
    #[error("capture operation not supported on this platform")]
    Unsupported,
    /// The specified display could not be found.
    #[error("display {0:?} not found")]
    DisplayNotFound(DisplayId),
    /// A general back-end failure with a descriptive message.
    #[error("capture failed: {0}")]
    Failed(String),
}

// ── CapturedFrame ─────────────────────────────────────────────────────────────

/// A single captured frame returned by [`PlatformCapture::next_frame`].
#[derive(Debug)]
pub struct CapturedFrame {
    /// The display this frame was captured from.
    pub display_id: DisplayId,
    /// Width of the frame in pixels.
    pub width: u32,
    /// Height of the frame in pixels.
    pub height: u32,
    /// Raw pixel data (format is back-end specific).
    pub data: Vec<u8>,
}

// ── PlatformCapture trait ─────────────────────────────────────────────────────

/// Trait implemented by each platform capture back-end.
///
/// # Thread-safety
/// Implementations must be `Send + 'static` so they can be moved to a
/// dedicated capture thread.
pub trait PlatformCapture: Send + 'static {
    /// Begin a capture session using the given `config`.
    ///
    /// Returns an error if capture cannot be started (e.g. the display no
    /// longer exists or permissions are missing).
    fn start(&mut self, config: CaptureConfig) -> Result<(), CaptureError>;

    /// Stop an active capture session.  No-op when already stopped.
    fn stop(&mut self);

    /// Request a new frame to be produced asynchronously.  Calling this while
    /// already capturing is a hint that the consumer wants a fresh frame soon.
    fn trigger_capture(&mut self) -> Result<(), CaptureError>;

    /// Block until a captured frame is available and return it.
    ///
    /// Returns `None` when capture has been stopped cleanly.
    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError>;

    /// Return metadata for every monitor visible to this capture back-end.
    fn enumerate_monitors(&self) -> Result<Vec<MonitorInfo>, CaptureError>;

    /// Create a new virtual display with the given configuration.  Returns the
    /// [`DisplayId`] of the newly created display.
    fn create_virtual_display(&mut self, config: DisplayConfig) -> Result<DisplayId, CaptureError>;

    /// Destroy a previously created virtual display.
    fn destroy_virtual_display(&mut self, id: DisplayId) -> Result<(), CaptureError>;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_mode_variants() {
        let modes = [
            CaptureMode::FullDesktop,
            CaptureMode::Window { hwnd: 0xDEAD_BEEF },
            CaptureMode::Virtual {
                resolution: (1920, 1080),
                refresh_rate: 60,
            },
        ];

        assert_eq!(modes[0], CaptureMode::FullDesktop);

        if let CaptureMode::Window { hwnd } = modes[1] {
            assert_eq!(hwnd, 0xDEAD_BEEF);
        } else {
            panic!("expected Window variant");
        }

        if let CaptureMode::Virtual {
            resolution,
            refresh_rate,
        } = modes[2]
        {
            assert_eq!(resolution, (1920, 1080));
            assert_eq!(refresh_rate, 60);
        } else {
            panic!("expected Virtual variant");
        }

        // CursorCapture derives PartialEq/Eq
        assert_eq!(CursorCapture::Embedded, CursorCapture::Embedded);
        assert_ne!(CursorCapture::Embedded, CursorCapture::None);
    }

    #[test]
    fn monitor_info_defaults() {
        let info = MonitorInfo {
            display_id: DisplayId(1),
            name: "Test Monitor".to_string(),
            resolution: (2560, 1440),
            position: (-2560, 0),
            scale_factor: 1.0,
            refresh_rate: 144,
            primary: false,
            is_virtual: false,
        };

        assert_eq!(info.display_id, DisplayId(1));
        assert_eq!(info.resolution, (2560, 1440));
        assert_eq!(info.position, (-2560, 0));
        assert!((info.scale_factor - 1.0).abs() < f32::EPSILON);
        assert_eq!(info.refresh_rate, 144);
        assert!(!info.primary);
        assert!(!info.is_virtual);

        let bounds = info.bounds();
        assert_eq!(bounds.x, -2560);
        assert_eq!(bounds.y, 0);
        assert_eq!(bounds.w, 2560);
        assert_eq!(bounds.h, 1440);
    }

    #[test]
    fn capture_error_display() {
        let e1 = CaptureError::Unsupported;
        let e2 = CaptureError::DisplayNotFound(DisplayId(7));
        let e3 = CaptureError::Failed("timeout".to_string());

        let s1 = e1.to_string();
        let s2 = e2.to_string();
        let s3 = e3.to_string();

        assert!(s1.contains("not supported"), "got: {s1}");
        assert!(s2.contains("not found"), "got: {s2}");
        assert!(s3.contains("timeout"), "got: {s3}");
    }
}
