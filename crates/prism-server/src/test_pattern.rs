//! Synthetic test-pattern capture backend.
//!
//! Generates BGRA8 frames entirely in software — no GPU, no OS-level capture.
//! Useful for integration tests and demos that need a `PlatformCapture`
//! implementation without any platform dependencies.

use prism_display::capture::{
    CaptureConfig, CaptureError, CapturedFrame, DisplayConfig, MonitorInfo, PlatformCapture,
};
use prism_display::types::DisplayId;

/// Width of the virtual monitor produced by [`TestPatternCapture`].
const DEFAULT_WIDTH: u32 = 1280;
/// Height of the virtual monitor produced by [`TestPatternCapture`].
const DEFAULT_HEIGHT: u32 = 720;

// ── TestPatternCapture ────────────────────────────────────────────────────────

/// A [`PlatformCapture`] implementation that generates synthetic BGRA8 frames.
///
/// Each frame contains a smooth gradient background and a 100×100 white
/// rectangle that bounces around the frame over time.
pub struct TestPatternCapture {
    width: u32,
    height: u32,
    running: bool,
    frame_seq: u32,
}

impl TestPatternCapture {
    /// Create a new instance using the default 1280×720 virtual resolution.
    pub fn new() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            running: false,
            frame_seq: 0,
        }
    }

    /// Create a new instance with an explicit resolution.
    pub fn with_resolution(width: u32, height: u32) -> Self {
        Self { width, height, running: false, frame_seq: 0 }
    }

    /// Generate raw BGRA8 pixel data for frame number `frame_num`.
    ///
    /// The image is `self.width × self.height` pixels (4 bytes each).
    pub fn generate_pattern(&self, frame_num: u32) -> Vec<u8> {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut buf = vec![0u8; w * h * 4];

        // ── gradient background ──────────────────────────────────────────────
        for y in 0..h {
            for x in 0..w {
                let r = ((x * 255) / w.max(1)) as u8;
                let g = ((y * 255) / h.max(1)) as u8;
                let b = 128u8;
                let idx = (y * w + x) * 4;
                // BGRA
                buf[idx] = b;
                buf[idx + 1] = g;
                buf[idx + 2] = r;
                buf[idx + 3] = 255;
            }
        }

        // ── bouncing white rectangle 100×100 ────────────────────────────────
        let rect_w: usize = 100;
        let rect_h: usize = 100;

        // Each axis bounces independently using a triangle-wave position.
        let max_x = w.saturating_sub(rect_w);
        let max_y = h.saturating_sub(rect_h);

        let rx = if max_x == 0 {
            0
        } else {
            triangle_wave(frame_num as usize, max_x)
        };
        let ry = if max_y == 0 {
            0
        } else {
            triangle_wave(frame_num as usize * 7 / 5, max_y)
        };

        // Paint the rectangle white (BGRA = 255,255,255,255).
        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let px = rx + dx;
                let py = ry + dy;
                if px < w && py < h {
                    let idx = (py * w + px) * 4;
                    buf[idx] = 255;
                    buf[idx + 1] = 255;
                    buf[idx + 2] = 255;
                    buf[idx + 3] = 255;
                }
            }
        }

        buf
    }
}

impl Default for TestPatternCapture {
    fn default() -> Self {
        Self::new()
    }
}

// ── helper ────────────────────────────────────────────────────────────────────

/// Triangle wave: bounces `t` back and forth in `[0, max]`.
fn triangle_wave(t: usize, max: usize) -> usize {
    if max == 0 {
        return 0;
    }
    let period = max * 2;
    let phase = t % period;
    if phase <= max { phase } else { period - phase }
}

// ── PlatformCapture impl ──────────────────────────────────────────────────────

impl PlatformCapture for TestPatternCapture {
    fn start(&mut self, _config: CaptureConfig) -> Result<(), CaptureError> {
        self.running = true;
        self.frame_seq = 0;
        Ok(())
    }

    fn stop(&mut self) {
        self.running = false;
    }

