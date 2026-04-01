// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// QualityMonitor: integrates transport quality estimators into the degradation ladder.

use prism_display::{DegradationLadder, DegradationLevel};
use prism_transport::quality::bandwidth::BandwidthEstimator;
use prism_transport::quality::one_way_delay::OneWayDelayEstimator;
use prism_transport::quality::prober::{ActivityState, ConnectionProber};
use prism_transport::quality::trend::{Trend, TrendDetector};
use prism_transport::{ConnectionQuality, TransportMetrics};

/// A snapshot of quality analysis produced by a single `update()` call.
#[derive(Debug, Clone)]
pub struct QualityUpdate {
    /// Full quality assessment from the transport layer.
    pub quality: ConnectionQuality,
    /// Target level index on the degradation ladder.
    pub target_level: usize,
    /// Parameters for the current (not necessarily target) level.
    pub current_level_params: Option<DegradationLevel>,
    /// Whether the target level differs from the level before this update.
    pub level_changed: bool,
    /// Current latency trend from the `TrendDetector`.
    pub trend: Trend,
}

/// Wires together transport-level quality estimators and a `DegradationLadder`.
pub struct QualityMonitor {
    prober: ConnectionProber,
    bandwidth: BandwidthEstimator,
    one_way_delay: OneWayDelayEstimator,
    trend: TrendDetector,
    ladder: DegradationLadder,
    current_level: usize,
}

impl QualityMonitor {
    /// Create a new monitor at degradation level 0 (highest quality).
    pub fn new(ladder: DegradationLadder) -> Self {
        Self {
            prober: ConnectionProber::new(),
            bandwidth: BandwidthEstimator::new(),
            one_way_delay: OneWayDelayEstimator::new(),
            trend: TrendDetector::new(),
            ladder,
            current_level: 0,
        }
    }

    /// Feed estimators with new transport metrics, compute quality, and map to
    /// a target degradation level.  Returns a `QualityUpdate` describing the
    /// result and whether the level changed.
    pub fn update(&mut self, metrics: &TransportMetrics) -> QualityUpdate {
        // Feed bandwidth estimator with cumulative byte counters from metrics.
        self.bandwidth.record_send(metrics.bytes_sent);
        self.bandwidth.record_recv(metrics.bytes_received);

        // Feed OWD estimator when timestamps are available.
        if let (Some(ds), Some(us)) = (metrics.downstream_delay_us, metrics.upstream_delay_us) {
            self.one_way_delay.record_downstream(ds, 0);
            self.one_way_delay.record_upstream(us, 0);
        }

        // Feed trend detector with RTT (in milliseconds as a float).
        let rtt_ms = metrics.rtt_us as f64 / 1_000.0;
        self.trend.record(rtt_ms);

        let asymmetry = self.one_way_delay.asymmetry();

        let quality = ConnectionQuality::compute(
            metrics.rtt_us,
            metrics.rtt_variance_us,
            metrics.loss_rate,
            self.bandwidth.send_bps(),
            self.bandwidth.recv_bps(),
            asymmetry,
        );

        let target_level = self.ladder.target_level(&quality.recommendation);
        let level_changed = target_level != self.current_level;
        let prev_level = self.current_level;
        self.current_level = target_level;

        let current_level_params = self.ladder.levels.get(prev_level).cloned();
        let trend = self.trend.trend();

        QualityUpdate {
            quality,
            target_level,
            current_level_params,
            level_changed,
            trend,
        }
    }

    /// Adjust the prober's polling frequency based on the activity state.
    pub fn set_activity(&mut self, state: ActivityState) {
        self.prober.set_activity(state);
    }

    /// Current degradation level index.
    pub fn current_level(&self) -> usize {
        self.current_level
    }

    /// Mutable access to the connection prober (e.g. to inject probe echoes).
    pub fn prober_mut(&mut self) -> &mut ConnectionProber {
        &mut self.prober
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn good_metrics() -> TransportMetrics {
        TransportMetrics {
            rtt_us: 2_000, // 2 ms
            rtt_variance_us: 200,
            loss_rate: 0.0,
            ..TransportMetrics::default()
        }
    }

    fn bad_metrics() -> TransportMetrics {
        TransportMetrics {
            rtt_us: 300_000, // 300 ms
            rtt_variance_us: 50_000,
            loss_rate: 0.15, // 15%
            ..TransportMetrics::default()
        }
    }

    #[test]
    fn good_quality_stays_at_level_0() {
        let mut monitor = QualityMonitor::new(DegradationLadder::gaming());
        let update = monitor.update(&good_metrics());
        assert_eq!(update.target_level, 0);
        assert!(
            !update.level_changed,
            "level should not change from initial 0"
        );
    }

    #[test]
    fn bad_quality_increases_level() {
        let mut monitor = QualityMonitor::new(DegradationLadder::gaming());
        let update = monitor.update(&bad_metrics());
        assert!(
            update.target_level > 0,
            "bad metrics should push level above 0"
        );
        assert!(update.level_changed, "level should have changed");
    }

    #[test]
    fn quality_update_includes_level_params() {
        let mut monitor = QualityMonitor::new(DegradationLadder::gaming());
        // Start at level 0 (optimal); first update with good metrics keeps us there.
        let update = monitor.update(&good_metrics());
        // current_level_params reflects level 0 before the update resolved.
        let params = update
            .current_level_params
            .expect("should have level params");
        assert_eq!(
            params.max_fps, 120,
            "gaming level 0 should have max_fps=120"
        );
    }

    #[test]
    fn activity_state_changes_probe_interval() {
        let mut monitor = QualityMonitor::new(DegradationLadder::gaming());

        monitor.set_activity(ActivityState::Idle);
        assert_eq!(
            monitor.prober_mut().probe_interval(),
            Duration::from_secs(60)
        );

        monitor.set_activity(ActivityState::ActiveStreaming);
        assert_eq!(
            monitor.prober_mut().probe_interval(),
            Duration::from_secs(2)
        );
    }
}
