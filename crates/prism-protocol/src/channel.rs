use bytes::Bytes;

use crate::header::PrismHeader;

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
        assert_eq!(channel_transport(CHANNEL_FILESHARE), ChannelTransport::Stream);
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
        assert!(priority_weight(ChannelPriority::Critical) > priority_weight(ChannelPriority::High));
        assert!(priority_weight(ChannelPriority::High) > priority_weight(ChannelPriority::Normal));
        assert!(priority_weight(ChannelPriority::Normal) > priority_weight(ChannelPriority::Low));
        assert!(priority_weight(ChannelPriority::Low) > priority_weight(ChannelPriority::Background));
    }

    #[test]
    fn unknown_channel_defaults_to_normal_stream() {
        assert_eq!(channel_priority(0x100), ChannelPriority::Normal);
        assert_eq!(channel_transport(0x100), ChannelTransport::Stream);
    }

    #[test]
    fn mobile_channels_have_expected_transport() {
        assert_eq!(channel_transport(CHANNEL_CAMERA), ChannelTransport::Datagram);
        assert_eq!(channel_transport(CHANNEL_SENSOR), ChannelTransport::Datagram);
        assert_eq!(channel_transport(CHANNEL_NOTIFY), ChannelTransport::Stream);
        assert_eq!(channel_transport(CHANNEL_TOUCH), ChannelTransport::Stream);
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
