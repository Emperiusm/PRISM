// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use bytes::{Buf, Bytes, BytesMut};

use crate::error::ProtocolError;
use crate::header::{HEADER_SIZE, PrismHeader};

// === Core channels (0x001-0x0FF) ===
pub const CHANNEL_DISPLAY: u16 = 0x001;
pub const CHANNEL_INPUT: u16 = 0x002;
pub const CHANNEL_AUDIO: u16 = 0x003;
pub const CHANNEL_CLIPBOARD: u16 = 0x004;
pub const CHANNEL_DEVICE: u16 = 0x005;
pub const CHANNEL_CONTROL: u16 = 0x006;
pub const CHANNEL_FILESHARE: u16 = 0x007;

// === Mobile extension channels (0x0E0-0x0EF) ===
pub const CHANNEL_NOTIFY: u16 = 0x0E1;
pub const CHANNEL_CAMERA: u16 = 0x0E2;
pub const CHANNEL_SENSOR: u16 = 0x0E3;
pub const CHANNEL_TOUCH: u16 = 0x0E4;

// === Extension channel range ===
pub const EXTENSION_CHANNEL_START: u16 = 0x100;
pub const EXTENSION_CHANNEL_END: u16 = 0xFFF;

/// A complete PRISM packet: header + payload.
#[derive(Debug, Clone)]
pub struct PrismPacket {
    pub header: PrismHeader,
    pub payload: Bytes,
}

impl PrismPacket {
    /// Encode the full packet (header + payload) into a buffer.
    pub fn encode(&self, buf: &mut BytesMut) {
        self.header.encode(buf);
        buf.extend_from_slice(&self.payload);
    }

    /// Decode a packet from a buffer. Consumes header + payload_length bytes.
    pub fn decode(buf: &mut impl Buf) -> Result<Self, ProtocolError> {
        let header = PrismHeader::decode(buf)?;
        if buf.remaining() < header.payload_length as usize {
            return Err(ProtocolError::BufferTooShort(buf.remaining()));
        }
        let payload = buf.copy_to_bytes(header.payload_length as usize);
        Ok(Self { header, payload })
    }

    /// Total wire size of this packet.
    pub fn wire_size(&self) -> usize {
        HEADER_SIZE + self.payload.len()
    }
}

/// Channel priority levels for bandwidth arbitration (R14).
/// Higher numeric value = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChannelPriority {
    Background = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Returns the default priority for a channel ID.
pub fn channel_priority(channel_id: u16) -> ChannelPriority {
    match channel_id {
        CHANNEL_INPUT => ChannelPriority::Critical,
        CHANNEL_DISPLAY | CHANNEL_AUDIO => ChannelPriority::High,
        CHANNEL_CONTROL | CHANNEL_CLIPBOARD => ChannelPriority::Normal,
        CHANNEL_FILESHARE | CHANNEL_DEVICE => ChannelPriority::Low,
        CHANNEL_SENSOR | CHANNEL_NOTIFY => ChannelPriority::Background,
        _ => ChannelPriority::Normal,
    }
}

/// Maps channel priority to a priority category index (0-4).
/// Used by bandwidth tracker for fixed-size array indexing.
pub fn priority_category(channel_id: u16) -> usize {
    channel_priority(channel_id) as usize
}

/// Whether a channel uses datagrams (unreliable) or streams (reliable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelTransport {
    /// Unreliable QUIC datagrams. Loss-tolerant, lowest latency.
    Datagram,
    /// Reliable QUIC streams. Every byte must arrive.
    Stream,
    /// Keyframes on stream, delta frames on datagram.
    Hybrid,
}

/// Returns the default transport mode for a channel ID.
pub fn channel_transport(channel_id: u16) -> ChannelTransport {
    match channel_id {
        CHANNEL_DISPLAY => ChannelTransport::Hybrid,
        CHANNEL_INPUT | CHANNEL_AUDIO | CHANNEL_CAMERA | CHANNEL_SENSOR => {
            ChannelTransport::Datagram
        }
        CHANNEL_CLIPBOARD | CHANNEL_DEVICE | CHANNEL_CONTROL | CHANNEL_FILESHARE
        | CHANNEL_NOTIFY | CHANNEL_TOUCH => ChannelTransport::Stream,
        _ => ChannelTransport::Stream,
    }
}

