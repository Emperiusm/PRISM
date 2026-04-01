// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use bytes::{Buf, BufMut, BytesMut};

use crate::error::ProtocolError;

/// PRISM protocol version. 0 = v1.
pub const PROTOCOL_VERSION: u8 = 0;

/// Total header size in bytes.
pub const HEADER_SIZE: usize = 16;

/// Flag bit: this is a keyframe / complete state.
pub const FLAG_KEYFRAME: u8 = 1 << 0;
/// Flag bit: high-priority delivery.
pub const FLAG_PRIORITY: u8 = 1 << 1;
/// Flag bit: payload uses channel-specific compression.
pub const FLAG_COMPRESSED: u8 = 1 << 2;
/// Flag bit: this is a preview frame (Display Engine speculative IDR).
pub const FLAG_PREVIEW: u8 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrismHeader {
    /// Protocol version (4 bits, 0 = v1).
    pub version: u8,
    /// Channel ID (12 bits, 0x000 reserved/invalid).
    pub channel_id: u16,
    /// Channel-specific message type.
    pub msg_type: u8,
    /// Flags (8 bits).
    pub flags: u8,
    /// Per-channel monotonic sequence counter.
    pub sequence: u32,
    /// Microsecond-precision relative timestamp. Epoch resets per session.
    pub timestamp_us: u32,
    /// Byte length of the payload following this header.
    pub payload_length: u32,
}

impl PrismHeader {
    /// Encode header into 16 bytes, little-endian.
    pub fn encode(&self, buf: &mut BytesMut) {
        let ver_chan: u16 = ((self.version as u16 & 0x0F) << 12) | (self.channel_id & 0x0FFF);
        buf.put_u16_le(ver_chan);
        buf.put_u8(self.msg_type);
        buf.put_u8(self.flags);
        buf.put_u32_le(self.sequence);
        buf.put_u32_le(self.timestamp_us);
        buf.put_u32_le(self.payload_length);
    }

    /// Validated constructor. Returns Err on invalid field values.
    pub fn new(
        channel_id: u16,
        msg_type: u8,
        flags: u8,
        sequence: u32,
        timestamp_us: u32,
        payload_length: u32,
    ) -> Result<Self, ProtocolError> {
        if channel_id == 0x000 {
            return Err(ProtocolError::ReservedChannel);
        }
        if channel_id > 0x0FFF {
            return Err(ProtocolError::ChannelIdOverflow(channel_id));
        }
        Ok(Self {
            version: PROTOCOL_VERSION,
            channel_id,
            msg_type,
            flags,
            sequence,
            timestamp_us,
            payload_length,
        })
    }

    /// Encode header directly into a byte slice. Slice must be >= HEADER_SIZE bytes.
    /// Zero-copy: no BytesMut allocation. Used by Display Engine packetizer.
    #[inline(always)]
    pub fn encode_to_slice(&self, buf: &mut [u8]) -> usize {
        let ver_chan: u16 = ((self.version as u16 & 0x0F) << 12) | (self.channel_id & 0x0FFF);
        buf[0..2].copy_from_slice(&ver_chan.to_le_bytes());
        buf[2] = self.msg_type;
        buf[3] = self.flags;
        buf[4..8].copy_from_slice(&self.sequence.to_le_bytes());
        buf[8..12].copy_from_slice(&self.timestamp_us.to_le_bytes());
        buf[12..16].copy_from_slice(&self.payload_length.to_le_bytes());
        HEADER_SIZE
    }

    /// Decode header from a byte slice. Zero-copy.
    #[inline(always)]
    pub fn decode_from_slice(buf: &[u8]) -> Result<Self, ProtocolError> {
        if buf.len() < HEADER_SIZE {
            return Err(ProtocolError::BufferTooShort(buf.len()));
        }
        let ver_chan = u16::from_le_bytes([buf[0], buf[1]]);
        let version = ((ver_chan >> 12) & 0x0F) as u8;
        let channel_id = ver_chan & 0x0FFF;

        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }
        if channel_id == 0x000 {
            return Err(ProtocolError::ReservedChannel);
        }

