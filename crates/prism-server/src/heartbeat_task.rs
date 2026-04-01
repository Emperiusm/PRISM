// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use bytes::{Bytes, BytesMut};
use prism_protocol::{
    channel::CHANNEL_CONTROL,
    header::{PrismHeader, HEADER_SIZE, PROTOCOL_VERSION},
};
use prism_session::control_msg::HEARTBEAT;

/// Zero-allocation heartbeat packet generator.
///
/// The 16-byte PRISM header is pre-built once at construction time.
/// Every call to [`HeartbeatGenerator::packet`] is a cheap `Bytes` clone
/// (an Arc reference-count increment — no heap allocation).
pub struct HeartbeatGenerator {
    packet: Bytes,
}

impl HeartbeatGenerator {
    /// Build the pre-baked heartbeat packet.
    pub fn new() -> Self {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id: CHANNEL_CONTROL,
            msg_type: HEARTBEAT,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        Self {
            packet: buf.freeze(),
        }
    }

    /// Return a clone of the pre-built heartbeat packet.
    ///
    /// This is an Arc reference-count increment — no heap allocation occurs.
    #[inline]
    pub fn packet(&self) -> Bytes {
        self.packet.clone()
    }
}

impl Default for HeartbeatGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_protocol::{
        channel::CHANNEL_CONTROL,
        header::{PrismHeader, HEADER_SIZE},
    };
    use prism_session::control_msg::HEARTBEAT;

    #[test]
    fn heartbeat_packet_is_16_bytes() {
        let hbg = HeartbeatGenerator::new();
        assert_eq!(hbg.packet().len(), HEADER_SIZE);
    }

    #[test]
    fn heartbeat_packet_is_valid_prism_header() {
        let hbg = HeartbeatGenerator::new();
        let pkt = hbg.packet();
        let header = PrismHeader::decode_from_slice(&pkt).expect("decode failed");
        assert_eq!(header.channel_id, CHANNEL_CONTROL);
        assert_eq!(header.msg_type, HEARTBEAT);
        assert_eq!(header.payload_length, 0);
    }

    #[test]
    fn clone_is_cheap() {
        let hbg = HeartbeatGenerator::new();
        let a = hbg.packet();
        let b = hbg.packet();
        // Both Bytes instances point into the same Arc-backed buffer.
        assert_eq!(a.as_ptr(), b.as_ptr());
    }
}
