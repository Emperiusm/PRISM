// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Unified transport abstraction over QUIC, WebSocket, and TCP.

use crate::connection::PrismConnection;
use prism_protocol::channel;

// ── ConnectionSlot ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionSlot {
    Latency,
    Throughput,
}

// ── ChannelRouting ────────────────────────────────────────────────────────────

pub struct ChannelRouting {
    /// Indexed by priority_category(channel_id) → 0..=4
    routes: [ConnectionSlot; 5],
}

impl ChannelRouting {
    pub fn slot_for_channel(&self, channel_id: u16) -> ConnectionSlot {
        let category = channel::priority_category(channel_id);
        self.routes[category]
    }
}

impl Default for ChannelRouting {
    fn default() -> Self {
        Self {
            routes: [
                ConnectionSlot::Throughput, // Background (0)
                ConnectionSlot::Throughput, // Low (1)
                ConnectionSlot::Latency,    // Normal (2)
                ConnectionSlot::Latency,    // High (3)
                ConnectionSlot::Latency,    // Critical (4)
            ],
        }
    }
}

// ── UnifiedConnection ─────────────────────────────────────────────────────────

pub struct UnifiedConnection {
    latency: Box<dyn PrismConnection>,
    throughput: Option<Box<dyn PrismConnection>>,
    routing: ChannelRouting,
}

impl UnifiedConnection {
    pub fn new(
        latency: Box<dyn PrismConnection>,
        throughput: Option<Box<dyn PrismConnection>>,
    ) -> Self {
        Self {
            latency,
            throughput,
            routing: ChannelRouting::default(),
        }
    }

    pub fn for_channel(&self, channel_id: u16) -> &dyn PrismConnection {
        match self.routing.slot_for_channel(channel_id) {
            ConnectionSlot::Latency => &*self.latency,
            ConnectionSlot::Throughput => self.throughput.as_deref().unwrap_or(&*self.latency),
        }
    }

    pub fn latency(&self) -> &dyn PrismConnection {
        &*self.latency
    }

    pub fn throughput(&self) -> &dyn PrismConnection {
        self.throughput.as_deref().unwrap_or(&*self.latency)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::TransportType;
    use crate::connection::mock::MockConnection;
    use prism_protocol::channel::{
        CHANNEL_AUDIO, CHANNEL_CLIPBOARD, CHANNEL_CONTROL, CHANNEL_DEVICE, CHANNEL_DISPLAY,
        CHANNEL_FILESHARE, CHANNEL_INPUT, CHANNEL_SENSOR,
    };

    #[test]
    fn default_routing_critical_to_latency() {
        let routing = ChannelRouting::default();
        assert_eq!(
            routing.slot_for_channel(CHANNEL_INPUT),
            ConnectionSlot::Latency
        );
    }

    #[test]
    fn default_routing_high_to_latency() {
        let routing = ChannelRouting::default();
        assert_eq!(
            routing.slot_for_channel(CHANNEL_DISPLAY),
            ConnectionSlot::Latency
        );
        assert_eq!(
            routing.slot_for_channel(CHANNEL_AUDIO),
            ConnectionSlot::Latency
        );
    }

    #[test]
    fn default_routing_normal_to_latency() {
        let routing = ChannelRouting::default();
        assert_eq!(
            routing.slot_for_channel(CHANNEL_CONTROL),
            ConnectionSlot::Latency
        );
        assert_eq!(
            routing.slot_for_channel(CHANNEL_CLIPBOARD),
            ConnectionSlot::Latency
        );
    }

    #[test]
    fn default_routing_low_to_throughput() {
        let routing = ChannelRouting::default();
        assert_eq!(
            routing.slot_for_channel(CHANNEL_FILESHARE),
            ConnectionSlot::Throughput
        );
        assert_eq!(
            routing.slot_for_channel(CHANNEL_DEVICE),
            ConnectionSlot::Throughput
        );
    }

    #[test]
    fn default_routing_background_to_throughput() {
        let routing = ChannelRouting::default();
        assert_eq!(
            routing.slot_for_channel(CHANNEL_SENSOR),
            ConnectionSlot::Throughput
        );
    }

    #[test]
    fn unified_for_channel_routes_display_to_latency() {
        let latency = MockConnection::new(1200);
        let throughput = MockConnection::new(1200);
        let unified = UnifiedConnection::new(Box::new(latency), Some(Box::new(throughput)));
        let conn = unified.for_channel(CHANNEL_DISPLAY);
        assert_eq!(conn.transport_type(), TransportType::Quic);
    }

    #[test]
    fn unified_single_connection_fallback() {
        let latency = MockConnection::new(1200);
        let unified = UnifiedConnection::new(Box::new(latency), None);
        let conn = unified.for_channel(CHANNEL_FILESHARE);
        assert_eq!(conn.transport_type(), TransportType::Quic);
    }

    #[test]
    fn unified_latency_accessor() {
        let latency = MockConnection::new(1200);
        let unified = UnifiedConnection::new(Box::new(latency), None);
        assert_eq!(unified.latency().max_datagram_size(), 1200);
    }

    #[test]
    fn unified_throughput_falls_back_to_latency() {
        let latency = MockConnection::new(1200);
        let unified = UnifiedConnection::new(Box::new(latency), None);
        assert_eq!(unified.throughput().max_datagram_size(), 1200);
    }
}
