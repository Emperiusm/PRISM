// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::atomic::{AtomicU64, Ordering};

pub struct RateCounter {
    total: AtomicU64,
    prev_total: AtomicU64,
    prev_timestamp_us: AtomicU64,
    cached_rate: AtomicU64,
}

impl RateCounter {
    pub fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            prev_total: AtomicU64::new(0),
            prev_timestamp_us: AtomicU64::new(0),
            cached_rate: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn inc(&self, n: u64) {
        self.total.fetch_add(n, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn rate(&self) -> u64 {
        self.cached_rate.load(Ordering::Relaxed)
    }

    pub fn compute_rate(&self, now_us: u64) {
        let total = self.total.load(Ordering::Relaxed);
        let prev = self.prev_total.swap(total, Ordering::Relaxed);
        let prev_ts = self.prev_timestamp_us.swap(now_us, Ordering::Relaxed);
        let elapsed_us = now_us.saturating_sub(prev_ts);
        if elapsed_us == 0 { return; }
        let delta = total.saturating_sub(prev);
        let rate = delta * 1_000_000 / elapsed_us;
        self.cached_rate.store(rate, Ordering::Relaxed);
    }
}

impl Default for RateCounter {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_increment() {
        let rc = RateCounter::new();
        rc.inc(1);
        rc.inc(1);
        rc.inc(1);
        assert_eq!(rc.total(), 3);
    }

    #[test]
    fn rate_starts_at_zero() {
        let rc = RateCounter::new();
        assert_eq!(rc.rate(), 0);
    }

    #[test]
    fn rate_computation() {
        let rc = RateCounter::new();
        rc.compute_rate(0);
        for _ in 0..100 { rc.inc(1); }
        rc.compute_rate(1_000_000);
        assert_eq!(rc.rate(), 100);
    }

    #[test]
    fn rate_updates_on_each_compute() {
        let rc = RateCounter::new();
        rc.compute_rate(0);
        for _ in 0..50 { rc.inc(1); }
        rc.compute_rate(1_000_000);
        assert_eq!(rc.rate(), 50);
        for _ in 0..200 { rc.inc(1); }
        rc.compute_rate(2_000_000);
        assert_eq!(rc.rate(), 200);
    }

    #[test]
    fn rate_with_zero_elapsed() {
        let rc = RateCounter::new();
        rc.compute_rate(1000);
        rc.inc(100);
        rc.compute_rate(1000);
        // Should not panic or divide by zero
    }

    #[test]
    fn rate_with_bulk_increment() {
        let rc = RateCounter::new();
        rc.compute_rate(0);
        rc.inc(5000);
        rc.compute_rate(1_000_000);
        assert_eq!(rc.rate(), 5000);
    }
}
