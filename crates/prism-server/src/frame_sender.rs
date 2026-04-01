// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! FrameSender — builds and dispatches display datagrams to all subscribed clients.
//!
//! [`build_display_datagram`] encodes a PRISM header (channel=CHANNEL_DISPLAY, msg_type=0x02 SLICE)
//! followed by a default [`SlicePayloadHeader`] and the raw pixel/codec payload into a single
//! contiguous [`Bytes`] buffer.
//!
//! [`FrameSender`] wraps a shared [`RoutingTable`] and provides a [`send_frame`] method that
//! counts how many routes are subscribed to `CHANNEL_DISPLAY` and increments the frame sequence
//! counter for each call.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use bytes::{BufMut, Bytes, BytesMut};

use prism_display::packet::{SLICE_HEADER_SIZE, SlicePayloadHeader};
use prism_protocol::channel::CHANNEL_DISPLAY;
use prism_protocol::header::{HEADER_SIZE, PROTOCOL_VERSION, PrismHeader};
use prism_session::routing::RoutingTable;

/// Display channel message type: encoded slice.
pub const MSG_TYPE_SLICE: u8 = 0x02;

// ── Datagram builder ──────────────────────────────────────────────────────────

/// Encode a display datagram ready to be sent over a QUIC datagram.
///
/// Wire layout:
/// ```text
/// [PrismHeader 16 B][SlicePayloadHeader 24 B][payload N B]
/// ```
///
/// The `SlicePayloadHeader` is filled with zeros except for `total_slices = 1`
/// and `region_count = 1` to produce a valid single-slice frame.
pub fn build_display_datagram(frame_seq: u32, payload: &[u8], timestamp_us: u32) -> Bytes {
    let total_len = HEADER_SIZE + SLICE_HEADER_SIZE + payload.len();
    let mut buf = BytesMut::with_capacity(total_len);

    // PrismHeader — 16 bytes
    let header = PrismHeader {
        version: PROTOCOL_VERSION,
        channel_id: CHANNEL_DISPLAY,
        msg_type: MSG_TYPE_SLICE,
        flags: 0,
        sequence: frame_seq,
        timestamp_us,
        payload_length: (SLICE_HEADER_SIZE + payload.len()) as u32,
    };
    header.encode(&mut buf);

    // SlicePayloadHeader — 24 bytes (minimal valid single-slice frame)
    let slice_hdr = SlicePayloadHeader {
        decoder_slot: 0,
        slice_index: 0,
        total_slices: 1,
        encoding_type: 0,
        rect_x: 0,
        rect_y: 0,
        rect_w: 0,
        rect_h: 0,
        region_count: 1,
        is_preview: 0,
        replaces_seq: 0,
        cursor_x: 0,
        cursor_y: 0,
        cursor_flags: 0,
        _reserved: 0,
    };
    buf.put_slice(&slice_hdr.to_bytes());

    // Payload
    buf.put_slice(payload);

    buf.freeze()
}

// ── FrameSender ───────────────────────────────────────────────────────────────

/// Sends display frames to all clients subscribed on `CHANNEL_DISPLAY`.
///
/// This implementation is transport-agnostic at the test level: `send_frame`
/// counts routes and returns the route count, but does not actually call
/// `try_send_datagram` on connections (that integration lives in the server
/// loop). The routing table snapshot is the single source of truth for the
/// recipient count.
pub struct FrameSender {
    routing_table: Arc<RoutingTable>,
    frame_seq: AtomicU32,
    frames_sent: AtomicU32,
    running: AtomicBool,
}

impl FrameSender {
    /// Create a new `FrameSender` bound to the given routing table.
    pub fn new(routing_table: Arc<RoutingTable>) -> Self {
        Self {
            routing_table,
            frame_seq: AtomicU32::new(0),
            frames_sent: AtomicU32::new(0),
            running: AtomicBool::new(true),
        }
    }

