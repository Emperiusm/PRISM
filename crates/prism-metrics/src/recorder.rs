// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use crate::histogram::AtomicHistogram;
use crate::snapshot::RecorderSnapshot;

pub struct MetricLabels<const C: usize, const G: usize, const H: usize> {
    pub counter_names: [&'static str; C],
    pub gauge_names: [&'static str; G],
    pub histogram_names: [&'static str; H],
}

pub struct MetricsRecorder<const C: usize, const G: usize, const H: usize> {
    counters: [AtomicU64; C],
    gauges: [AtomicI64; G],
    histograms: [AtomicHistogram; H],
    labels: MetricLabels<C, G, H>,
}

impl<const C: usize, const G: usize, const H: usize> MetricsRecorder<C, G, H> {
    pub fn new(labels: MetricLabels<C, G, H>) -> Self {
        Self {
            counters: std::array::from_fn(|_| AtomicU64::new(0)),
            gauges: std::array::from_fn(|_| AtomicI64::new(0)),
            histograms: std::array::from_fn(|_| AtomicHistogram::new()),
            labels,
        }
    }

    #[inline(always)]
    pub fn inc(&self, counter: usize, value: u64) {
        debug_assert!(counter < C, "counter index {counter} out of bounds (recorder has {C} counters)");
        self.counters[counter].fetch_add(value, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn counter(&self, counter: usize) -> u64 {
        debug_assert!(counter < C, "counter index {counter} out of bounds (recorder has {C} counters)");
        self.counters[counter].load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn set(&self, gauge: usize, value: i64) {
        debug_assert!(gauge < G, "gauge index {gauge} out of bounds (recorder has {G} gauges)");
        self.gauges[gauge].store(value, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn gauge(&self, gauge: usize) -> i64 {
        debug_assert!(gauge < G, "gauge index {gauge} out of bounds (recorder has {G} gauges)");
        self.gauges[gauge].load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn observe(&self, histogram: usize, value_us: u64) {
        debug_assert!(histogram < H, "histogram index {histogram} out of bounds (recorder has {H} histograms)");
        self.histograms[histogram].record(value_us);
    }

    pub fn snapshot(&self) -> RecorderSnapshot {
        RecorderSnapshot {
            counters: self.counters.iter().map(|c| c.load(Ordering::Relaxed)).collect(),
            gauges: self.gauges.iter().map(|g| g.load(Ordering::Relaxed)).collect(),
            histograms: self.histograms.iter().map(|h| h.snapshot()).collect(),
            counter_names: self.labels.counter_names.to_vec(),
            gauge_names: self.labels.gauge_names.to_vec(),
            histogram_names: self.labels.histogram_names.to_vec(),
        }
    }
}

pub trait Observable: Send + Sync {
    fn name(&self) -> &'static str;
    fn snapshot(&self) -> RecorderSnapshot;
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestRecorder = MetricsRecorder<3, 2, 1>;

    fn test_labels() -> MetricLabels<3, 2, 1> {
        MetricLabels {
            counter_names: ["events", "bytes_sent", "errors"],
            gauge_names: ["active_connections", "queue_depth"],
            histogram_names: ["latency_us"],
        }
    }

    #[test]
    fn counter_increment() {
        let rec = TestRecorder::new(test_labels());
        rec.inc(0, 1);
        rec.inc(0, 1);
        rec.inc(0, 1);
        assert_eq!(rec.counter(0), 3);
        assert_eq!(rec.counter(1), 0);
    }

    #[test]
    fn gauge_set() {
        let rec = TestRecorder::new(test_labels());
        rec.set(0, 42);
        assert_eq!(rec.gauge(0), 42);
        rec.set(0, -1);
        assert_eq!(rec.gauge(0), -1);
    }

    #[test]
    fn histogram_observe() {
        let rec = TestRecorder::new(test_labels());
        rec.observe(0, 100);
        rec.observe(0, 200);
        rec.observe(0, 300);
        let snap = rec.snapshot();
        assert_eq!(snap.histograms[0].count, 3);
        assert_eq!(snap.histograms[0].sum_us, 600);
    }

    #[test]
    fn snapshot_captures_all_metrics() {
        let rec = TestRecorder::new(test_labels());
        rec.inc(0, 10);
        rec.inc(1, 20);
        rec.inc(2, 30);
        rec.set(0, 5);
        rec.set(1, -3);
        rec.observe(0, 1000);
        let snap = rec.snapshot();
        assert_eq!(snap.counters, vec![10, 20, 30]);
        assert_eq!(snap.gauges, vec![5, -3]);
        assert_eq!(snap.histograms.len(), 1);
        assert_eq!(snap.histograms[0].count, 1);
        assert_eq!(snap.counter_names, vec!["events", "bytes_sent", "errors"]);
        assert_eq!(snap.gauge_names, vec!["active_connections", "queue_depth"]);
        assert_eq!(snap.histogram_names, vec!["latency_us"]);
    }

    #[test]
    fn concurrent_increments() {
        use std::sync::Arc;
        use std::thread;
        let rec = Arc::new(TestRecorder::new(test_labels()));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let rec = rec.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..10_000 { rec.inc(0, 1); }
            }));
        }
        for handle in handles { handle.join().unwrap(); }
        assert_eq!(rec.counter(0), 40_000);
    }

    #[test]
    fn zero_sized_recorder() {
        type EmptyRecorder = MetricsRecorder<0, 0, 0>;
        let labels = MetricLabels {
            counter_names: [],
            gauge_names: [],
            histogram_names: [],
        };
        let rec = EmptyRecorder::new(labels);
        let snap = rec.snapshot();
        assert!(snap.counters.is_empty());
        assert!(snap.gauges.is_empty());
        assert!(snap.histograms.is_empty());
    }
}