/// Returns the priority weight for bandwidth arbitration.
/// Higher weight = larger share of available bandwidth.
pub fn priority_weight(priority: ChannelPriority) -> u32 {
    match priority {
        ChannelPriority::Critical => 16,
        ChannelPriority::High => 8,
        ChannelPriority::Normal => 4,
        ChannelPriority::Low => 2,
        ChannelPriority::Background => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_is_critical_priority() {
        assert_eq!(channel_priority(CHANNEL_INPUT), ChannelPriority::Critical);
    }

    #[test]
    fn display_is_high_priority() {
        assert_eq!(channel_priority(CHANNEL_DISPLAY), ChannelPriority::High);
    }

    #[test]
    fn fileshare_is_low_priority() {
        assert_eq!(channel_priority(CHANNEL_FILESHARE), ChannelPriority::Low);
    }

    #[test]
    fn display_is_hybrid_transport() {
        assert_eq!(channel_transport(CHANNEL_DISPLAY), ChannelTransport::Hybrid);
    }

    #[test]
    fn input_is_datagram_transport() {
        assert_eq!(channel_transport(CHANNEL_INPUT), ChannelTransport::Datagram);
    }

    #[test]
    fn fileshare_is_stream_transport() {
        assert_eq!(
            channel_transport(CHANNEL_FILESHARE),
            ChannelTransport::Stream
        );
    }

    #[test]
    fn priority_ordering() {
        assert!(ChannelPriority::Critical > ChannelPriority::High);
        assert!(ChannelPriority::High > ChannelPriority::Normal);
        assert!(ChannelPriority::Normal > ChannelPriority::Low);
        assert!(ChannelPriority::Low > ChannelPriority::Background);
    }

    #[test]
    fn priority_weights_are_monotonic() {
        assert!(
            priority_weight(ChannelPriority::Critical) > priority_weight(ChannelPriority::High)
        );
        assert!(priority_weight(ChannelPriority::High) > priority_weight(ChannelPriority::Normal));
        assert!(priority_weight(ChannelPriority::Normal) > priority_weight(ChannelPriority::Low));
        assert!(
            priority_weight(ChannelPriority::Low) > priority_weight(ChannelPriority::Background)
        );
    }

    #[test]
    fn unknown_channel_defaults_to_normal_stream() {
        assert_eq!(channel_priority(0x100), ChannelPriority::Normal);
        assert_eq!(channel_transport(0x100), ChannelTransport::Stream);
    }

    #[test]
    fn mobile_channels_have_expected_transport() {
        assert_eq!(
            channel_transport(CHANNEL_CAMERA),
            ChannelTransport::Datagram
        );
        assert_eq!(
            channel_transport(CHANNEL_SENSOR),
            ChannelTransport::Datagram
        );
        assert_eq!(channel_transport(CHANNEL_NOTIFY), ChannelTransport::Stream);
        assert_eq!(channel_transport(CHANNEL_TOUCH), ChannelTransport::Stream);
    }

    #[test]
    fn packet_encode_decode_roundtrip() {
        use bytes::BytesMut;
        let header = PrismHeader {
            version: 0,
            channel_id: CHANNEL_CONTROL,
            msg_type: 0x01,
            flags: 0,
            sequence: 1,
            timestamp_us: 5000,
            payload_length: 5,
        };
        let packet = PrismPacket {
            header,
            payload: Bytes::from_static(b"hello"),
        };
        let mut buf = BytesMut::with_capacity(packet.wire_size());
        packet.encode(&mut buf);
        assert_eq!(buf.len(), HEADER_SIZE + 5);

        let decoded = PrismPacket::decode(&mut buf.freeze()).unwrap();
        assert_eq!(decoded.header, header);
        assert_eq!(decoded.payload, Bytes::from_static(b"hello"));
    }

    #[test]
    fn packet_wire_size() {
        let packet = PrismPacket {
            header: PrismHeader {
                version: 0,
                channel_id: 0x001,
                msg_type: 0,
                flags: 0,
                sequence: 0,
                timestamp_us: 0,
                payload_length: 100,
            },
            payload: Bytes::from(vec![0u8; 100]),
        };
        assert_eq!(packet.wire_size(), HEADER_SIZE + 100);
    }

    #[test]
    fn priority_category_indexes() {
        assert_eq!(priority_category(CHANNEL_INPUT), 4);
        assert_eq!(priority_category(CHANNEL_DISPLAY), 3);
        assert_eq!(priority_category(CHANNEL_CONTROL), 2);
        assert_eq!(priority_category(CHANNEL_FILESHARE), 1);
        assert_eq!(priority_category(CHANNEL_SENSOR), 0);
    }
}
