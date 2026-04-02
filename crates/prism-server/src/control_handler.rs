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
    header::{HEADER_SIZE, PrismHeader},
};
use prism_session::{
    control_msg::{
        HEARTBEAT, PROBE_REQUEST, PROBE_RESPONSE, PROFILE_SWITCH, ProfileSwitchPayload,
        QUALITY_UPDATE, QualityUpdatePayload,
    },
    dispatch::{ChannelError, ChannelHandler},
    types::ClientId,
};

/// Per-handler statistics for the control channel.
#[derive(Debug, Default)]
pub struct ControlStats {
    pub heartbeats_received: AtomicU32,
    pub probes_received: AtomicU32,
    pub profile_switches: AtomicU32,
    pub quality_updates: AtomicU32,
    pub unknown_messages: AtomicU32,
}

pub struct ControlChannelHandler {
    stats: Arc<ControlStats>,
}

impl ControlChannelHandler {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(ControlStats::default()),
        }
    }

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
        let header = match PrismHeader::decode_from_slice(&data) {
            Ok(h) => h,
            Err(_) => return Ok(()),
        };

        match header.msg_type {
            HEARTBEAT => {
                self.stats
                    .heartbeats_received
                    .fetch_add(1, Ordering::Relaxed);
            }
            PROBE_REQUEST => {
                self.stats.probes_received.fetch_add(1, Ordering::Relaxed);
            }
            PROBE_RESPONSE => {
                self.stats.probes_received.fetch_add(1, Ordering::Relaxed);
                tracing::trace!("probe response received");
            }
            PROFILE_SWITCH => {
                match serde_json::from_slice::<ProfileSwitchPayload>(&data[HEADER_SIZE..]) {
                    Ok(payload) => {
                        tracing::info!(profile = %payload.profile_name, "client requested profile switch");
                        self.stats.profile_switches.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        self.stats.unknown_messages.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            QUALITY_UPDATE => {
                match serde_json::from_slice::<QualityUpdatePayload>(&data[HEADER_SIZE..]) {
                    Ok(payload) => {
                        tracing::info!(?payload, "client requested quality update");
                        self.stats.quality_updates.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        self.stats.unknown_messages.fetch_add(1, Ordering::Relaxed);
                    }
                }
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
        header::{PROTOCOL_VERSION, PrismHeader},
    };
    use prism_session::control_msg::{
        HEARTBEAT, PROBE_REQUEST, PROFILE_SWITCH, ProfileSwitchPayload, QUALITY_UPDATE,
        QualityUpdatePayload,
    };
    use uuid::Uuid;

    fn client() -> ClientId {
        Uuid::nil()
    }

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

    fn make_datagram_with_payload<T: serde::Serialize>(
        channel_id: u16,
        msg_type: u8,
        payload: &T,
    ) -> Bytes {
        let payload_bytes = serde_json::to_vec(payload).expect("serialize");
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id,
            msg_type,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: payload_bytes.len() as u32,
        };
        let mut buf = BytesMut::with_capacity(16 + payload_bytes.len());
        header.encode(&mut buf);
        buf.extend_from_slice(&payload_bytes);
        buf.freeze()
    }

    #[tokio::test]
    async fn heartbeat_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let data = make_datagram(CHANNEL_CONTROL, HEARTBEAT);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.heartbeats_received.load(Ordering::Relaxed), 1);
        assert_eq!(stats.probes_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.profile_switches.load(Ordering::Relaxed), 0);
        assert_eq!(stats.quality_updates.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn probe_request_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let data = make_datagram(CHANNEL_CONTROL, PROBE_REQUEST);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.heartbeats_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.probes_received.load(Ordering::Relaxed), 1);
        assert_eq!(stats.profile_switches.load(Ordering::Relaxed), 0);
        assert_eq!(stats.quality_updates.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn profile_switch_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let payload = ProfileSwitchPayload {
            profile_name: "Gaming".to_string(),
            max_fps: 120,
            encoder_preset: "UltraLowLatency".to_string(),
            prefer_lossless_text: false,
            region_detection: false,
        };
        let data = make_datagram_with_payload(CHANNEL_CONTROL, PROFILE_SWITCH, &payload);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.profile_switches.load(Ordering::Relaxed), 1);
        assert_eq!(stats.quality_updates.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn quality_update_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let payload = QualityUpdatePayload {
            encoder_preset: Some("Balanced".to_string()),
            max_fps: Some(90),
            bitrate_bps: Some(25_000_000),
            lossless_text: Some(true),
            region_detection: Some(false),
        };
        let data = make_datagram_with_payload(CHANNEL_CONTROL, QUALITY_UPDATE, &payload);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.profile_switches.load(Ordering::Relaxed), 0);
        assert_eq!(stats.quality_updates.load(Ordering::Relaxed), 1);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn unknown_msg_type_counted() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let data = make_datagram(CHANNEL_CONTROL, 0xFF);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.profile_switches.load(Ordering::Relaxed), 0);
        assert_eq!(stats.quality_updates.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn short_datagram_ignored() {
        let handler = ControlChannelHandler::new();
        let stats = handler.stats();
        let data = Bytes::from_static(&[0x00, 0x06, 0x01]);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.heartbeats_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.probes_received.load(Ordering::Relaxed), 0);
        assert_eq!(stats.profile_switches.load(Ordering::Relaxed), 0);
        assert_eq!(stats.quality_updates.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_messages.load(Ordering::Relaxed), 0);
    }
}
