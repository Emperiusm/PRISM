// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// QUIC endpoint configuration.

use std::sync::Arc;
use std::time::Duration;

pub fn latency_transport_config(initial_rtt: Option<Duration>) -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();
    config.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
    config.datagram_receive_buffer_size(Some(65_536));
    config.max_idle_timeout(Some(Duration::from_secs(10).try_into().unwrap()));
    config.keep_alive_interval(Some(Duration::from_secs(5)));
    // Connection-level flow control
    config.receive_window(quinn::VarInt::from_u32(4_194_304));
    config.send_window(4_194_304);
    // Per-stream receive window
    config.stream_receive_window(quinn::VarInt::from_u32(1_048_576));
    // Concurrent stream limits
    config.max_concurrent_bidi_streams(quinn::VarInt::from_u32(16));
    config.max_concurrent_uni_streams(quinn::VarInt::from_u32(16));
    config.mtu_discovery_config(Some(quinn::MtuDiscoveryConfig::default()));
    if let Some(rtt) = initial_rtt {
        config.initial_rtt(rtt);
    }
    config
}

pub fn throughput_transport_config() -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();
    config.congestion_controller_factory(Arc::new(quinn::congestion::CubicConfig::default()));
    config.datagram_receive_buffer_size(None);
    config.max_idle_timeout(Some(Duration::from_secs(300).try_into().unwrap()));
    config.keep_alive_interval(Some(Duration::from_secs(30)));
    // Connection-level flow control
    config.receive_window(quinn::VarInt::from_u32(33_554_431)); // max u30 VarInt
    config.send_window(33_554_432);
    // Per-stream receive window
    config.stream_receive_window(quinn::VarInt::from_u32(8_388_607));
    // Concurrent stream limits
    config.max_concurrent_bidi_streams(quinn::VarInt::from_u32(64));
    config.max_concurrent_uni_streams(quinn::VarInt::from_u32(64));
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_config_creates_successfully() {
        let c = latency_transport_config(None);
        drop(c);
    }

    #[test]
    fn latency_config_with_initial_rtt() {
        let c = latency_transport_config(Some(Duration::from_millis(50)));
        drop(c);
    }

    #[test]
    fn throughput_config_creates_successfully() {
        let c = throughput_transport_config();
        drop(c);
    }
}
