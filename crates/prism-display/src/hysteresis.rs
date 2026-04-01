// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::time::{Duration, Instant};

use crate::degradation::DegradationLevel;

/// Prevents rapid oscillation between degradation levels by enforcing hold durations.
///
/// Timer model:
/// - The *first* call that requests a change starts the hold timer and returns `false`.
/// - Subsequent calls return `true` only after the hold duration has elapsed.
/// - If the target level changes direction, the timer resets.
#[derive(Debug)]
pub struct Hysteresis {
    /// Minimum time to wait before degrading (moving to a worse level, i.e. higher index).
    pub downgrade_hold: Duration,
    /// Minimum time to wait before improving (moving to a better level, i.e. lower index).
    pub upgrade_hold: Duration,
    /// When the current pending change was first observed.
    last_change: Option<Instant>,
    /// The level target that started the current timer.
    last_target: Option<usize>,
}

impl Hysteresis {
    pub fn new(downgrade_hold: Duration, upgrade_hold: Duration) -> Self {
        Self {
            downgrade_hold,
            upgrade_hold,
            last_change: None,
            last_target: None,
        }
    }

    /// Returns `true` when the hold period for the requested transition has elapsed.
    ///
    /// On the *first ever* call requesting a change, the timer is started and `false` is
    /// returned.  Once the relevant hold duration has elapsed the next call returns `true`
    /// and the timer is cleared (ready for the next event).
    pub fn should_change(&mut self, current_level: usize, target_level: usize) -> bool {
        if current_level == target_level {
            // No change needed; clear any pending timer.
            self.last_change = None;
            self.last_target = None;
            return false;
        }

        let is_downgrade = target_level > current_level;
        let hold = if is_downgrade { self.downgrade_hold } else { self.upgrade_hold };

        // If the target changed, restart the timer.
        if self.last_target != Some(target_level) {
            self.last_change = Some(Instant::now());
            self.last_target = Some(target_level);
            return false;
        }

        // Same target — check whether the hold has elapsed.
        match self.last_change {
            Some(start) if start.elapsed() >= hold => {
                self.last_change = None;
                self.last_target = None;
                true
            }
            _ => false,
        }
    }
}

/// Describes a completed level transition and the side-effects it requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelChange {
    pub old_level: u8,
    pub new_level: u8,
    pub resolution_changed: bool,
    /// True when the encoder must be torn down and restarted (e.g. resolution change).
    pub needs_encoder_reinit: bool,
    /// True when a key-frame (IDR) must be injected.
    pub needs_idr: bool,
}

impl LevelChange {
    /// Compute the side-effects of moving from `old_level` to `new_level`.
    pub fn compute(
        old_level: u8,
        new_level: u8,
        old_res: (u32, u32),
        new_res: (u32, u32),
    ) -> Self {
        let resolution_changed = old_res != new_res;
        Self {
            old_level,
            new_level,
            resolution_changed,
            needs_encoder_reinit: resolution_changed,
            needs_idr: old_level != new_level,
        }
    }
}

/// User-imposed constraints that override automatic degradation choices.
#[derive(Debug, Clone, Default)]
pub struct UserConstraints {
    /// Do not allow a resolution smaller than this.
    pub min_resolution: Option<(u32, u32)>,
    /// Lock the resolution to exactly this value.
    pub pin_resolution: Option<(u32, u32)>,
    /// Lock the frame-rate to exactly this value.
    pub pin_fps: Option<u8>,
    /// Do not allow a frame-rate lower than this.
    pub min_fps: Option<u8>,
}

