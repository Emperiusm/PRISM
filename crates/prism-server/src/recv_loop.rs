// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// RecvLoop: per-client receive loop that reads datagrams/streams and dispatches them.
//
// The live QUIC receive loop requires a running connection and is deferred to a
// later task. This module contains the pure classification and bandwidth-tracking
// logic, which is fully testable without a real connection.

use std::sync::Arc;

use bytes::Bytes;
use prism_protocol::channel::CHANNEL_CONTROL;
use prism_protocol::header::{HEADER_SIZE, PrismHeader};
use prism_session::control_msg;
use prism_session::{ChannelBandwidthTracker, ChannelDispatcher, ClientId};
use prism_transport::{PrismConnection, TransportError};
use tokio::sync::mpsc;

/// The action the recv loop should take for a received datagram.
#[derive(Debug, PartialEq)]
pub enum DatagramAction {
    /// A probe-response datagram — used for RTT measurement.
    ProbeResponse,
    /// A channel data datagram — forward to the named channel handler.
    ChannelDispatch { channel_id: u16 },
    /// The datagram is malformed or too short — discard silently.
    Drop,
}

/// Classify an incoming raw datagram into the appropriate action.
///
/// Pure function; no I/O. Decodes the first `HEADER_SIZE` bytes and returns
/// the routing decision. Datagrams shorter than `HEADER_SIZE` or with an
/// invalid header are classified as `Drop`.
pub fn classify_datagram(data: &Bytes) -> DatagramAction {
    if data.len() < HEADER_SIZE {
        return DatagramAction::Drop;
    }
    match PrismHeader::decode_from_slice(data) {
        Ok(header) => {
            if header.channel_id == CHANNEL_CONTROL
                && header.msg_type == control_msg::PROBE_RESPONSE
            {
                DatagramAction::ProbeResponse
            } else {
                DatagramAction::ChannelDispatch {
                    channel_id: header.channel_id,
                }
            }
        }
        Err(_) => DatagramAction::Drop,
    }
}

/// Record the received bytes for this datagram in the bandwidth tracker.
///
/// Uses `header.payload_length` (the declared payload size) as the byte count.
/// Call this after a successful `classify_datagram` that did not return `Drop`.
#[inline(always)]
pub fn record_datagram_bandwidth(tracker: &ChannelBandwidthTracker, header: &PrismHeader) {
    tracker.record_recv(header.channel_id, header.payload_length);
}

// ── LiveRecvLoop ──────────────────────────────────────────────────────────────

/// A handle to a running per-client receive loop.
///
/// Call [`RecvLoopHandle::stop`] to cancel the loop gracefully.
pub struct RecvLoopHandle {
    cancel_tx: mpsc::Sender<()>,
}

impl RecvLoopHandle {
    /// Signal the recv loop to stop. Non-blocking; the loop will exit at the
    /// next iteration.
    pub async fn stop(&self) {
        let _ = self.cancel_tx.send(()).await;
    }
}

/// Spawn an async per-client datagram receive loop.
///
/// Reads datagrams from `connection`, classifies them, and either dispatches
/// them via `dispatcher` or records bandwidth via `tracker`. Sends an activity
/// ping on `activity_tx` for every valid datagram received.
///
/// Returns a [`RecvLoopHandle`] that can be used to stop the loop.
pub fn spawn_recv_loop(
    client_id: ClientId,
    connection: Arc<dyn PrismConnection>,
    dispatcher: Arc<ChannelDispatcher>,
    tracker: Arc<ChannelBandwidthTracker>,
    activity_tx: mpsc::Sender<ClientId>,
) -> RecvLoopHandle {
    let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = cancel_rx.recv() => {
                    break;
                }
                result = connection.recv_datagram() => {
                    match result {
                        Ok(data) => {
                            // Notify activity monitor.
                            let _ = activity_tx.try_send(client_id);

                            match classify_datagram(&data) {
                                DatagramAction::ChannelDispatch { channel_id } => {
                                    // Record bandwidth, then dispatch.
                                    if let Ok(header) = PrismHeader::decode_from_slice(&data) {
                                        record_datagram_bandwidth(&tracker, &header);
                                    }
                                    dispatcher.dispatch(client_id, channel_id, data).await;
                                }
                                DatagramAction::ProbeResponse => {
                                    // RTT probe — record bandwidth only.
                                    if let Ok(header) = PrismHeader::decode_from_slice(&data) {
                                        record_datagram_bandwidth(&tracker, &header);
                                    }
                                }
                                DatagramAction::Drop => {
                                    // Malformed — discard silently.
                                }
                            }
                        }
                        Err(TransportError::ConnectionClosed) => {
                            break;
                        }
                        Err(_) => {
                            // Transient error — keep looping.
                        }
                    }
                }
            }
        }
    });

    RecvLoopHandle { cancel_tx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    /// Build a minimal datagram containing only the encoded header.
    fn make_datagram(channel_id: u16, msg_type: u8, payload_len: u32) -> Bytes {
        let header = PrismHeader {
            version: 0,
            channel_id,
            msg_type,
            flags: 0,
            sequence: 1,
            timestamp_us: 0,
            payload_length: payload_len,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        buf.freeze()
    }

    #[test]
    fn classify_probe_response() {
        let data = make_datagram(CHANNEL_CONTROL, control_msg::PROBE_RESPONSE, 0);
        assert_eq!(classify_datagram(&data), DatagramAction::ProbeResponse);
    }

    #[test]
    fn classify_display_datagram() {
        // Channel 0x001, arbitrary msg_type 0x02 → ChannelDispatch
        let data = make_datagram(0x001, 0x02, 1000);
        assert_eq!(
            classify_datagram(&data),
            DatagramAction::ChannelDispatch { channel_id: 0x001 }
        );
    }

    #[test]
    fn classify_control_non_probe() {
        // CHANNEL_CONTROL + HEARTBEAT → ChannelDispatch (not a probe response)
        let data = make_datagram(CHANNEL_CONTROL, control_msg::HEARTBEAT, 0);
        assert_eq!(
            classify_datagram(&data),
            DatagramAction::ChannelDispatch {
                channel_id: CHANNEL_CONTROL
            }
        );
    }

    #[test]
    fn classify_too_short_drops() {
        // Only 4 bytes — far below the 16-byte header minimum.
        let data = Bytes::from_static(&[0x00, 0x10, 0x01, 0x00]);
        assert_eq!(classify_datagram(&data), DatagramAction::Drop);
    }

    #[test]
    fn bandwidth_tracking() {
        let tracker = ChannelBandwidthTracker::new();
        let header = PrismHeader {
            version: 0,
            channel_id: 0x001,
            msg_type: 0x02,
            flags: 0,
            sequence: 1,
            timestamp_us: 0,
            payload_length: 5000,
        };
        record_datagram_bandwidth(&tracker, &header);
        assert_eq!(tracker.recv_bytes(0x001), 5000);
    }

    #[tokio::test]
    async fn recv_loop_handle_can_be_created_and_stopped() {
        let (tx, _rx) = tokio::sync::mpsc::channel::<()>(1);
        let handle = RecvLoopHandle { cancel_tx: tx };
        handle.stop().await;
        // No panic = success
    }
}
