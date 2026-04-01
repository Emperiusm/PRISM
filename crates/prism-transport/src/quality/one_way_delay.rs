// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// One-way delay measurement for transport quality.

use crate::connection::DelayAsymmetry;

/// Tracks one-way delay in a single direction using the minimum-offset baseline approach.
struct DirectionEstimator {
    min_offset_us: Option<i64>,
    current_delay_us: i64,
}

impl DirectionEstimator {
    fn new() -> Self {
        Self {
            min_offset_us: None,
            current_delay_us: 0,
        }
    }

    /// `local_us` and `remote_us` are microsecond timestamps from each side.
    /// The clocks need not be synchronized; we track the minimum offset.
    fn record(&mut self, local_us: i64, remote_us: i64) {
        let offset = local_us - remote_us;
        match self.min_offset_us {
            None => {
                // First sample — set baseline, delay starts at zero
                self.min_offset_us = Some(offset);
                self.current_delay_us = 0;
            }
            Some(min) => {
                if offset < min {
                    // New minimum — reset baseline
                    self.min_offset_us = Some(offset);
                    self.current_delay_us = 0;
                } else {
                    self.current_delay_us = offset - min;
                }
            }
        }
    }

    fn delay_us(&self) -> Option<i64> {
        self.min_offset_us.map(|_| self.current_delay_us)
    }
}

pub struct OneWayDelayEstimator {
    downstream: DirectionEstimator,
    upstream: DirectionEstimator,
}

/// Threshold below which both directions are considered symmetric (microseconds).
const SYMMETRIC_THRESHOLD_US: i64 = 100;

impl OneWayDelayEstimator {
    pub fn new() -> Self {
        Self {
            downstream: DirectionEstimator::new(),
            upstream: DirectionEstimator::new(),
        }
    }

    /// Record a downstream (server→client) packet.
    /// `local_recv_us`: local receive timestamp in microseconds.
    /// `remote_send_us`: sender's send timestamp in microseconds.
    pub fn record_downstream(&mut self, local_recv_us: i64, remote_send_us: i64) {
        self.downstream.record(local_recv_us, remote_send_us);
    }

    /// Record an upstream (client→server) packet.
    pub fn record_upstream(&mut self, local_recv_us: i64, remote_send_us: i64) {
        self.upstream.record(local_recv_us, remote_send_us);
    }

    pub fn downstream_delay_us(&self) -> Option<i64> {
        self.downstream.delay_us()
    }

    pub fn upstream_delay_us(&self) -> Option<i64> {
        self.upstream.delay_us()
    }

    pub fn asymmetry(&self) -> DelayAsymmetry {
        let (Some(ds), Some(us)) = (self.downstream.delay_us(), self.upstream.delay_us()) else {
            return DelayAsymmetry::Unknown;
        };

        if ds <= SYMMETRIC_THRESHOLD_US && us <= SYMMETRIC_THRESHOLD_US {
            return DelayAsymmetry::Symmetric;
        }

        // Avoid division by zero
        let min = ds.min(us);
        if min == 0 {
            // One direction is effectively zero — use a large ratio
            if ds > us {
                return DelayAsymmetry::DownstreamSlow { ratio: f32::MAX };
            } else {
                return DelayAsymmetry::UpstreamSlow { ratio: f32::MAX };
            }
        }

        let ratio = ds.max(us) as f32 / min as f32;
        if ratio >= 2.0 {
            if ds > us {
                DelayAsymmetry::DownstreamSlow { ratio }
            } else {
                DelayAsymmetry::UpstreamSlow { ratio }
            }
        } else {
            DelayAsymmetry::Symmetric
        }
    }
}

impl Default for OneWayDelayEstimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_estimator_returns_unknown() {
        let est = OneWayDelayEstimator::new();
        assert_eq!(est.asymmetry(), DelayAsymmetry::Unknown);
        assert_eq!(est.downstream_delay_us(), None);
        assert_eq!(est.upstream_delay_us(), None);
    }

    #[test]
    fn first_sample_sets_baseline() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000);
        assert_eq!(est.downstream_delay_us(), Some(0));
    }

    #[test]
    fn increasing_offset_shows_delay() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_250, 1_000_100);
        assert_eq!(est.downstream_delay_us(), Some(50));
    }

    #[test]
    fn lower_offset_resets_baseline() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_250, 1_000_100);
        est.record_downstream(1_000_050, 1_000_000);
        assert_eq!(est.downstream_delay_us(), Some(0));
    }

    #[test]
    fn symmetric_when_delays_similar() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_200, 1_000_100);
        est.record_upstream(2_000_100, 2_000_000);
        est.record_upstream(2_000_200, 2_000_100);
        assert_eq!(est.asymmetry(), DelayAsymmetry::Symmetric);
    }

    #[test]
    fn downstream_slow_detected() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_700, 1_000_100);
        est.record_upstream(2_000_100, 2_000_000);
        est.record_upstream(2_000_250, 2_000_100);
        match est.asymmetry() {
            DelayAsymmetry::DownstreamSlow { ratio } => assert!(ratio > 2.0),
            other => panic!("expected DownstreamSlow, got {:?}", other),
        }
    }

    #[test]
    fn upstream_slow_detected() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_150, 1_000_050);
        est.record_upstream(2_000_100, 2_000_000);
        est.record_upstream(2_000_700, 2_000_100);
        match est.asymmetry() {
            DelayAsymmetry::UpstreamSlow { ratio } => assert!(ratio > 2.0),
            other => panic!("expected UpstreamSlow, got {:?}", other),
        }
    }
}
