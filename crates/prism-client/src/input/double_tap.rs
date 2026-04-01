// SPDX-License-Identifier: AGPL-3.0-or-later
//! Double-tap Left Ctrl detector for overlay toggle.

use std::time::{Duration, Instant};

enum State {
    Idle,
    FirstTapSeen { tap_time: Instant },
    Triggered,
}

pub struct DoubleTapDetector {
    state: State,
    window: Duration,
}

impl DoubleTapDetector {
    pub fn new(window: Duration) -> Self {
        Self {
            state: State::Idle,
            window,
        }
    }

    /// Returns true if this key_down completes a double-tap.
    pub fn key_down(&mut self, now: Instant) -> bool {
        match self.state {
            State::Idle => {
                self.state = State::FirstTapSeen { tap_time: now };
                false
            }
            State::FirstTapSeen { tap_time } => {
                if now.duration_since(tap_time) <= self.window {
                    self.state = State::Triggered;
                    true
                } else {
                    self.state = State::FirstTapSeen { tap_time: now };
                    false
                }
            }
            State::Triggered => false,
        }
    }

    /// No-op: we track press-to-press timing only.
    pub fn key_up(&mut self, _now: Instant) {}

    /// Any other key cancels detection.
    pub fn other_key_pressed(&mut self) {
        self.state = State::Idle;
    }

    pub fn is_triggered(&self) -> bool {
        matches!(self.state, State::Triggered)
    }

    /// Reset to Idle after the overlay has been toggled.
    pub fn consume(&mut self) {
        self.state = State::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> DoubleTapDetector {
        DoubleTapDetector::new(Duration::from_millis(300))
    }

    #[test]
    fn no_tap_no_trigger() {
        let d = detector();
        assert!(!d.is_triggered());
    }

    #[test]
    fn single_tap_no_trigger() {
        let mut d = detector();
        let t0 = Instant::now();
        d.key_down(t0);
        d.key_up(t0);
        assert!(!d.is_triggered());
    }

    #[test]
    fn double_tap_within_window_triggers() {
        let mut d = detector();
        let t0 = Instant::now();
        let result1 = d.key_down(t0);
        let result2 = d.key_down(t0 + Duration::from_millis(100));
        assert!(!result1);
        assert!(result2);
        assert!(d.is_triggered());
    }

    #[test]
    fn double_tap_outside_window_no_trigger() {
        let mut d = detector();
        let t0 = Instant::now();
        let result1 = d.key_down(t0);
        let result2 = d.key_down(t0 + Duration::from_millis(400));
        assert!(!result1);
        assert!(!result2);
        assert!(!d.is_triggered());
    }

    #[test]
    fn other_key_resets() {
        let mut d = detector();
        let t0 = Instant::now();
        d.key_down(t0);
        d.other_key_pressed();
        let result = d.key_down(t0 + Duration::from_millis(100));
        assert!(!result);
        assert!(!d.is_triggered());
    }

    #[test]
    fn consume_resets_state() {
        let mut d = detector();
        let t0 = Instant::now();
        // First double-tap
        d.key_down(t0);
        d.key_down(t0 + Duration::from_millis(100));
        assert!(d.is_triggered());
        // Consume
        d.consume();
        assert!(!d.is_triggered());
        // Second double-tap should work again
        let t1 = Instant::now();
        d.key_down(t1);
        let result = d.key_down(t1 + Duration::from_millis(100));
        assert!(result);
        assert!(d.is_triggered());
    }
}
