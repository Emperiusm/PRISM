// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

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

// ─── BandwidthArbiter ────────────────────────────────────────────────────────

use std::collections::HashMap;
use crate::types::ClientId;
use prism_protocol::channel::ChannelPriority;

struct ChannelEntry {
    priority: ChannelPriority,
    needs: BandwidthNeeds,
    allocated_bps: u64,
}

/// Distributes a fixed total bandwidth budget across (client, channel) pairs
/// using a two-phase algorithm:
///   1. Satisfy `min_bps` guarantees for every channel.
///   2. Distribute remaining headroom proportionally to priority weight,
///      capped at each channel's `max_bps`.
pub struct BandwidthArbiter {
    channels: HashMap<(ClientId, u16), ChannelEntry>,
    total_bps: u64,
}

impl BandwidthArbiter {
    pub fn new(total_bps: u64) -> Self {
        Self {
            channels: HashMap::new(),
            total_bps,
        }
    }

    pub fn add_channel(
        &mut self,
        client_id: ClientId,
        channel_id: u16,
        priority: ChannelPriority,
        needs: BandwidthNeeds,
    ) {
        self.channels.insert(
            (client_id, channel_id),
            ChannelEntry { priority, needs, allocated_bps: 0 },
        );
    }

    pub fn remove_client(&mut self, client_id: ClientId) {
        self.channels.retain(|(cid, _), _| *cid != client_id);
    }

    pub fn allocation(&self, client_id: ClientId, channel_id: u16) -> Option<u64> {
        self.channels
            .get(&(client_id, channel_id))
            .map(|e| e.allocated_bps)
    }

    /// Recompute allocations for all registered channels.
    pub fn rebalance(&mut self) {
        if self.channels.is_empty() {
            return;
        }

        // Phase 1: guarantee minimums
        let total_min: u64 = self.channels.values().map(|e| e.needs.min_bps).sum();
        let remaining = self.total_bps.saturating_sub(total_min);
        for entry in self.channels.values_mut() {
            entry.allocated_bps = entry.needs.min_bps;
        }

        // Phase 2: distribute leftover bandwidth by priority weight
        let total_weight: u64 = self
            .channels
            .values()
            .map(|e| prism_protocol::channel::priority_weight(e.priority) as u64)
            .sum();

        if total_weight > 0 && remaining > 0 {
            let keys: Vec<_> = self.channels.keys().cloned().collect();
            for key in keys {
                if let Some(entry) = self.channels.get(&key) {
                    let weight =
                        prism_protocol::channel::priority_weight(entry.priority) as u64;
                    let headroom = entry.needs.max_bps.saturating_sub(entry.needs.min_bps);
                    let share = (remaining * weight / total_weight).min(headroom);
                    let new_alloc = entry.allocated_bps + share;
                    self.channels.get_mut(&key).unwrap().allocated_bps = new_alloc;
                }
            }
        }
    }
}

// ─── StarvationDetector ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StarvationWarning {
    pub channel_id: u16,
    pub allocated_bps: u64,
    pub actual_bps: u64,
    pub starved_ticks: u32,
}

/// Detects channels that are chronically using far less bandwidth than allocated,
/// which may indicate a stuck sender or allocation/need mismatch.
pub struct StarvationDetector {
    /// channel_id → consecutive ticks where actual < allocated/2
    channels: HashMap<u16, u32>,
    threshold_ticks: u32,
}

impl StarvationDetector {
    pub fn new(threshold_ticks: u32) -> Self {
        Self {
            channels: HashMap::new(),
            threshold_ticks,
        }
    }

    /// Call once per measurement tick for every active channel.
    pub fn update(&mut self, channel_id: u16, allocated_bps: u64, actual_bps: u64) {
        if actual_bps < allocated_bps / 2 && allocated_bps > 0 {
            *self.channels.entry(channel_id).or_insert(0) += 1;
        } else {
            self.channels.remove(&channel_id);
        }
    }

