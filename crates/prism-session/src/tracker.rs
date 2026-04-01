// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::atomic::{AtomicU64, Ordering};

/// Per-channel bandwidth counters. Uses the low 8 bits of channel_id as the
/// array index, giving 256 buckets with O(1) lock-free updates.
pub struct ChannelBandwidthTracker {
    send_counters: [AtomicU64; 256],
    recv_counters: [AtomicU64; 256],
}

impl ChannelBandwidthTracker {
    pub fn new() -> Self {
        Self {
            send_counters: std::array::from_fn(|_| AtomicU64::new(0)),
            recv_counters: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    #[inline(always)]
    pub fn record_send(&self, channel_id: u16, bytes: u32) {
        self.send_counters[(channel_id & 0xFF) as usize].fetch_add(bytes as u64, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn record_recv(&self, channel_id: u16, bytes: u32) {
        self.recv_counters[(channel_id & 0xFF) as usize].fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn send_bytes(&self, channel_id: u16) -> u64 {
        self.send_counters[(channel_id & 0xFF) as usize].load(Ordering::Relaxed)
    }

    pub fn recv_bytes(&self, channel_id: u16) -> u64 {
        self.recv_counters[(channel_id & 0xFF) as usize].load(Ordering::Relaxed)
    }

    /// Reset all counters to zero (e.g., at the start of a measurement window).
    pub fn reset(&self) {
        for c in &self.send_counters {
            c.store(0, Ordering::Relaxed);
        }
        for c in &self.recv_counters {
            c.store(0, Ordering::Relaxed);
        }
    }
}

impl Default for ChannelBandwidthTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_send_accumulates_per_channel() {
        let t = ChannelBandwidthTracker::new();
        t.record_send(0x001, 100);
        t.record_send(0x001, 200);
        t.record_send(0x002, 50);
        assert_eq!(t.send_bytes(0x001), 300);
        assert_eq!(t.send_bytes(0x002), 50);
        // unrelated channel is still zero
        assert_eq!(t.send_bytes(0x003), 0);
    }

    #[test]
    fn record_recv_accumulates_per_channel() {
        let t = ChannelBandwidthTracker::new();
        t.record_recv(0x003, 512);
        t.record_recv(0x003, 512);
        assert_eq!(t.recv_bytes(0x003), 1024);
        assert_eq!(t.recv_bytes(0x001), 0);
    }

    #[test]
    fn reset_clears_all_counters() {
        let t = ChannelBandwidthTracker::new();
        t.record_send(0x001, 999);
        t.record_recv(0x002, 888);
        t.reset();
        assert_eq!(t.send_bytes(0x001), 0);
        assert_eq!(t.recv_bytes(0x002), 0);
    }

    #[test]
    fn channel_index_uses_low_byte() {
        let t = ChannelBandwidthTracker::new();
        // 0x101 and 0x001 both map to index 1 (low byte = 0x01)
        t.record_send(0x001, 100);
        t.record_send(0x101, 50);
        assert_eq!(t.send_bytes(0x001), 150);
        assert_eq!(t.send_bytes(0x101), 150);
    }

    #[test]
    fn default_gives_zeroed_tracker() {
        let t = ChannelBandwidthTracker::default();
        assert_eq!(t.send_bytes(0x000), 0);
        assert_eq!(t.recv_bytes(0x0FF), 0);
    }
}
