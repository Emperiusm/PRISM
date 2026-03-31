// Trend analysis for transport quality metrics.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Stable,
    Degrading,
    SlowlyDegrading,
    Improving,
}

pub struct TrendDetector {
    short_ema: f64,
    long_ema: f64,
    short_alpha: f64, // 2/(5+1) ≈ 0.333
    long_alpha: f64,  // 2/(30+1) ≈ 0.0645
    slope_window: VecDeque<f64>,
    slope_window_max: usize,
    initialized: bool,
}

impl TrendDetector {
    pub fn new() -> Self {
        Self {
            short_ema: 0.0,
            long_ema: 0.0,
            short_alpha: 2.0 / (5.0 + 1.0),
            long_alpha: 2.0 / (30.0 + 1.0),
            slope_window: VecDeque::new(),
            slope_window_max: 60,
            initialized: false,
        }
    }

    pub fn record(&mut self, value: f64) {
        if !self.initialized {
            self.short_ema = value;
            self.long_ema = value;
            self.initialized = true;
        } else {
            self.short_ema = self.short_alpha * value + (1.0 - self.short_alpha) * self.short_ema;
            self.long_ema = self.long_alpha * value + (1.0 - self.long_alpha) * self.long_ema;
        }

        self.slope_window.push_back(value);
        if self.slope_window.len() > self.slope_window_max {
            self.slope_window.pop_front();
        }
    }

    pub fn trend(&self) -> Trend {
        if !self.initialized {
            return Trend::Stable;
        }

        // Guard against zero long EMA
        if self.long_ema == 0.0 {
            return Trend::Stable;
        }

        let ratio = self.short_ema / self.long_ema;

        if ratio >= 1.3 {
            return Trend::Degrading;
        }

        if ratio <= 0.7 {
            return Trend::Improving;
        }

        // Check for slow degradation via slope
        if self.slope_window.len() >= 10 && self.slope() > 0.1 {
            return Trend::SlowlyDegrading;
        }

        Trend::Stable
    }

    /// Linear regression slope over the slope window.
    pub fn slope(&self) -> f64 {
        let n = self.slope_window.len();
        if n < 2 {
            return 0.0;
        }

        let n_f = n as f64;
        let mut sum_x = 0.0_f64;
        let mut sum_y = 0.0_f64;
        let mut sum_xy = 0.0_f64;
        let mut sum_xx = 0.0_f64;

        for (i, &y) in self.slope_window.iter().enumerate() {
            let x = i as f64;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let denom = n_f * sum_xx - sum_x * sum_x;
        if denom == 0.0 {
            return 0.0;
        }

        (n_f * sum_xy - sum_x * sum_y) / denom
    }

    pub fn ema_values(&self) -> (f64, f64) {
        (self.short_ema, self.long_ema)
    }
}

impl Default for TrendDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_detector_is_stable() {
        let det = TrendDetector::new();
        assert_eq!(det.trend(), Trend::Stable);
    }

    #[test]
    fn stable_constant_input() {
        let mut det = TrendDetector::new();
        for _ in 0..60 {
            det.record(10.0);
        }
        assert_eq!(det.trend(), Trend::Stable);
    }

    #[test]
    fn degrading_sharp_increase() {
        let mut det = TrendDetector::new();
        for _ in 0..30 {
            det.record(10.0);
        }
        for _ in 0..10 {
            det.record(20.0);
        }
        assert_eq!(det.trend(), Trend::Degrading);
    }

    #[test]
    fn improving_sharp_decrease() {
        let mut det = TrendDetector::new();
        for _ in 0..30 {
            det.record(20.0);
        }
        for _ in 0..10 {
            det.record(5.0);
        }
        assert_eq!(det.trend(), Trend::Improving);
    }

    #[test]
    fn slowly_degrading_gradual_increase() {
        let mut det = TrendDetector::new();
        for i in 0..60 {
            det.record(10.0 + i as f64 * 0.5);
        }
        let trend = det.trend();
        assert!(
            trend == Trend::Degrading || trend == Trend::SlowlyDegrading,
            "got {:?}",
            trend
        );
    }

    #[test]
    fn short_and_long_ema_track_values() {
        let mut det = TrendDetector::new();
        for _ in 0..100 {
            det.record(42.0);
        }
        let (short, long) = det.ema_values();
        assert!((short - 42.0).abs() < 1.0);
        assert!((long - 42.0).abs() < 1.0);
    }

    #[test]
    fn slope_of_constant_is_near_zero() {
        let mut det = TrendDetector::new();
        for _ in 0..60 {
            det.record(10.0);
        }
        assert!(det.slope().abs() < 0.01);
    }
}
