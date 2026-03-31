use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

// ── InputTriggerCoalescer ─────────────────────────────────────────────────────

/// Coalesces rapid input events into a minimum-interval trigger stream.
///
/// Thread-safe: `trigger` and `has_pending` use atomic operations so they may
/// be called from any thread.  The `last_trigger` field stores a microsecond
/// timestamp so only one `u64` needs atomic access.
pub struct InputTriggerCoalescer {
    /// Minimum duration between two consecutive fired triggers.
    pub min_interval: Duration,
    /// Microsecond timestamp of the last fired trigger (0 = never).
    last_trigger: AtomicU64,
    /// True when an event arrived during a suppression window.
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

    /// Attempt to fire a trigger at `now_us` microseconds.
    ///
    /// Returns `true` (and resets the pending flag) when `now_us - last_trigger
    /// >= min_interval`.  Returns `false` and sets the pending flag otherwise.
    pub fn trigger(&self, now_us: u64) -> bool {
        let last = self.last_trigger.load(Ordering::Relaxed);
        let elapsed_us = now_us.saturating_sub(last);
        let threshold_us = self.min_interval.as_micros() as u64;

        if elapsed_us >= threshold_us {
            self.last_trigger.store(now_us, Ordering::Relaxed);
            self.pending.store(false, Ordering::Relaxed);
            true
        } else {
            self.pending.store(true, Ordering::Relaxed);
            false
        }
    }

    /// Returns `true` if an event was suppressed and is waiting for the next window.
    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed)
    }
}

// ── FramePacer ────────────────────────────────────────────────────────────────

/// Controls the capture rate, adapting to content frame-rate while respecting a hard cap.
pub struct FramePacer {
    /// Hard upper bound on capture rate.
    pub target_fps: u8,
    /// Observed content frame-rate (used for adaptive pacing).
    pub content_fps: f32,
    /// Current effective minimum inter-frame interval.
    min_interval: Duration,
    last_capture: Instant,
}

impl FramePacer {
    pub fn new(target_fps: u8) -> Self {
        let interval = Duration::from_secs_f64(1.0 / target_fps as f64);
        Self {
            target_fps,
            content_fps: target_fps as f32,
            min_interval: interval,
            last_capture: Instant::now() - interval, // allow immediate first capture
        }
    }

    /// Notify the pacer of the observed content frame-rate.
    ///
    /// The effective pacing rate is set to `content_fps * 1.2`, capped at
    /// `target_fps` so we never exceed the hard limit.
    pub fn set_content_fps(&mut self, fps: f32) {
        self.content_fps = fps;
        let adaptive = (fps * 1.2).min(self.target_fps as f32).max(1.0);
        self.min_interval = Duration::from_secs_f64(1.0 / adaptive as f64);
    }

    /// Returns `true` if enough time has elapsed since the last capture.
    ///
    /// When `true` is returned the internal timestamp is refreshed.
    pub fn should_capture(&mut self) -> bool {
        if self.last_capture.elapsed() >= self.min_interval {
            self.last_capture = Instant::now();
            true
        } else {
            false
        }
    }

    /// The current minimum inter-frame interval.
    pub fn current_interval(&self) -> Duration {
        self.min_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── InputTriggerCoalescer ─────────────────────────────────────────────────

    #[test]
    fn input_trigger_debounce() {
        let coalescer = InputTriggerCoalescer::new(Duration::from_millis(50));
        let base_us = 1_000_000u64; // 1 second in µs

        // First trigger: no previous event → fires.
        assert!(coalescer.trigger(base_us));
        assert!(!coalescer.has_pending());

        // 10 ms later — within the 50 ms window, should not fire.
        assert!(!coalescer.trigger(base_us + 10_000));
        assert!(coalescer.has_pending());

        // 60 ms after base — window has passed, fires and clears pending.
        assert!(coalescer.trigger(base_us + 60_000));
        assert!(!coalescer.has_pending());
    }

    // ── FramePacer ────────────────────────────────────────────────────────────

    #[test]
    fn frame_pacer_respects_interval() {
        let mut pacer = FramePacer::new(60);
        // First call should fire immediately (initialised in the past).
        assert!(pacer.should_capture());
        // Immediately again — interval not elapsed.
        assert!(!pacer.should_capture());
    }

    #[test]
    fn frame_pacer_adapts_to_content() {
        let mut pacer = FramePacer::new(120);
        // Content running at 30 fps → adaptive target = 36 fps.
        pacer.set_content_fps(30.0);
        let expected_interval = Duration::from_secs_f64(1.0 / 36.0);
        let actual = pacer.current_interval();
        // Allow 1 ms of floating-point slop.
        let diff = if actual > expected_interval {
            actual - expected_interval
        } else {
            expected_interval - actual
        };
        assert!(diff < Duration::from_millis(1), "diff={:?}", diff);
    }

    #[test]
    fn frame_pacer_caps_at_target() {
        let mut pacer = FramePacer::new(60);
        // Content at 200 fps: adaptive would be 240, but must cap at 60.
        pacer.set_content_fps(200.0);
        let expected = Duration::from_secs_f64(1.0 / 60.0);
        let actual = pacer.current_interval();
        let diff = if actual > expected {
            actual - expected
        } else {
            expected - actual
        };
        assert!(diff < Duration::from_millis(1), "diff={:?}", diff);
    }
}
