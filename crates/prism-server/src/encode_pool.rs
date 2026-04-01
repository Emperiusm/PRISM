// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::atomic::{AtomicU32, Ordering};

/// Configuration for the encoder worker pool.
pub struct EncodePoolConfig {
    pub num_workers: usize,
    pub max_pending_jobs: usize,
}

impl Default for EncodePoolConfig {
    fn default() -> Self {
        Self {
            num_workers: 2,
            max_pending_jobs: 8,
        }
    }
}

/// Tracks encoder pool statistics.
#[derive(Debug, Default)]
pub struct EncodePoolStats {
    pub jobs_submitted: AtomicU32,
    pub jobs_completed: AtomicU32,
    pub jobs_dropped: AtomicU32,
}

impl EncodePoolStats {
    pub fn submit(&self) {
        self.jobs_submitted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn complete(&self) {
        self.jobs_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn drop_job(&self) {
        self.jobs_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pending(&self) -> u32 {
        self.jobs_submitted
            .load(Ordering::Relaxed)
            .saturating_sub(self.jobs_completed.load(Ordering::Relaxed))
            .saturating_sub(self.jobs_dropped.load(Ordering::Relaxed))
    }

    pub fn completion_rate(&self) -> f32 {
        let submitted = self.jobs_submitted.load(Ordering::Relaxed);
        let completed = self.jobs_completed.load(Ordering::Relaxed);
        if submitted > 0 {
            completed as f32 / submitted as f32
        } else {
            1.0
        }
    }
}

/// Determines whether a new encode job should be accepted or dropped
/// based on pool pressure.
pub fn should_accept_job(stats: &EncodePoolStats, config: &EncodePoolConfig) -> bool {
    (stats.pending() as usize) < config.max_pending_jobs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_default_zero() {
        let stats = EncodePoolStats::default();
        assert_eq!(stats.jobs_submitted.load(Ordering::Relaxed), 0);
        assert_eq!(stats.jobs_completed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.jobs_dropped.load(Ordering::Relaxed), 0);
        assert_eq!(stats.pending(), 0);
    }

    #[test]
    fn stats_submit_complete() {
        let stats = EncodePoolStats::default();
        stats.submit();
        stats.submit();
        stats.submit();
        stats.complete();
        stats.complete();
        assert_eq!(stats.pending(), 1);
    }

    #[test]
    fn stats_completion_rate() {
        let stats = EncodePoolStats::default();
        for _ in 0..10 {
            stats.submit();
        }
        for _ in 0..8 {
            stats.complete();
        }
        let rate = stats.completion_rate();
        assert!(
            (rate - 0.8).abs() < f32::EPSILON,
            "expected 0.8, got {rate}"
        );
    }

    #[test]
    fn should_accept_when_not_full() {
        let stats = EncodePoolStats::default();
        let config = EncodePoolConfig::default(); // max_pending_jobs = 8
        // pending = 0 < 8 → accept
        assert!(should_accept_job(&stats, &config));
    }

    #[test]
    fn should_reject_when_full() {
        let stats = EncodePoolStats::default();
        let config = EncodePoolConfig::default(); // max_pending_jobs = 8
        // Submit 8 jobs with none completed → pending = 8
        for _ in 0..8 {
            stats.submit();
        }
        assert!(!should_accept_job(&stats, &config));
    }
}
