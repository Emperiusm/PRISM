use std::sync::atomic::{AtomicU64, Ordering};

/// Shared handle to a single channel's bandwidth allocation.
/// Written by the arbiter, read on the hot path by senders.
pub struct AllocationHandle {
    allocated_bps: AtomicU64,
    min_bps: AtomicU64,
    max_bps: AtomicU64,
}

impl AllocationHandle {
    pub fn new(allocated: u64, min: u64, max: u64) -> Self {
        Self {
            allocated_bps: AtomicU64::new(allocated),
            min_bps: AtomicU64::new(min),
            max_bps: AtomicU64::new(max),
        }
    }

    #[inline(always)]
    pub fn allocated_bps(&self) -> u64 {
        self.allocated_bps.load(Ordering::Relaxed)
    }

    pub fn min_bps(&self) -> u64 {
        self.min_bps.load(Ordering::Relaxed)
    }

    pub fn max_bps(&self) -> u64 {
        self.max_bps.load(Ordering::Relaxed)
    }

    pub fn set_allocated(&self, bps: u64) {
        self.allocated_bps.store(bps, Ordering::Relaxed);
    }
}

/// Bandwidth requirements declared by a channel / session.
#[derive(Debug, Clone, Copy, Default)]
pub struct BandwidthNeeds {
    /// Absolute floor — drop below this and the channel is effectively unusable.
    pub min_bps: u64,
    /// Desired operating point for full-quality operation.
    pub ideal_bps: u64,
    /// Hard ceiling; the channel will never benefit from more than this.
    pub max_bps: u64,
    /// 0.0–1.0 urgency hint used as a tie-breaker during rebalance.
    pub urgency: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_handle_atomic_read_write() {
        let h = AllocationHandle::new(1_000, 500, 5_000);
        assert_eq!(h.allocated_bps(), 1_000);
        assert_eq!(h.min_bps(), 500);
        assert_eq!(h.max_bps(), 5_000);

        h.set_allocated(3_000);
        assert_eq!(h.allocated_bps(), 3_000);
        // min/max unchanged
        assert_eq!(h.min_bps(), 500);
        assert_eq!(h.max_bps(), 5_000);
    }

    #[test]
    fn bandwidth_needs_defaults_to_zero() {
        let n = BandwidthNeeds::default();
        assert_eq!(n.min_bps, 0);
        assert_eq!(n.ideal_bps, 0);
        assert_eq!(n.max_bps, 0);
        assert_eq!(n.urgency, 0.0);
    }
}
