// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::collections::{HashMap, VecDeque};

/// A single timestamped measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeSample {
    /// Wall-clock second at which the measurement was taken.
    pub timestamp_secs: u64,
    /// Measured value.
    pub value: f64,
}

/// A fixed-capacity ring buffer of [`TimeSample`] values.
///
/// When the buffer is full, `push` evicts the oldest sample before inserting
/// the new one so the buffer never exceeds `max_len` entries.
#[derive(Debug, Clone)]
pub struct TimeSeriesRing {
    samples: VecDeque<TimeSample>,
    max_len: usize,
}

impl TimeSeriesRing {
    /// Create a new ring buffer that retains at most `max_len` samples.
    ///
    /// # Panics
    ///
    /// Panics if `max_len` is zero.
    pub fn new(max_len: usize) -> Self {
        assert!(max_len > 0, "max_len must be > 0");
        Self {
            samples: VecDeque::with_capacity(max_len),
            max_len,
        }
    }

    /// Append a new sample, evicting the oldest one if the buffer is full.
    pub fn push(&mut self, sample: TimeSample) {
        if self.samples.len() == self.max_len {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// Return a reference to all retained samples, oldest first.
    pub fn samples(&self) -> &VecDeque<TimeSample> {
        &self.samples
    }

    /// Return the most-recently inserted sample, or `None` if the ring is empty.
    pub fn latest(&self) -> Option<&TimeSample> {
        self.samples.back()
    }

    /// Number of samples currently held.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns `true` if no samples have been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// A named collection of [`TimeSeriesRing`] buffers.
///
/// Each metric name maps to its own ring buffer so callers can record
/// independent time series (e.g. `"encode_us"`, `"rtt_us"`) without
/// coordinating buffer allocation themselves.
#[derive(Debug, Clone)]
pub struct MetricsTimeSeries {
    rings: HashMap<String, TimeSeriesRing>,
    max_samples: usize,
}

impl MetricsTimeSeries {
    /// Create a new store where every ring retains at most `max_samples` entries.
    pub fn new(max_samples: usize) -> Self {
        Self {
            rings: HashMap::new(),
            max_samples,
        }
    }

    /// Record a measurement.
    ///
    /// If no ring exists for `name` yet, one is created automatically.
    pub fn record(&mut self, name: &str, timestamp_secs: u64, value: f64) {
        let max = self.max_samples;
        self.rings
            .entry(name.to_owned())
            .or_insert_with(|| TimeSeriesRing::new(max))
            .push(TimeSample {
                timestamp_secs,
                value,
            });
    }

    /// Return the ring for `name`, or `None` if no data has been recorded for it.
    pub fn get(&self, name: &str) -> Option<&TimeSeriesRing> {
        self.rings.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ring() {
        let ring = TimeSeriesRing::new(10);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert!(ring.latest().is_none());
        assert!(ring.samples().is_empty());
    }

    #[test]
    fn record_samples_and_latest() {
        let mut ring = TimeSeriesRing::new(5);
        ring.push(TimeSample {
            timestamp_secs: 1,
            value: 1.0,
        });
        ring.push(TimeSample {
            timestamp_secs: 2,
            value: 2.0,
        });
        ring.push(TimeSample {
            timestamp_secs: 3,
            value: 3.0,
        });

        assert_eq!(ring.len(), 3);
        let latest = ring.latest().unwrap();
        assert_eq!(latest.timestamp_secs, 3);
        assert!((latest.value - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn eviction_at_capacity() {
        let mut ring = TimeSeriesRing::new(3);
        for i in 0..5u64 {
            ring.push(TimeSample {
                timestamp_secs: i,
                value: i as f64,
            });
        }

        // Only the last 3 samples should be retained.
        assert_eq!(ring.len(), 3);
        let samples: Vec<_> = ring.samples().iter().collect();
        assert_eq!(samples[0].timestamp_secs, 2);
        assert_eq!(samples[1].timestamp_secs, 3);
        assert_eq!(samples[2].timestamp_secs, 4);
    }

    #[test]
    fn multiple_metrics_independent_rings() {
        let mut ts = MetricsTimeSeries::new(10);
        ts.record("encode_us", 1, 2_000.0);
        ts.record("encode_us", 2, 2_100.0);
        ts.record("rtt_us", 1, 12_000.0);

        let enc = ts.get("encode_us").unwrap();
        assert_eq!(enc.len(), 2);

        let rtt = ts.get("rtt_us").unwrap();
        assert_eq!(rtt.len(), 1);

        assert!(ts.get("missing_metric").is_none());
    }

    #[test]
    fn sparkline_data_correct_order() {
        // Verify that samples() returns data oldest-first for sparkline rendering.
        let mut ring = TimeSeriesRing::new(5);
        let values = [10.0f64, 20.0, 30.0, 40.0, 50.0];
        for (i, &v) in values.iter().enumerate() {
            ring.push(TimeSample {
                timestamp_secs: i as u64,
                value: v,
            });
        }

        let collected: Vec<f64> = ring.samples().iter().map(|s| s.value).collect();
        assert_eq!(collected, values.to_vec());
    }

    #[test]
    fn metrics_time_series_eviction_respects_max_samples() {
        let mut ts = MetricsTimeSeries::new(4);
        for i in 0..8u64 {
            ts.record("fps", i, i as f64);
        }
        let ring = ts.get("fps").unwrap();
        assert_eq!(ring.len(), 4);
        // Oldest retained should be i=4.
        assert_eq!(ring.samples().front().unwrap().timestamp_secs, 4);
    }
}