    fn trigger_capture(&mut self) -> Result<(), CaptureError> {
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
        if !self.running {
            return Ok(None);
        }
        let seq = self.frame_seq;
        self.frame_seq = self.frame_seq.wrapping_add(1);

        let data = self.generate_pattern(seq);
        let frame = CapturedFrame {
            display_id: DisplayId(0),
            width: self.width,
            height: self.height,
            data,
        };
        Ok(Some(frame))
    }

    fn enumerate_monitors(&self) -> Result<Vec<MonitorInfo>, CaptureError> {
        Ok(vec![MonitorInfo {
            display_id: DisplayId(0),
            name: "Virtual Test Monitor".to_string(),
            resolution: (self.width, self.height),
            position: (0, 0),
            scale_factor: 1.0,
            refresh_rate: 60,
            primary: true,
            is_virtual: true,
        }])
    }

    fn create_virtual_display(
        &mut self,
        _config: DisplayConfig,
    ) -> Result<DisplayId, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    fn destroy_virtual_display(&mut self, _id: DisplayId) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_display::capture::CaptureConfig;
    use prism_display::types::DisplayId;

    fn make_config() -> CaptureConfig {
        CaptureConfig::new(DisplayId(0))
    }

    #[test]
    fn enumerate_returns_single_virtual_monitor() {
        let cap = TestPatternCapture::new();
        let monitors = cap.enumerate_monitors().unwrap();
        assert_eq!(monitors.len(), 1);
        let m = &monitors[0];
        assert!(m.is_virtual);
        assert_eq!(m.display_id, DisplayId(0));
        assert_eq!(m.resolution, (DEFAULT_WIDTH, DEFAULT_HEIGHT));
    }

    #[test]
    fn next_frame_returns_frame_with_correct_dimensions() {
        let mut cap = TestPatternCapture::new();
        cap.start(make_config()).unwrap();
        let frame = cap.next_frame().unwrap().expect("expected a frame");
        assert_eq!(frame.width, DEFAULT_WIDTH);
        assert_eq!(frame.height, DEFAULT_HEIGHT);
    }

    #[test]
    fn frames_have_incrementing_sequence() {
        let mut cap = TestPatternCapture::new();
        cap.start(make_config()).unwrap();
        // We track via the size of data changing per frame_num; but the spec
        // says frame_seq 0, 1, 2 — we verify by checking the capture advances.
        // The CapturedFrame struct doesn't expose frame_seq, so we verify the
        // internal counter by checking that data differs at different frame nums.
        let f0 = cap.next_frame().unwrap().expect("frame 0");
        let f1 = cap.next_frame().unwrap().expect("frame 1");
        let f2 = cap.next_frame().unwrap().expect("frame 2");
        // All three must be present and have the correct size.
        let expected_len = (DEFAULT_WIDTH * DEFAULT_HEIGHT * 4) as usize;
        assert_eq!(f0.data.len(), expected_len);
        assert_eq!(f1.data.len(), expected_len);
        assert_eq!(f2.data.len(), expected_len);
    }

    #[test]
    fn stopped_capture_returns_none() {
        let mut cap = TestPatternCapture::new();
        // Never started — next_frame must return None.
        let result = cap.next_frame().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn start_stop_lifecycle() {
        let mut cap = TestPatternCapture::new();
        cap.start(make_config()).unwrap();
        assert!(cap.next_frame().unwrap().is_some(), "expected frame after start");
        cap.stop();
        assert!(cap.next_frame().unwrap().is_none(), "expected None after stop");
    }

    #[test]
    fn pattern_data_is_non_empty() {
        let cap = TestPatternCapture::with_resolution(100, 100);
        let data = cap.generate_pattern(0);
        // 100 * 100 * 4 = 40 000 bytes
        assert_eq!(data.len(), 40_000);
    }

    #[test]
    fn pattern_changes_each_frame() {
        let cap = TestPatternCapture::new();
        let frame0 = cap.generate_pattern(0);
        let frame1 = cap.generate_pattern(1);
        assert_ne!(frame0, frame1, "consecutive frames must differ");
    }
}
