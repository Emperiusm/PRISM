// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::net::SocketAddr;

use prism_transport::quic::config::throughput_transport_config;

/// Configuration for the throughput QUIC endpoint.
pub struct ThroughputEndpointConfig {
    pub addr: SocketAddr,
    pub enabled: bool,
}

impl ThroughputEndpointConfig {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr, enabled: true }
    }

    pub fn disabled() -> Self {
        Self {
            addr: "0.0.0.0:0".parse().unwrap(),
            enabled: false,
        }
    }
}

impl Default for ThroughputEndpointConfig {
    fn default() -> Self {
        Self::new("0.0.0.0:9877".parse().unwrap())
    }
}

/// Channels that should use the throughput connection.
pub fn is_throughput_channel(channel_id: u16) -> bool {
    use prism_protocol::channel::*;
    matches!(channel_id, CHANNEL_FILESHARE | CHANNEL_DEVICE)
}

/// Build a quinn TransportConfig optimized for throughput.
pub fn build_throughput_config() -> quinn::TransportConfig {
    throughput_transport_config()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_protocol::channel::{CHANNEL_DISPLAY, CHANNEL_FILESHARE};

    #[test]
    fn default_config_port() {
        let config = ThroughputEndpointConfig::default();
        assert_eq!(config.addr.port(), 9877);
        assert!(config.enabled);
    }

    #[test]
    fn fileshare_is_throughput() {
        assert!(is_throughput_channel(CHANNEL_FILESHARE));
    }

    #[test]
    fn display_is_not_throughput() {
        assert!(!is_throughput_channel(CHANNEL_DISPLAY));
    }
}