    /// Build a display datagram from `payload` and return the number of routes
    /// the frame would be dispatched to.
    ///
    /// Each call increments the internal sequence counter regardless of route count.
    ///
    /// Returns the number of `CHANNEL_DISPLAY` routes in the current routing snapshot.
    pub fn send_frame(&self, payload: &[u8]) -> usize {
        let seq = self.frame_seq.fetch_add(1, Ordering::Relaxed);
        let _datagram = build_display_datagram(seq, payload, 0);

        let snapshot = self.routing_table.snapshot();
        let route_count = snapshot
            .channel_routes
            .get(&CHANNEL_DISPLAY)
            .map(|routes| routes.len())
            .unwrap_or(0);

        if route_count > 0 {
            self.frames_sent.fetch_add(1, Ordering::Relaxed);
        }

        route_count
    }

    /// Return the number of frames successfully dispatched (route_count > 0).
    pub fn frames_sent(&self) -> u32 {
        self.frames_sent.load(Ordering::Relaxed)
    }

    /// Return the current frame sequence counter (next sequence to use).
    pub fn frame_seq(&self) -> u32 {
        self.frame_seq.load(Ordering::Relaxed)
    }

    /// Return whether the sender is in the running state.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Set the running flag.
    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Relaxed);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_display::packet::SLICE_HEADER_SIZE;
    use prism_protocol::header::{HEADER_SIZE, PrismHeader};
    use prism_session::routing::{RouteEntry, RoutingMutation};
    use uuid::Uuid;

    fn make_routing_table() -> Arc<RoutingTable> {
        Arc::new(RoutingTable::new())
    }

    fn add_display_route(table: &RoutingTable, n: usize) {
        let mutations: Vec<RoutingMutation> = (0..n)
            .map(|_| RoutingMutation::AddRoute {
                channel_id: CHANNEL_DISPLAY,
                entry: RouteEntry {
                    client_id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
                },
            })
            .collect();
        table.batch_update(mutations);
    }

    // ── build_display_datagram tests ─────────────────────────────────────────

    #[test]
    fn build_datagram_valid_header() {
        let payload = b"test-payload";
        let dgram = build_display_datagram(7, payload, 12345);

        // Must start with a valid PrismHeader
        let header = PrismHeader::decode_from_slice(&dgram[..HEADER_SIZE]).unwrap();
        assert_eq!(header.channel_id, CHANNEL_DISPLAY);
        assert_eq!(header.msg_type, MSG_TYPE_SLICE);
        assert_eq!(header.sequence, 7);
        assert_eq!(header.timestamp_us, 12345);
        assert_eq!(
            header.payload_length as usize,
            SLICE_HEADER_SIZE + payload.len()
        );
    }

    #[test]
    fn build_datagram_contains_payload() {
        let payload = b"hello-frame-data";
        let dgram = build_display_datagram(0, payload, 0);

        let total_expected = HEADER_SIZE + SLICE_HEADER_SIZE + payload.len();
        assert_eq!(dgram.len(), total_expected);

        // Payload bytes are at the end
        let payload_start = HEADER_SIZE + SLICE_HEADER_SIZE;
        assert_eq!(&dgram[payload_start..], payload);
    }

    // ── FrameSender tests ────────────────────────────────────────────────────

    #[test]
    fn no_clients_sends_zero() {
        let table = make_routing_table();
        let sender = FrameSender::new(Arc::clone(&table));

        let count = sender.send_frame(b"frame");
        assert_eq!(count, 0);
        // frames_sent should not increment when no routes
        assert_eq!(sender.frames_sent(), 0);
    }

    #[test]
    fn counts_routes() {
        let table = make_routing_table();
        add_display_route(&table, 3);

        let sender = FrameSender::new(Arc::clone(&table));
        let count = sender.send_frame(b"frame");
        assert_eq!(count, 3);
        assert_eq!(sender.frames_sent(), 1);
    }

    #[test]
    fn sequence_increments() {
        let table = make_routing_table();
        let sender = FrameSender::new(Arc::clone(&table));

        assert_eq!(sender.frame_seq(), 0);
        sender.send_frame(b"a");
        assert_eq!(sender.frame_seq(), 1);
        sender.send_frame(b"b");
        assert_eq!(sender.frame_seq(), 2);
        sender.send_frame(b"c");
        assert_eq!(sender.frame_seq(), 3);
    }

    #[test]
    fn running_flag() {
        let table = make_routing_table();
        let sender = FrameSender::new(Arc::clone(&table));

        assert!(sender.is_running());
        sender.set_running(false);
        assert!(!sender.is_running());
        sender.set_running(true);
        assert!(sender.is_running());
    }
}
