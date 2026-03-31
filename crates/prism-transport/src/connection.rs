// Transport connection types: errors, transport variants, stream priorities, metrics, events.

use std::net::SocketAddr;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelayAsymmetry {
    Unknown,
    Symmetric,
    DownstreamSlow { ratio: f32 },
    UpstreamSlow { ratio: f32 },
}

#[derive(Debug, Clone, Copy)]
pub struct TransportMetrics {
    pub rtt_us: u64,
    pub rtt_variance_us: u64,
    pub loss_rate: f32,
    pub theoretical_bandwidth_bps: u64,
    pub actual_send_bps: u64,
    pub actual_recv_bps: u64,
    pub downstream_delay_us: Option<i64>,
    pub upstream_delay_us: Option<i64>,
    pub delay_asymmetry: DelayAsymmetry,
    pub transport_type: TransportType,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub datagrams_sent: u64,
    pub datagrams_dropped: u64,
    pub probe_rtt_us: u64,
}

impl Default for TransportMetrics {
    fn default() -> Self {
        Self {
            rtt_us: 0,
            rtt_variance_us: 0,
            loss_rate: 0.0,
            theoretical_bandwidth_bps: 0,
            actual_send_bps: 0,
            actual_recv_bps: 0,
            downstream_delay_us: None,
            upstream_delay_us: None,
            delay_asymmetry: DelayAsymmetry::Unknown,
            transport_type: TransportType::Quic,
            bytes_sent: 0,
            bytes_received: 0,
            datagrams_sent: 0,
            datagrams_dropped: 0,
            probe_rtt_us: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransportEvent {
    Connected { transport_type: TransportType, remote_addr: SocketAddr },
    Migrated { old_addr: SocketAddr, new_addr: SocketAddr },
    MetricsUpdated(TransportMetrics),
    Degraded { reason: String },
    Upgraded { from: TransportType, to: TransportType },
    Disconnected { reason: String },
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

    #[test]
    fn delay_asymmetry_default_is_unknown() {
        let a = DelayAsymmetry::Unknown;
        assert_eq!(a, DelayAsymmetry::Unknown);
    }

    #[test]
    fn delay_asymmetry_downstream_slow() {
        let a = DelayAsymmetry::DownstreamSlow { ratio: 2.5 };
        if let DelayAsymmetry::DownstreamSlow { ratio } = a {
            assert!((ratio - 2.5).abs() < f32::EPSILON);
        } else {
            panic!("expected DownstreamSlow");
        }
    }

    #[test]
    fn transport_metrics_default_is_zeroed() {
        let m = TransportMetrics::default();
        assert_eq!(m.rtt_us, 0);
        assert_eq!(m.loss_rate, 0.0);
        assert_eq!(m.bytes_sent, 0);
        assert_eq!(m.transport_type, TransportType::Quic);
        assert_eq!(m.delay_asymmetry, DelayAsymmetry::Unknown);
    }

    #[test]
    fn transport_event_clone() {
        let event = TransportEvent::Degraded { reason: "high loss".into() };
        let cloned = event.clone();
        if let TransportEvent::Degraded { reason } = cloned {
            assert_eq!(reason, "high loss");
        } else {
            panic!("expected Degraded");
        }
    }
}
