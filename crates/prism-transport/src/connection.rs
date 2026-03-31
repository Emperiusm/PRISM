// Transport connection types: errors, transport variants, stream priorities, metrics, events.

use thiserror::Error;
use prism_protocol::channel::ChannelPriority;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("connection closed")]
    ConnectionClosed,
    #[error("datagram too large: {size} bytes (max {max})")]
    DatagramTooLarge { size: usize, max: usize },
    #[error("datagrams not supported on this transport")]
    DatagramUnsupported,
    #[error("would block")]
    WouldBlock,
    #[error("rate limited")]
    RateLimited,
    #[error("stream error: {0}")]
    StreamError(String),
    #[error("connection error: {0}")]
    ConnectionError(String),
    #[error("message too large: {0} bytes")]
    MessageTooLarge(usize),
    #[error("timeout")]
    Timeout,
    #[error("all transports failed")]
    AllTransportsFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportType {
    Quic,
    WebSocket,
    Tcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StreamPriority {
    Background = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

impl StreamPriority {
    pub fn to_quinn_priority(self) -> i32 {
        4 - self as i32
    }
}

impl From<ChannelPriority> for StreamPriority {
    fn from(p: ChannelPriority) -> Self {
        match p {
            ChannelPriority::Background => StreamPriority::Background,
            ChannelPriority::Low => StreamPriority::Low,
            ChannelPriority::Normal => StreamPriority::Normal,
            ChannelPriority::High => StreamPriority::High,
            ChannelPriority::Critical => StreamPriority::Critical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_error_display_connection_closed() {
        let err = TransportError::ConnectionClosed;
        assert_eq!(format!("{err}"), "connection closed");
    }

    #[test]
    fn transport_error_display_datagram_too_large() {
        let err = TransportError::DatagramTooLarge { size: 2000, max: 1200 };
        assert_eq!(format!("{err}"), "datagram too large: 2000 bytes (max 1200)");
    }

    #[test]
    fn transport_error_display_message_too_large() {
        let err = TransportError::MessageTooLarge(20_000_000);
        assert_eq!(format!("{err}"), "message too large: 20000000 bytes");
    }

    #[test]
    fn transport_type_equality() {
        assert_eq!(TransportType::Quic, TransportType::Quic);
        assert_ne!(TransportType::Quic, TransportType::WebSocket);
    }

    #[test]
    fn stream_priority_ordering_matches_channel_priority() {
        assert!(StreamPriority::Critical > StreamPriority::High);
        assert!(StreamPriority::High > StreamPriority::Normal);
        assert!(StreamPriority::Normal > StreamPriority::Low);
        assert!(StreamPriority::Low > StreamPriority::Background);
    }

    #[test]
    fn stream_priority_to_quinn_maps_correctly() {
        assert!(StreamPriority::Critical.to_quinn_priority() < StreamPriority::Background.to_quinn_priority());
        assert_eq!(StreamPriority::Critical.to_quinn_priority(), 0);
        assert_eq!(StreamPriority::Background.to_quinn_priority(), 4);
    }

    #[test]
    fn stream_priority_from_channel_priority() {
        use prism_protocol::channel::ChannelPriority;
        assert_eq!(StreamPriority::from(ChannelPriority::Critical), StreamPriority::Critical);
        assert_eq!(StreamPriority::from(ChannelPriority::High), StreamPriority::High);
        assert_eq!(StreamPriority::from(ChannelPriority::Normal), StreamPriority::Normal);
        assert_eq!(StreamPriority::from(ChannelPriority::Low), StreamPriority::Low);
        assert_eq!(StreamPriority::from(ChannelPriority::Background), StreamPriority::Background);
    }
}