    /// Returns channels that have been starved for more than `threshold_ticks`.
    pub fn check(&self) -> Vec<StarvationWarning> {
        self.channels
            .iter()
            .filter(|&(_, &ticks)| ticks > self.threshold_ticks)
            .map(|(&channel_id, &starved_ticks)| StarvationWarning {
                channel_id,
                allocated_bps: 0,
                actual_bps: 0,
                starved_ticks,
            })
            .collect()
    }
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

    // ── BandwidthArbiter ────────────────────────────────────────────────────

    use uuid::Uuid;

    fn client() -> ClientId {
        Uuid::nil()
    }

    fn other_client() -> ClientId {
        Uuid::from_u128(1)
    }

    fn needs(min: u64, ideal: u64, max: u64) -> BandwidthNeeds {
        BandwidthNeeds { min_bps: min, ideal_bps: ideal, max_bps: max, urgency: 0.5 }
    }

    #[test]
    fn single_channel_gets_all_bandwidth() {
        let mut arb = BandwidthArbiter::new(10_000_000);
        arb.add_channel(
            client(),
            0x001,
            ChannelPriority::Normal,
            needs(0, 5_000_000, 10_000_000),
        );
        arb.rebalance();
        let alloc = arb.allocation(client(), 0x001).unwrap();
        assert!(alloc >= 5_000_000, "expected >= 5M, got {alloc}");
    }

    #[test]
    fn min_guarantees_satisfied() {
        let mut arb = BandwidthArbiter::new(300_000);
        arb.add_channel(client(), 0x001, ChannelPriority::Normal, needs(100_000, 200_000, 400_000));
        arb.add_channel(client(), 0x002, ChannelPriority::Normal, needs(100_000, 200_000, 400_000));
        arb.rebalance();
        assert!(arb.allocation(client(), 0x001).unwrap() >= 100_000);
        assert!(arb.allocation(client(), 0x002).unwrap() >= 100_000);
    }

    #[test]
    fn priority_weighting_higher_gets_more() {
        let mut arb = BandwidthArbiter::new(1_000_000);
        arb.add_channel(client(),       0x001, ChannelPriority::High, needs(0, 500_000, 1_000_000));
        arb.add_channel(other_client(), 0x002, ChannelPriority::Low,  needs(0, 500_000, 1_000_000));
        arb.rebalance();
        let high_alloc = arb.allocation(client(),       0x001).unwrap();
        let low_alloc  = arb.allocation(other_client(), 0x002).unwrap();
        assert!(high_alloc > low_alloc,
            "High priority ({high_alloc}) should exceed Low priority ({low_alloc})");
    }

    #[test]
    fn remove_client_frees_allocation() {
        let mut arb = BandwidthArbiter::new(1_000_000);
        arb.add_channel(client(),       0x001, ChannelPriority::Normal, needs(0, 500_000, 1_000_000));
        arb.add_channel(other_client(), 0x002, ChannelPriority::Normal, needs(0, 500_000, 1_000_000));
        arb.rebalance();
        arb.remove_client(client());
        assert!(arb.allocation(client(), 0x001).is_none());
        assert!(arb.allocation(other_client(), 0x002).is_some());
    }

    // ── StarvationDetector ──────────────────────────────────────────────────

    #[test]
    fn starvation_detected_after_threshold() {
        let mut det = StarvationDetector::new(5);
        for _ in 0..6 {
            det.update(0x001, 1_000_000, 0);
        }
        let warnings = det.check();
        assert!(!warnings.is_empty(), "expected starvation warning after 6 ticks");
        assert_eq!(warnings[0].channel_id, 0x001);
        assert!(warnings[0].starved_ticks > 5);
    }

    #[test]
    fn starvation_clears_when_usage_recovers() {
        let mut det = StarvationDetector::new(3);
        for _ in 0..5 {
            det.update(0x001, 1_000_000, 0);
        }
        assert!(!det.check().is_empty());
        det.update(0x001, 1_000_000, 600_000);
        assert!(det.check().is_empty(), "starvation should clear after recovery");
    }
}
