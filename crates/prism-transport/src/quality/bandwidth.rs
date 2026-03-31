// Bandwidth estimation for transport quality.

use std::time::{Duration, Instant};
use std::collections::VecDeque;

/// Free function to avoid &self/&mut field borrow conflict
fn trim_samples(samples: &mut VecDeque<(Instant, u64)>, window: Duration, now: Instant) {
    while let Some((t, _)) = samples.front() {
        if now.duration_since(*t) > window && samples.len() > 1 {
            samples.pop_front();
        } else {
            break;
        }
    }
}

pub struct BandwidthEstimator {
    send_samples: VecDeque<(Instant, u64)>,
    recv_samples: VecDeque<(Instant, u64)>,
    window: Duration,
}

impl BandwidthEstimator {
    pub fn new() -> Self {
        Self {
            send_samples: VecDeque::new(),
            recv_samples: VecDeque::new(),
            window: Duration::from_secs(5),
        }
    }

    pub fn record_send(&mut self, cumulative_bytes: u64) {
        self.record_send_at(cumulative_bytes, Instant::now());
    }

    pub fn record_send_at(&mut self, cumulative_bytes: u64, at: Instant) {
        self.send_samples.push_back((at, cumulative_bytes));
        trim_samples(&mut self.send_samples, self.window, at);
    }

    pub fn record_recv(&mut self, cumulative_bytes: u64) {
        self.record_recv_at(cumulative_bytes, Instant::now());
    }

    pub fn record_recv_at(&mut self, cumulative_bytes: u64, at: Instant) {
        self.recv_samples.push_back((at, cumulative_bytes));
        trim_samples(&mut self.recv_samples, self.window, at);
    }

    pub fn send_bps(&self) -> u64 {
        Self::compute_bps(&self.send_samples)
    }

    pub fn recv_bps(&self) -> u64 {
        Self::compute_bps(&self.recv_samples)
    }

    fn compute_bps(samples: &VecDeque<(Instant, u64)>) -> u64 {
        if samples.len() < 2 {
            return 0;
        }
        let (ft, fb) = samples.front().unwrap();
        let (lt, lb) = samples.back().unwrap();
        let elapsed = lt.duration_since(*ft);
        if elapsed.is_zero() {
            return 0;
        }
        let bits = lb.saturating_sub(*fb) * 8;
        (bits as f64 / elapsed.as_secs_f64()) as u64
    }
}

impl Default for BandwidthEstimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_estimator_returns_zero() {
        let est = BandwidthEstimator::new();
        assert_eq!(est.send_bps(), 0);
        assert_eq!(est.recv_bps(), 0);
    }

    #[test]
    fn single_sample_returns_zero() {
        let mut est = BandwidthEstimator::new();
        est.record_send(1000);
        assert_eq!(est.send_bps(), 0);
    }

    #[test]
    fn two_samples_compute_bandwidth() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_send_at(0, now);
        est.record_send_at(125_000, now + Duration::from_secs(1));
        let bps = est.send_bps();
        assert_eq!(bps, 1_000_000); // 125KB * 8 / 1s
    }

    #[test]
    fn recv_bandwidth_tracked_separately() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_recv_at(0, now);
        est.record_recv_at(250_000, now + Duration::from_secs(1));
        assert_eq!(est.recv_bps(), 2_000_000);
        assert_eq!(est.send_bps(), 0);
    }

    #[test]
    fn old_samples_expire() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_send_at(0, now);
        est.record_send_at(125_000, now + Duration::from_secs(1));
        est.record_send_at(125_000, now + Duration::from_secs(10));
        est.record_send_at(250_000, now + Duration::from_secs(11));
        let bps = est.send_bps();
        assert_eq!(bps, 1_000_000);
    }

    #[test]
    fn multiple_samples_average() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_send_at(0, now);
        est.record_send_at(250_000, now + Duration::from_secs(1));
        est.record_send_at(500_000, now + Duration::from_secs(2));
        let bps = est.send_bps();
        assert_eq!(bps, 2_000_000);
    }
}
