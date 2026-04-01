// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::atomic::{AtomicU64, Ordering};

const BUCKET_COUNT: usize = 25;

pub struct AtomicHistogram {
    buckets: [AtomicU64; BUCKET_COUNT],
    sum: AtomicU64,
    count: AtomicU64,
    min: AtomicU64,
    max: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct HistogramSnapshot {
    pub buckets: [u64; BUCKET_COUNT],
    pub sum_us: u64,
    pub count: u64,
    pub avg_us: u64,
    pub min_us: u64,
    pub max_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
}

impl AtomicHistogram {
    pub fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
            min: AtomicU64::new(u64::MAX),
            max: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn record(&self, value_us: u64) {
        let bucket = if value_us == 0 {
            0
        } else {
            (63 - value_us.leading_zeros() as usize).min(BUCKET_COUNT - 1)
        };
        self.buckets[bucket].fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value_us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.update_min(value_us);
        self.update_max(value_us);
    }

    fn update_min(&self, value: u64) {
        let mut current = self.min.load(Ordering::Relaxed);
        while value < current {
            match self.min.compare_exchange_weak(
                current, value, Ordering::Relaxed, Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    fn update_max(&self, value: u64) {
        let mut current = self.max.load(Ordering::Relaxed);
        while value > current {
            match self.max.compare_exchange_weak(
                current, value, Ordering::Relaxed, Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn snapshot(&self) -> HistogramSnapshot {
        let buckets: [u64; BUCKET_COUNT] =
            std::array::from_fn(|i| self.buckets[i].load(Ordering::Relaxed));
        let count = self.count.load(Ordering::Relaxed);
        let sum = self.sum.load(Ordering::Relaxed);
        let min = self.min.load(Ordering::Relaxed);
        let max = self.max.load(Ordering::Relaxed);

        HistogramSnapshot {
            buckets,
            sum_us: sum,
            count,
            avg_us: if count > 0 { sum / count } else { 0 },
            min_us: if min == u64::MAX { 0 } else { min },
            max_us: max,
            p50_us: Self::percentile(&buckets, count, 0.50),
            p95_us: Self::percentile(&buckets, count, 0.95),
            p99_us: Self::percentile(&buckets, count, 0.99),
        }
    }

    /// Reset all counters to zero. Used for windowed measurements.
    pub fn reset(&self) {
        for bucket in &self.buckets {
            bucket.store(0, Ordering::Relaxed);
        }
        self.sum.store(0, Ordering::Relaxed);
        self.count.store(0, Ordering::Relaxed);
        self.min.store(u64::MAX, Ordering::Relaxed);
        self.max.store(0, Ordering::Relaxed);
    }

    /// Snapshot and reset. Returns the snapshot, then zeros counters.
    pub fn snapshot_and_reset(&self) -> HistogramSnapshot {
        let snap = self.snapshot();
        self.reset();
        snap
    }

    fn percentile(buckets: &[u64; BUCKET_COUNT], total: u64, pct: f64) -> u64 {
        if total == 0 { return 0; }
        let target = (total as f64 * pct) as u64;
        let mut cumulative = 0u64;
        for (i, &count) in buckets.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                let bucket_start = if i == 0 { 0 } else { 1u64 << i };
                let bucket_end = 1u64 << (i + 1);
                let fraction = if count > 0 {
                    (target.saturating_sub(cumulative - count)) as f64 / count as f64
                } else { 0.0 };
                return bucket_start + ((bucket_end - bucket_start) as f64 * fraction) as u64;
            }
        }
        0
    }
}

impl Default for AtomicHistogram {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_histogram() {
        let h = AtomicHistogram::new();
        let snap = h.snapshot();
        assert_eq!(snap.count, 0);
        assert_eq!(snap.sum_us, 0);
        assert_eq!(snap.avg_us, 0);
        assert_eq!(snap.min_us, 0);
        assert_eq!(snap.max_us, 0);
        assert_eq!(snap.p50_us, 0);
    }

    #[test]
    fn single_value() {
        let h = AtomicHistogram::new();
        h.record(1000);
        let snap = h.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.sum_us, 1000);
        assert_eq!(snap.avg_us, 1000);
        assert_eq!(snap.min_us, 1000);
        assert_eq!(snap.max_us, 1000);
    }

    #[test]
    fn min_max_tracking() {
        let h = AtomicHistogram::new();
        h.record(100);
        h.record(500);
        h.record(50);
        h.record(1000);
        let snap = h.snapshot();
        assert_eq!(snap.min_us, 50);
        assert_eq!(snap.max_us, 1000);
        assert_eq!(snap.count, 4);
    }

    #[test]
    fn zero_value() {
        let h = AtomicHistogram::new();
        h.record(0);
        let snap = h.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.min_us, 0);
        assert_eq!(snap.max_us, 0);
    }

    #[test]
    fn large_value() {
        let h = AtomicHistogram::new();
        h.record(34_000_000);
        let snap = h.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.max_us, 34_000_000);
    }

    #[test]
    fn percentile_uniform_distribution() {
        let h = AtomicHistogram::new();
        for i in 1..=100 { h.record(i); }
        let snap = h.snapshot();
        assert_eq!(snap.count, 100);
        assert_eq!(snap.min_us, 1);
        assert_eq!(snap.max_us, 100);
        assert!(snap.p50_us >= 32 && snap.p50_us <= 80, "p50 was {}", snap.p50_us);
        assert!(snap.p95_us >= 64 && snap.p95_us <= 128, "p95 was {}", snap.p95_us);
    }

    #[test]
    fn percentile_all_same_value() {
        let h = AtomicHistogram::new();
        for _ in 0..1000 { h.record(500); }
        let snap = h.snapshot();
        assert!(snap.p50_us >= 256 && snap.p50_us <= 512, "p50 was {}", snap.p50_us);
    }

    #[test]
    fn reset_clears_all() {
        let h = AtomicHistogram::new();
        h.record(100);
        h.record(200);
        h.reset();
        let snap = h.snapshot();
        assert_eq!(snap.count, 0);
        assert_eq!(snap.sum_us, 0);
        assert_eq!(snap.min_us, 0);
        assert_eq!(snap.max_us, 0);
    }

    #[test]
    fn snapshot_and_reset_returns_data_then_clears() {
        let h = AtomicHistogram::new();
        h.record(100);
        h.record(200);
        let snap = h.snapshot_and_reset();
        assert_eq!(snap.count, 2);
        assert_eq!(snap.sum_us, 300);
        // After reset
        let snap2 = h.snapshot();
        assert_eq!(snap2.count, 0);
        assert_eq!(snap2.sum_us, 0);
    }

    #[test]
    fn concurrent_recording() {
        use std::sync::Arc;
        use std::thread;
        let h = Arc::new(AtomicHistogram::new());
        let mut handles = Vec::new();
        for t in 0..4 {
            let h = h.clone();
            handles.push(thread::spawn(move || {
                for i in 0..1000 { h.record((t * 1000 + i) as u64); }
            }));
        }
        for handle in handles { handle.join().unwrap(); }
        let snap = h.snapshot();
        assert_eq!(snap.count, 4000);
        assert_eq!(snap.min_us, 0);
        assert_eq!(snap.max_us, 3999);
    }
}
