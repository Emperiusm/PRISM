// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Bandwidth arbiter wiring: reads ChannelBandwidthTracker data and feeds
//! BandwidthArbiter to keep per-channel allocations current.
//!
//! Call [`update_arbiter_from_tracker`] every ~100 ms to refresh allocations,
//! then read back per-client display budget via [`display_allocation_bps`].

use prism_protocol::channel::{CHANNEL_AUDIO, CHANNEL_DISPLAY, CHANNEL_INPUT, ChannelPriority};
use prism_session::{BandwidthArbiter, BandwidthNeeds, ChannelBandwidthTracker, ClientId};

/// Update arbiter allocations from current bandwidth usage.
///
/// Called periodically (every ~100 ms) to keep allocations current.  The
/// display channel's `BandwidthNeeds` uses fixed production values; a future
/// revision can derive them from the Display Engine's own estimates.
///
/// Note: `add_channel` on the arbiter replaces any existing entry for
/// `(client_id, channel_id)`, so repeated calls are idempotent.
pub fn update_arbiter_from_tracker(
    arbiter: &mut BandwidthArbiter,
    tracker: &ChannelBandwidthTracker,
    client_id: ClientId,
) {
    // Read cumulative byte counts; the caller is responsible for windowing.
    let _display_bytes = tracker.send_bytes(CHANNEL_DISPLAY);
    let _input_bytes = tracker.recv_bytes(CHANNEL_INPUT);
    let _audio_bytes = tracker.send_bytes(CHANNEL_AUDIO);

    // Register the display channel with fixed production bandwidth needs.
    // In production this would come from the Display Engine's BandwidthNeeds.
    let display_needs = BandwidthNeeds {
        min_bps: 500_000,
        ideal_bps: 5_000_000,
        max_bps: 20_000_000,
        urgency: 0.0,
    };

    arbiter.add_channel(
        client_id,
        CHANNEL_DISPLAY,
        ChannelPriority::High,
        display_needs,
    );

    arbiter.rebalance();
}

/// Read the display allocation for a client.
///
/// Returns `5_000_000` (5 Mbps) as a conservative default when the client
/// has not yet been registered with the arbiter.
pub fn display_allocation_bps(arbiter: &BandwidthArbiter, client_id: ClientId) -> u64 {
    arbiter
        .allocation(client_id, CHANNEL_DISPLAY)
        .unwrap_or(5_000_000)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn client() -> ClientId {
        Uuid::nil()
    }

    fn unknown_client() -> ClientId {
        Uuid::from_u128(0xDEAD_BEEF)
    }

    // 1. After update_arbiter_from_tracker, allocation exists for the client.
    #[test]
    fn update_registers_display_channel() {
        let mut arbiter = BandwidthArbiter::new(100_000_000);
        let tracker = ChannelBandwidthTracker::new();

        update_arbiter_from_tracker(&mut arbiter, &tracker, client());

        let alloc = arbiter.allocation(client(), CHANNEL_DISPLAY);
        assert!(
            alloc.is_some(),
            "display allocation should exist after update"
        );
    }

    // 2. After rebalance, display allocation is > 0.
    #[test]
    fn display_allocation_returns_value() {
        let mut arbiter = BandwidthArbiter::new(100_000_000);
        let tracker = ChannelBandwidthTracker::new();

        update_arbiter_from_tracker(&mut arbiter, &tracker, client());

        let bps = display_allocation_bps(&arbiter, client());
        assert!(bps > 0, "display allocation should be > 0 after rebalance");
    }

    // 3. With a 10 Mbps total budget, display gets <= 10 Mbps.
    #[test]
    fn allocation_respects_total_budget() {
        let mut arbiter = BandwidthArbiter::new(10_000_000);
        let tracker = ChannelBandwidthTracker::new();

        update_arbiter_from_tracker(&mut arbiter, &tracker, client());

        let bps = display_allocation_bps(&arbiter, client());
        assert!(
            bps <= 10_000_000,
            "display allocation {bps} bps exceeds 10 Mbps total budget"
        );
    }

    // 4. Unknown client falls back to the 5 Mbps default.
    #[test]
    fn missing_client_returns_default() {
        let arbiter = BandwidthArbiter::new(100_000_000);
        // No channels registered — unknown_client has no entry.
        let bps = display_allocation_bps(&arbiter, unknown_client());
        assert_eq!(
            bps, 5_000_000,
            "expected 5 Mbps default for unregistered client"
        );
    }
}
