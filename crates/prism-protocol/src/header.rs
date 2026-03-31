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