impl UserConstraints {
    /// Returns `true` if `level` satisfies all active constraints.
    pub fn allows(&self, level: &DegradationLevel) -> bool {
        if let Some((min_w, min_h)) = self.min_resolution
            && (level.resolution.0 < min_w || level.resolution.1 < min_h)
        {
            return false;
        }
        if let Some(pin) = self.pin_resolution
            && level.resolution != pin
        {
            return false;
        }
        if let Some(pin_fps) = self.pin_fps
            && level.max_fps != pin_fps
        {
            return false;
        }
        if let Some(min_fps) = self.min_fps
            && level.max_fps < min_fps
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CodecId;

    fn make_level(res: (u32, u32), fps: u8) -> DegradationLevel {
        DegradationLevel {
            name: "test".into(),
            max_bitrate_mbps: 10,
            resolution: res,
            max_fps: fps,
            codec: CodecId::H264,
            region_detection: false,
            fec_ratio: 0.0,
        }
    }

    // ── Hysteresis ─────────────────────────────────────────────────────────────

    /// First call requesting a change must start the timer and return false.
    #[test]
    fn immediate_first_change_starts_timer() {
        let mut h = Hysteresis::new(Duration::from_millis(200), Duration::from_millis(500));
        // First request: timer starts, returns false.
        assert!(!h.should_change(0, 1));
        // Last target is now recorded.
        assert_eq!(h.last_target, Some(1));
        assert!(h.last_change.is_some());
    }

    /// After the upgrade hold elapses, should_change returns true.
    #[test]
    fn upgrade_hold() {
        let mut h = Hysteresis::new(
            Duration::from_millis(500),
            Duration::from_millis(1), // near-zero upgrade hold
        );
        // Start the timer (upgrade: target < current).
        assert!(!h.should_change(2, 0));
        // Wait for the hold to expire.
        std::thread::sleep(Duration::from_millis(10));
        assert!(h.should_change(2, 0));
    }

    /// After the downgrade hold elapses, should_change returns true.
    #[test]
    fn fast_downgrade() {
        let mut h = Hysteresis::new(
            Duration::from_millis(1), // near-zero downgrade hold
            Duration::from_millis(500),
        );
        assert!(!h.should_change(0, 2));
        std::thread::sleep(Duration::from_millis(10));
        assert!(h.should_change(0, 2));
    }

    // ── LevelChange ────────────────────────────────────────────────────────────

    #[test]
    fn level_change_detects_resolution() {
        let lc = LevelChange::compute(0, 1, (3840, 2160), (1920, 1080));
        assert!(lc.resolution_changed);
        assert!(lc.needs_encoder_reinit);
        assert!(lc.needs_idr);
    }

    #[test]
    fn level_change_bitrate_only() {
        // Same resolution, different level (bitrate only changed).
        let lc = LevelChange::compute(0, 1, (1920, 1080), (1920, 1080));
        assert!(!lc.resolution_changed);
        assert!(!lc.needs_encoder_reinit);
        assert!(lc.needs_idr); // level still changed
    }

    // ── UserConstraints ────────────────────────────────────────────────────────

    #[test]
    fn user_constraints_clamp() {
        let low_res = make_level((1280, 720), 30);
        let high_res = make_level((1920, 1080), 60);

        let constraints = UserConstraints {
            min_resolution: Some((1920, 1080)),
            ..Default::default()
        };

        assert!(!constraints.allows(&low_res), "720p should be rejected");
        assert!(constraints.allows(&high_res), "1080p should be allowed");
    }

    #[test]
    fn user_constraints_pin_fps() {
        let level_60 = make_level((1920, 1080), 60);
        let level_30 = make_level((1920, 1080), 30);

        let constraints = UserConstraints {
            pin_fps: Some(60),
            ..Default::default()
        };

        assert!(constraints.allows(&level_60));
        assert!(!constraints.allows(&level_30));
    }

    #[test]
    fn user_constraints_min_fps() {
        let level_15 = make_level((1280, 720), 15);
        let level_30 = make_level((1920, 1080), 30);

        let constraints = UserConstraints {
            min_fps: Some(30),
            ..Default::default()
        };

        assert!(!constraints.allows(&level_15));
        assert!(constraints.allows(&level_30));
    }
}
