// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use prism_display::window_event::WindowEvent;
use std::time::{Duration, Instant};

/// Tracks window events and triggers speculative IDR on scene changes.
pub struct SpeculativeIdrController {
    last_foreground_hwnd: u64,
    last_idr_time: Option<Instant>,
    cooldown: Duration,
    idrs_triggered: u32,
}

impl SpeculativeIdrController {
    pub fn new(cooldown: Duration) -> Self {
        Self {
            last_foreground_hwnd: 0,
            last_idr_time: None,
            cooldown,
            idrs_triggered: 0,
        }
    }

    /// Process a window event. Returns true if an IDR should be triggered.
    pub fn process_event(&mut self, event: &WindowEvent) -> bool {
        if !event.triggers_speculative_idr() {
            return false;
        }

        // Check cooldown
        if self.last_idr_time.is_some_and(|last| last.elapsed() < self.cooldown) {
            return false;
        }

        let hwnd = event.hwnd();
        if hwnd != self.last_foreground_hwnd {
            self.last_foreground_hwnd = hwnd;
            self.last_idr_time = Some(Instant::now());
            self.idrs_triggered += 1;
            true
        } else {
            false
        }
    }

    pub fn idrs_triggered(&self) -> u32 {
        self.idrs_triggered
    }
}

impl Default for SpeculativeIdrController {
    fn default() -> Self {
        Self::new(Duration::from_secs(1))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_change_triggers_idr() {
        let mut ctrl = SpeculativeIdrController::new(Duration::from_secs(1));
        let event = WindowEvent::ForegroundChanged { hwnd: 0x1000 };
        assert!(ctrl.process_event(&event));
        assert_eq!(ctrl.idrs_triggered(), 1);
    }

    #[test]
    fn same_window_no_idr() {
        let mut ctrl = SpeculativeIdrController::new(Duration::from_secs(1));
        // First event seeds last_foreground_hwnd
        let event = WindowEvent::ForegroundChanged { hwnd: 0x2000 };
        assert!(ctrl.process_event(&event));
        // Same hwnd again — no IDR
        assert!(!ctrl.process_event(&event));
        assert_eq!(ctrl.idrs_triggered(), 1);
    }

    #[test]
    fn cooldown_respected() {
        let mut ctrl = SpeculativeIdrController::new(Duration::from_secs(60));
        let ev1 = WindowEvent::ForegroundChanged { hwnd: 0x3000 };
        let ev2 = WindowEvent::ForegroundChanged { hwnd: 0x4000 };
        // First change triggers IDR
        assert!(ctrl.process_event(&ev1));
        // Second change within cooldown is blocked
        assert!(!ctrl.process_event(&ev2));
        assert_eq!(ctrl.idrs_triggered(), 1);
    }

    #[test]
    fn non_speculative_event_ignored() {
        let mut ctrl = SpeculativeIdrController::new(Duration::from_secs(1));
        let event = WindowEvent::MoveSizeEnd { hwnd: 0x5000 };
        assert!(!ctrl.process_event(&event));
        assert_eq!(ctrl.idrs_triggered(), 0);
    }
}
