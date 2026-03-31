// MTU discovery for transport quality.

use std::time::{Duration, Instant};

const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(1);

pub struct MtuTracker {
    last_known: usize,
    check_interval: Duration,
    last_check: Instant,
}

impl MtuTracker {
    pub fn new(initial: usize) -> Self {
        Self {
            last_known: initial,
            check_interval: DEFAULT_CHECK_INTERVAL,
            last_check: Instant::now(),
        }
    }

    pub fn with_check_interval(initial: usize, interval: Duration) -> Self {
        Self {
            last_known: initial,
            check_interval: interval,
            last_check: Instant::now(),
        }
    }

    pub fn current_max(&self) -> usize {
        self.last_known
    }

    pub fn needs_recheck(&self) -> bool {
        self.last_check.elapsed() >= self.check_interval
    }

    pub fn update(&mut self, new_mtu: usize) {
        self.last_known = new_mtu;
        self.last_check = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_initial_value() {
        let t = MtuTracker::new(1200);
        assert_eq!(t.current_max(), 1200);
    }

    #[test]
    fn update_changes_value() {
        let mut t = MtuTracker::new(1200);
        t.update(1400);
        assert_eq!(t.current_max(), 1400);
    }

    #[test]
    fn needs_recheck_after_interval() {
        let mut t = MtuTracker::with_check_interval(1200, Duration::from_millis(10));
        assert!(!t.needs_recheck());
        std::thread::sleep(Duration::from_millis(15));
        assert!(t.needs_recheck());
        t.update(1200);
        assert!(!t.needs_recheck());
    }

    #[test]
    fn mtu_shrink_detected() {
        let mut t = MtuTracker::new(1400);
        t.update(1200);
        assert_eq!(t.current_max(), 1200);
    }
}
