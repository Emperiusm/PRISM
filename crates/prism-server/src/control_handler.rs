// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use prism_protocol::{
    channel::CHANNEL_CONTROL,
    header::PrismHeader,
};
use prism_session::{
    control_msg::{HEARTBEAT, PROBE_REQUEST, PROBE_RESPONSE},
    dispatch::{ChannelError, ChannelHandler},
    types::ClientId,
};

/// Per-handler statistics for the control channel.
#[derive(Debug, Default)]
pub struct ControlStats {
    pub heartbeats_received: AtomicU32,
    pub probes_received: AtomicU32,
    pub unknown_messages: AtomicU32,
}

/// `ChannelHandler` implementation for [`CHANNEL_CONTROL`].
///
/// Routes incoming datagrams by `msg_type`:
/// - [`HEARTBEAT`] → increment `heartbeats_received`
/// - [`PROBE_REQUEST`] / [`PROBE_RESPONSE`] → increment `probes_received`
/// - anything else (or too-short datagrams) → increment `unknown_messages`
///   unless the datagram is too short to parse, in which case all counters
///   are left untouched.
pub struct ControlChannelHandler {
    stats: Arc<ControlStats>,
}

impl ControlChannelHandler {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(ControlStats::default()),
        }
    }

    /// Borrow the shared stats handle (e.g. for inspection in tests or metrics).
    pub fn stats(&self) -> Arc<ControlStats> {
        self.stats.clone()
    }
}

impl Default for ControlChannelHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChannelHandler for ControlChannelHandler {
    fn channel_id(&self) -> u16 {
        CHANNEL_CONTROL
    }

    async fn handle_datagram(&self, _from: ClientId, data: Bytes) -> Result<(), ChannelError> {
        // Silently discard datagrams that are too short to contain a header.
        let header = match PrismHeader::decode_from_slice(&data) {
            Ok(h) => h,
            Err(_) => return Ok(()),
        };

        match header.msg_type {
            HEARTBEAT => {
                self.stats.heartbeats_received.fetch_add(1, Ordering::Relaxed);
            }
            PROBE_REQUEST => {
                self.stats.probes_received.fetch_add(1, Ordering::Relaxed);
                // Echo handled on the client side: client receives PROBE_REQUEST
                // and is expected to reply with PROBE_RESPONSE.  The server
                // simply counts inbound probe requests here; full echo logic
                // requires a client-side ControlChannelHandler (not yet wired).
            }
            PROBE_RESPONSE => {
                self.stats.probes_received.fetch_add(1, Ordering::Relaxed);
                tracing::trace!("probe response received — RTT measurement pending");
            }
            _ => {
                self.stats.unknown_messages.fetch_add(1, Ordering::Relaxed);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use prism_protocol::{
        channel::CHANNEL_CONTROL,
        header::{PrismHeader, PROTOCOL_VERSION},
    };
    use prism_session::control_msg::{HEARTBEAT, PROBE_REQUEST};
    use uuid::Uuid;

    fn client() -> ClientId {
        Uuid::nil()
    }

    /// Encode a minimal PRISM header datagram (no payload) with the given msg_type.
    fn make_datagram(channel_id: u16, msg_type: u8) -> Bytes {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id,
            msg_type,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(16);
        header.encode(&mut buf);
        buf.freeze()
    }

    #[test]
    fn handler_channel_id_is_control() {
        let handler = ControlChannelHandler::new();
        assert_eq!(handler.channel_id(), CHANNEL_CONTROL);
    }

    #[tokio::test]
    async fn heartbeat_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let data = make_datagram(CHANNEL_CONTROL, HEARTBEAT);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.heartbeats_received.load(Ordering::Relaxed), 1);
        assert_eq!(stats.probes_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn probe_request_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let data = make_datagram(CHANNEL_CONTROL, PROBE_REQUEST);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.probes_received.load(Ordering::Relaxed), 1);
        assert_eq!(stats.heartbeats_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn unknown_msg_type_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        // 0xFF is not a known control message type.
        let data = make_datagram(CHANNEL_CONTROL, 0xFF);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 1);
        assert_eq!(stats.heartbeats_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.probes_received.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn short_datagram_ignored() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        // 3 bytes — too short for a PrismHeader (needs 16).
        let data = Bytes::from_static(&[0x00, 0x06, 0x01]);
        handler.handle_datagram(client(), data).await.unwrap();
        // No counter should be incremented.
        assert_eq!(stats.heartbeats_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.probes_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }
}
