use prism_observability::time_series::MetricsTimeSeries;

/// Collects periodic metric samples for sparkline display.
pub struct MetricsCollector {
    series: MetricsTimeSeries,
    sample_count: u64,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            series: MetricsTimeSeries::new(300), // 5 min history
            sample_count: 0,
        }
    }

    /// Record a snapshot of current metrics.
    pub fn record_snapshot(&mut self, fps: f64, rtt_us: f64, bitrate_bps: f64) {
        let ts = self.sample_count;
        self.series.record("fps", ts, fps);
        self.series.record("rtt_us", ts, rtt_us);
        self.series.record("bitrate_bps", ts, bitrate_bps);
        self.sample_count += 1;
    }

    pub fn fps_history(&self) -> Vec<f64> {
        self.series
            .get("fps")
            .map(|ring| ring.samples().iter().map(|s| s.value).collect())
            .unwrap_or_default()
    }

    pub fn rtt_history(&self) -> Vec<f64> {
        self.series
            .get("rtt_us")
            .map(|ring| ring.samples().iter().map(|s| s.value).collect())
            .unwrap_or_default()
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_history() {
        let mc = MetricsCollector::new();
        assert!(mc.fps_history().is_empty());
        assert!(mc.rtt_history().is_empty());
        assert_eq!(mc.sample_count(), 0);
    }

    #[test]
    fn record_and_read() {
        let mut mc = MetricsCollector::new();
        mc.record_snapshot(60.0, 5_000.0, 10_000_000.0);
        mc.record_snapshot(59.5, 6_000.0, 9_500_000.0);

        let fps = mc.fps_history();
        assert_eq!(fps.len(), 2);
        assert!((fps[0] - 60.0).abs() < f64::EPSILON);
        assert!((fps[1] - 59.5).abs() < f64::EPSILON);

        let rtt = mc.rtt_history();
        assert_eq!(rtt.len(), 2);
        assert!((rtt[0] - 5_000.0).abs() < f64::EPSILON);

        assert_eq!(mc.sample_count(), 2);
    }

    #[test]
    fn ring_buffer_capacity() {
        let mut mc = MetricsCollector::new(); // capacity = 300

        // Record 400 samples — only the latest 300 should be retained.
        for i in 0..400u64 {
            mc.record_snapshot(i as f64, 0.0, 0.0);
        }

        let fps = mc.fps_history();
        assert_eq!(fps.len(), 300, "ring should hold at most 300 samples");
        // The oldest retained value should be sample 100 (fps = 100.0).
        assert!(
            (fps[0] - 100.0).abs() < f64::EPSILON,
            "oldest retained fps should be 100.0, got {}",
            fps[0]
        );
        // The newest should be 399.0.
        assert!((fps[299] - 399.0).abs() < f64::EPSILON);

        assert_eq!(mc.sample_count(), 400);
    }
}
