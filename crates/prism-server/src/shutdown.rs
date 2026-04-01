// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// ShutdownCoordinator: manages graceful server shutdown with countdown and notice broadcasting.

use std::time::{Duration, Instant};

use prism_session::control_msg::ShutdownNotice;

/// Lifecycle state of the shutdown coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownState {
    /// Server is operating normally.
    Running,
    /// Shutdown has been initiated; notices have been sent to clients.
    NoticesSent,
    /// The grace period has elapsed; the server may now exit.
    GracePeriodExpired,
}

/// Coordinates a graceful shutdown sequence: issue a `ShutdownNotice`, count
/// down a configurable grace period, then transition to `GracePeriodExpired`.
pub struct ShutdownCoordinator {
    state: ShutdownState,
    notice: Option<ShutdownNotice>,
    shutdown_initiated: Option<Instant>,
    grace_period: Duration,
}

impl ShutdownCoordinator {
    /// Create a new coordinator in the `Running` state.
    pub fn new(grace_period: Duration) -> Self {
        Self {
            state: ShutdownState::Running,
            notice: None,
            shutdown_initiated: None,
            grace_period,
        }
    }

    /// Begin shutdown: record a `ShutdownNotice` and transition to `NoticesSent`.
    ///
    /// Subsequent calls are no-ops if shutdown has already been initiated.
    pub fn initiate(&mut self, reason: String, will_restart: bool) {
        if self.state != ShutdownState::Running {
            return;
        }
        let seconds_remaining = self.grace_period.as_secs() as u32;
        self.notice = Some(ShutdownNotice {
            reason,
            seconds_remaining,
            will_restart,
        });
        self.shutdown_initiated = Some(Instant::now());
        self.state = ShutdownState::NoticesSent;
    }

    /// Return the current `ShutdownNotice`, or `None` if shutdown has not been
    /// initiated yet.
    pub fn notice(&self) -> Option<&ShutdownNotice> {
        self.notice.as_ref()
    }

    /// Advance the coordinator clock:
    /// - If the grace period has elapsed, transition to `GracePeriodExpired`.
    /// - Otherwise, update `seconds_remaining` in the notice.
    ///
    /// Returns the new `ShutdownState`.
    pub fn tick(&mut self) -> ShutdownState {
        if self.state == ShutdownState::GracePeriodExpired {
            return self.state;
        }

        let Some(initiated) = self.shutdown_initiated else {
            return self.state;
        };

        let elapsed = initiated.elapsed();

        if elapsed >= self.grace_period {
            self.state = ShutdownState::GracePeriodExpired;
            if let Some(notice) = &mut self.notice {
                notice.seconds_remaining = 0;
            }
        } else {
            let remaining = self.grace_period.saturating_sub(elapsed);
            if let Some(notice) = &mut self.notice {
                notice.seconds_remaining = remaining.as_secs() as u32;
            }
        }

        self.state
    }

    /// Current lifecycle state.
    pub fn state(&self) -> ShutdownState {
        self.state
    }

    /// `true` when shutdown has been initiated (either `NoticesSent` or
    /// `GracePeriodExpired`).
    pub fn is_shutting_down(&self) -> bool {
        self.state != ShutdownState::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn starts_running() {
        let coord = ShutdownCoordinator::new(Duration::from_secs(30));
        assert_eq!(coord.state(), ShutdownState::Running);
        assert!(!coord.is_shutting_down());
        assert!(coord.notice().is_none());
    }

    #[test]
    fn initiate_changes_state() {
        let mut coord = ShutdownCoordinator::new(Duration::from_secs(30));
        coord.initiate("maintenance".to_string(), true);

        assert_eq!(coord.state(), ShutdownState::NoticesSent);
        assert!(coord.is_shutting_down());

        let notice = coord.notice().expect("notice should be set");
        assert_eq!(notice.reason, "maintenance");
        assert!(notice.will_restart);
        assert_eq!(notice.seconds_remaining, 30);
    }

    #[test]
    fn grace_period_expires() {
        let mut coord = ShutdownCoordinator::new(Duration::from_millis(10));
        coord.initiate("fast shutdown".to_string(), false);

        thread::sleep(Duration::from_millis(15));

        let state = coord.tick();
        assert_eq!(state, ShutdownState::GracePeriodExpired);
        assert_eq!(coord.notice().unwrap().seconds_remaining, 0);
    }

    #[test]
    fn countdown_updates() {
        let mut coord = ShutdownCoordinator::new(Duration::from_secs(30));
        coord.initiate("update".to_string(), true);

        // tick immediately — should still have ≤30 seconds remaining.
        coord.tick();
        let remaining = coord.notice().unwrap().seconds_remaining;
        assert!(remaining <= 30, "remaining={remaining} should be <=30");
    }
}