        Ok(Self {
            version,
            channel_id,
            msg_type: buf[2],
            flags: buf[3],
            sequence: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            timestamp_us: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            payload_length: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
        })
    }

    /// Decode header from 16 bytes, little-endian.
    pub fn decode(buf: &mut impl Buf) -> Result<Self, ProtocolError> {
        if buf.remaining() < HEADER_SIZE {
            return Err(ProtocolError::BufferTooShort(buf.remaining()));
        }

        let ver_chan = buf.get_u16_le();
        let version = ((ver_chan >> 12) & 0x0F) as u8;
        let channel_id = ver_chan & 0x0FFF;

        let msg_type = buf.get_u8();
        let flags = buf.get_u8();
        let sequence = buf.get_u32_le();
        let timestamp_us = buf.get_u32_le();
        let payload_length = buf.get_u32_le();

        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }
        if channel_id == 0x000 {
            return Err(ProtocolError::ReservedChannel);
        }

        Ok(PrismHeader {
            version,
            channel_id,
            msg_type,
            flags,
            sequence,
            timestamp_us,
            payload_length,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn roundtrip_basic() {
        let header = PrismHeader {
            version: 0,
            channel_id: 0x001,
            msg_type: 0x01,
            flags: 0,
            sequence: 42,
            timestamp_us: 123456,
            payload_length: 1024,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        assert_eq!(buf.len(), HEADER_SIZE);
        let decoded = PrismHeader::decode(&mut buf.freeze()).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn roundtrip_max_values() {
        let header = PrismHeader {
            version: 0,
            channel_id: 0x0FFF,
            msg_type: 0xFF,
            flags: 0xFF,
            sequence: u32::MAX,
            timestamp_us: u32::MAX,
            payload_length: u32::MAX,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        assert_eq!(buf.len(), HEADER_SIZE);
        let decoded = PrismHeader::decode(&mut buf.freeze()).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn reserved_channel_rejected() {
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        buf.put_u16_le(0x0000); // version 0, channel 0x000
        buf.put_u8(0);
        buf.put_u8(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        let result = PrismHeader::decode(&mut buf.freeze());
        assert!(matches!(result, Err(ProtocolError::ReservedChannel)));
    }

    #[test]
    fn buffer_too_short() {
        let buf = Bytes::from_static(&[0u8; 8]);
        let result = PrismHeader::decode(&mut buf.clone());
        assert!(matches!(result, Err(ProtocolError::BufferTooShort(8))));
    }

    #[test]
    fn flag_bits() {
        let header = PrismHeader {
            version: 0,
            channel_id: 0x001,
            msg_type: 0,
            flags: FLAG_KEYFRAME | FLAG_PRIORITY,
            sequence: 0,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        let decoded = PrismHeader::decode(&mut buf.freeze()).unwrap();
        assert_ne!(decoded.flags & FLAG_KEYFRAME, 0);
        assert_ne!(decoded.flags & FLAG_PRIORITY, 0);
        assert_eq!(decoded.flags & FLAG_COMPRESSED, 0);
    }

    #[test]
    fn unsupported_version() {
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        let ver_chan: u16 = (1u16 << 12) | 0x001;
        buf.put_u16_le(ver_chan);
        buf.put_u8(0);
        buf.put_u8(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        let result = PrismHeader::decode(&mut buf.freeze());
        assert!(matches!(result, Err(ProtocolError::UnsupportedVersion(1))));
    }

    #[test]
    fn version_and_channel_packing() {
        let header = PrismHeader {
            version: 0,
            channel_id: 0x0E1,
            msg_type: 0,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        let first_u16 = u16::from_le_bytes([buf[0], buf[1]]);
        assert_eq!((first_u16 >> 12) & 0x0F, 0);
        assert_eq!(first_u16 & 0x0FFF, 0x0E1);
    }

    #[test]
    fn new_validates_reserved_channel() {
        let result = PrismHeader::new(0x000, 0, 0, 0, 0, 0);
        assert!(matches!(result, Err(ProtocolError::ReservedChannel)));
    }

    #[test]
    fn new_validates_channel_overflow() {
        let result = PrismHeader::new(0x2000, 0, 0, 0, 0, 0);
        assert!(matches!(
            result,
            Err(ProtocolError::ChannelIdOverflow(0x2000))
        ));
    }

    #[test]
    fn new_sets_protocol_version() {
        let header = PrismHeader::new(0x001, 0x01, 0, 42, 0, 0).unwrap();
        assert_eq!(header.version, PROTOCOL_VERSION);
        assert_eq!(header.channel_id, 0x001);
    }

    #[test]
    fn encode_decode_from_slice_roundtrip() {
        let header = PrismHeader::new(0x001, 0x01, FLAG_KEYFRAME, 42, 123456, 1024).unwrap();
        let mut buf = [0u8; HEADER_SIZE];
        let written = header.encode_to_slice(&mut buf);
        assert_eq!(written, HEADER_SIZE);
        let decoded = PrismHeader::decode_from_slice(&buf).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn decode_from_slice_too_short() {
        let buf = [0u8; 8];
        let result = PrismHeader::decode_from_slice(&buf);
        assert!(matches!(result, Err(ProtocolError::BufferTooShort(8))));
    }

    #[test]
    fn header_size_is_16_bytes() {
        let header = PrismHeader {
            version: 0,
            channel_id: 0x007,
            msg_type: 0x42,
            flags: FLAG_COMPRESSED,
            sequence: 999,
            timestamp_us: 555,
            payload_length: 65536,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        assert_eq!(buf.len(), 16);
    }
}
