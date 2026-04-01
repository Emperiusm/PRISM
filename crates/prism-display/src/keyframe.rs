// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use crate::encode_config::KeyframeInterval;

/// Decides when to force an IDR / keyframe based on a configurable interval
/// policy, packet-loss feedback, and scene-change detection.
pub struct KeyframeDecider {
    interval: KeyframeInterval,
    /// Number of frames encoded since the last IDR was emitted.
    frames_since_idr: u32,
}

impl KeyframeDecider {
    pub fn new(interval: KeyframeInterval) -> Self {
        Self { interval, frames_since_idr: 0 }
    }

    /// Reset the frame counter (call after emitting an IDR).
    pub fn reset(&mut self) {
        self.frames_since_idr = 0;
    }

    /// Determine whether the current frame should be a forced IDR.
    ///
    /// * `loss_rate` — recent packet-loss ratio in `[0.0, 1.0]`.
    /// * `is_scene_change` — true when the upstream detector flagged a scene
    ///   change (hard cut, major content change).
    ///
    /// A scene change always triggers an IDR regardless of policy.
    /// The method advances `frames_since_idr` and resets it when it fires.
    pub fn should_force_idr(&mut self, loss_rate: f32, is_scene_change: bool) -> bool {
        self.frames_since_idr += 1;

        if is_scene_change {
            self.reset();
            return true;
        }

        let threshold = match self.interval {
            KeyframeInterval::Fixed(n) => n,
            KeyframeInterval::Adaptive { min_frames, max_frames } => {
                if loss_rate > 0.02 {
                    // High loss → use the short (minimum) interval.
                    min_frames
                } else if loss_rate == 0.0 {
                    // Clean link → use the long (maximum) interval.
                    max_frames
                } else {
                    // Middle ground: midpoint of the range.
                    (min_frames + max_frames) / 2
                }
            }
            KeyframeInterval::OnDemand => {
                // Never auto-trigger; only scene changes (handled above) fire.
                return false;
            }
        };

        if self.frames_since_idr >= threshold {
            self.reset();
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_interval_triggers_at_count() {
        let mut decider = KeyframeDecider::new(KeyframeInterval::Fixed(30));

        // First 29 frames should not trigger.
        for _ in 0..29 {
            assert!(!decider.should_force_idr(0.0, false));
        }
        // 30th frame triggers.
        assert!(decider.should_force_idr(0.0, false));
        // Counter resets; frame 31 should not trigger again immediately.
        assert!(!decider.should_force_idr(0.0, false));
    }

    #[test]
    fn scene_change_always_triggers() {
        let mut decider = KeyframeDecider::new(KeyframeInterval::Fixed(1000));
        // Even with a very long fixed interval, a scene change fires right away.
        assert!(decider.should_force_idr(0.0, true));
        // After reset the counter is 0; next call is frame 1 — should not fire.
        assert!(!decider.should_force_idr(0.0, false));
    }

    #[test]
    fn adaptive_shortens_on_loss() {
        // High loss → IDR every min_frames=60.
        let mut decider =
            KeyframeDecider::new(KeyframeInterval::Adaptive { min_frames: 60, max_frames: 1800 });

        for _ in 0..59 {
            assert!(!decider.should_force_idr(0.05, false));
        }
        // Frame 60 should trigger.
        assert!(decider.should_force_idr(0.05, false));
    }

    #[test]
    fn adaptive_extends_on_clean() {
        // Zero loss → IDR every max_frames=1800.
        let mut decider =
            KeyframeDecider::new(KeyframeInterval::Adaptive { min_frames: 60, max_frames: 1800 });

        for _ in 0..1799 {
            assert!(!decider.should_force_idr(0.0, false));
        }
        // Frame 1800 should trigger.
        assert!(decider.should_force_idr(0.0, false));
    }

    #[test]
    fn on_demand_never_auto_triggers() {
        let mut decider = KeyframeDecider::new(KeyframeInterval::OnDemand);
        for _ in 0..10_000 {
            assert!(!decider.should_force_idr(0.5, false));
        }
    }
}
