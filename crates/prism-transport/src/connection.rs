// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Transport connection types: errors, transport variants, stream priorities, metrics, events.

use async_trait::async_trait;
use bytes::Bytes;
use prism_protocol::channel::ChannelPriority;
use std::net::SocketAddr;
#[cfg(test)]
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};
use thiserror::Error;
use tokio::sync::broadcast;

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
    Connected {
        transport_type: TransportType,
        remote_addr: SocketAddr,
    },
    Migrated {
        old_addr: SocketAddr,
        new_addr: SocketAddr,
    },
    MetricsUpdated(TransportMetrics),
    Degraded {
        reason: String,
    },
    Upgraded {
        from: TransportType,
        to: TransportType,
    },
    Disconnected {
        reason: String,
    },
}

// ── Stream inner enums ────────────────────────────────────────────────────────

enum SendStreamInner {
    Quic(quinn::SendStream),
    #[cfg(test)]
    Mock {
        buffer: Arc<StdMutex<Vec<u8>>>,
        finished: Arc<AtomicBool>,
    },
}

enum RecvStreamInner {
    Quic(quinn::RecvStream),
    #[cfg(test)]
    Mock {
        cursor: StdMutex<std::io::Cursor<Vec<u8>>>,
    },
}

// ── OwnedSendStream ───────────────────────────────────────────────────────────

pub struct OwnedSendStream {
    inner: SendStreamInner,
}

impl OwnedSendStream {
    pub fn from_quic(s: quinn::SendStream) -> Self {
        Self {
            inner: SendStreamInner::Quic(s),
        }
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        match &mut self.inner {
            SendStreamInner::Quic(s) => s
                .write_all(data)
                .await
                .map_err(|e| TransportError::StreamError(e.to_string())),
            #[cfg(test)]
            SendStreamInner::Mock { buffer, .. } => {
                buffer.lock().unwrap().extend_from_slice(data);
                Ok(())
            }
        }
    }

    pub fn set_priority(&mut self, priority: StreamPriority) -> Result<(), TransportError> {
        match &mut self.inner {
            SendStreamInner::Quic(s) => s
                .set_priority(priority.to_quinn_priority())
                .map_err(|e| TransportError::StreamError(e.to_string())),
            #[cfg(test)]
            SendStreamInner::Mock { .. } => Ok(()),
        }
    }

    pub async fn finish(self) -> Result<(), TransportError> {
        match self.inner {
            SendStreamInner::Quic(mut s) => s
                .finish()
                .map_err(|e| TransportError::StreamError(e.to_string())),
            #[cfg(test)]
            SendStreamInner::Mock { finished, .. } => {
                finished.store(true, Ordering::Release);
                Ok(())
            }
        }
    }

    #[cfg(test)]
    pub fn mock() -> (Self, Arc<StdMutex<Vec<u8>>>) {
        let buffer = Arc::new(StdMutex::new(Vec::new()));
        let finished = Arc::new(AtomicBool::new(false));
        let stream = Self {
            inner: SendStreamInner::Mock {
                buffer: Arc::clone(&buffer),
                finished,
            },
        };
        (stream, buffer)
    }
}

// ── OwnedRecvStream ───────────────────────────────────────────────────────────

pub struct OwnedRecvStream {
    inner: RecvStreamInner,
}

impl OwnedRecvStream {
    pub fn from_quic(s: quinn::RecvStream) -> Self {
        Self {
            inner: RecvStreamInner::Quic(s),
        }
    }

    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), TransportError> {
        match &mut self.inner {
            RecvStreamInner::Quic(s) => s
                .read_exact(buf)
                .await
                .map_err(|e| TransportError::StreamError(e.to_string())),
            #[cfg(test)]
            RecvStreamInner::Mock { cursor } => {
                use std::io::Read;
                cursor
                    .lock()
                    .unwrap()
                    .read_exact(buf)
                    .map_err(|e| TransportError::StreamError(e.to_string()))
            }
        }
    }

    pub async fn read_to_end(self, limit: usize) -> Result<Vec<u8>, TransportError> {
        match self.inner {
            RecvStreamInner::Quic(mut s) => s
                .read_to_end(limit)
                .await
                .map_err(|e| TransportError::StreamError(e.to_string())),
            #[cfg(test)]
            RecvStreamInner::Mock { cursor } => {
                use std::io::Read;
                let mut buf = Vec::new();
                cursor
                    .into_inner()
                    .unwrap()
                    .read_to_end(&mut buf)
                    .map_err(|e| TransportError::StreamError(e.to_string()))?;
                if buf.len() > limit {
                    return Err(TransportError::MessageTooLarge(buf.len()));
                }
                Ok(buf)
            }
        }
    }

    #[cfg(test)]
    pub fn mock(data: Vec<u8>) -> Self {
        Self {
            inner: RecvStreamInner::Mock {
                cursor: StdMutex::new(std::io::Cursor::new(data)),
            },
        }
    }
}

// ── PrismConnection trait ─────────────────────────────────────────────────────

#[async_trait]
pub trait PrismConnection: Send + Sync {
    fn try_send_datagram(&self, data: Bytes) -> Result<(), TransportError>;
    async fn send_datagram(&self, data: Bytes) -> Result<(), TransportError>;
    async fn recv_datagram(&self) -> Result<Bytes, TransportError>;
    async fn open_bi(
        &self,
        priority: StreamPriority,
    ) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>;
    async fn open_uni(&self, priority: StreamPriority) -> Result<OwnedSendStream, TransportError>;
    async fn accept_bi(&self) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>;
    async fn accept_uni(&self) -> Result<OwnedRecvStream, TransportError>;
    fn metrics(&self) -> TransportMetrics;
    fn transport_type(&self) -> TransportType;
    fn max_datagram_size(&self) -> usize;
    fn events(&self) -> broadcast::Receiver<TransportEvent>;
    async fn close(&self);
}

// ── MockConnection ────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::collections::VecDeque;
    use tokio::sync::Mutex as TokioMutex;

    pub struct MockConnection {
        max_datagram_size: usize,
        datagrams_sent: Arc<StdMutex<Vec<Bytes>>>,
        recv_queue: Arc<TokioMutex<VecDeque<Bytes>>>,
        event_tx: broadcast::Sender<TransportEvent>,
    }

    impl MockConnection {
        pub fn new(max_datagram_size: usize) -> Self {
            let (event_tx, _) = broadcast::channel(16);
            Self {
                max_datagram_size,
                datagrams_sent: Arc::new(StdMutex::new(Vec::new())),
                recv_queue: Arc::new(TokioMutex::new(VecDeque::new())),
                event_tx,
            }
        }

        pub fn sent_datagrams(&self) -> Vec<Bytes> {
            self.datagrams_sent.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PrismConnection for MockConnection {
        fn try_send_datagram(&self, data: Bytes) -> Result<(), TransportError> {
            if data.len() > self.max_datagram_size {
                return Err(TransportError::DatagramTooLarge {
                    size: data.len(),
                    max: self.max_datagram_size,
                });
            }
            self.datagrams_sent.lock().unwrap().push(data);
            Ok(())
        }

        async fn send_datagram(&self, data: Bytes) -> Result<(), TransportError> {
            self.try_send_datagram(data)
        }

        async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
            loop {
                {
                    let mut q = self.recv_queue.lock().await;
                    if let Some(dgram) = q.pop_front() {
                        return Ok(dgram);
                    }
                }
                // yield to avoid busy-spin in tests that don't push anything
                tokio::task::yield_now().await;
            }
        }

        async fn open_bi(
            &self,
            _priority: StreamPriority,
        ) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
            let (send, _buf) = OwnedSendStream::mock();
            let recv = OwnedRecvStream::mock(Vec::new());
            Ok((send, recv))
        }

        async fn open_uni(
            &self,
            _priority: StreamPriority,
        ) -> Result<OwnedSendStream, TransportError> {
            let (send, _buf) = OwnedSendStream::mock();
            Ok(send)
        }

        async fn accept_bi(&self) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
            std::future::pending().await
        }

        async fn accept_uni(&self) -> Result<OwnedRecvStream, TransportError> {
            std::future::pending().await
        }

        fn metrics(&self) -> TransportMetrics {
            TransportMetrics::default()
        }

        fn transport_type(&self) -> TransportType {
            TransportType::Quic
        }

        fn max_datagram_size(&self) -> usize {
            self.max_datagram_size
        }

        fn events(&self) -> broadcast::Receiver<TransportEvent> {
            self.event_tx.subscribe()
        }

        async fn close(&self) {}
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
        let err = TransportError::DatagramTooLarge {
            size: 2000,
            max: 1200,
        };
        assert_eq!(
            format!("{err}"),
            "datagram too large: 2000 bytes (max 1200)"
        );
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
        assert!(
            StreamPriority::Critical.to_quinn_priority()
                < StreamPriority::Background.to_quinn_priority()
        );
        assert_eq!(StreamPriority::Critical.to_quinn_priority(), 0);
        assert_eq!(StreamPriority::Background.to_quinn_priority(), 4);
    }

    #[test]
    fn stream_priority_from_channel_priority() {
        use prism_protocol::channel::ChannelPriority;
        assert_eq!(
            StreamPriority::from(ChannelPriority::Critical),
            StreamPriority::Critical
        );
        assert_eq!(
            StreamPriority::from(ChannelPriority::High),
            StreamPriority::High
        );
        assert_eq!(
            StreamPriority::from(ChannelPriority::Normal),
            StreamPriority::Normal
        );
        assert_eq!(
            StreamPriority::from(ChannelPriority::Low),
            StreamPriority::Low
        );
        assert_eq!(
            StreamPriority::from(ChannelPriority::Background),
            StreamPriority::Background
        );
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
        let event = TransportEvent::Degraded {
            reason: "high loss".into(),
        };
        let cloned = event.clone();
        if let TransportEvent::Degraded { reason } = cloned {
            assert_eq!(reason, "high loss");
        } else {
            panic!("expected Degraded");
        }
    }

    // ── Task 4: OwnedSendStream / OwnedRecvStream / MockConnection ────────────

    #[tokio::test]
    async fn mock_send_stream_captures_writes() {
        let (mut stream, buffer) = OwnedSendStream::mock();
        stream.write(b"hello").await.unwrap();
        stream.write(b" world").await.unwrap();
        assert_eq!(buffer.lock().unwrap().as_slice(), b"hello world");
    }

    #[tokio::test]
    async fn mock_send_stream_finish() {
        let (stream, _buffer) = OwnedSendStream::mock();
        stream.finish().await.unwrap();
    }

    #[tokio::test]
    async fn mock_recv_stream_read_exact() {
        let mut stream = OwnedRecvStream::mock(b"hello world".to_vec());
        let mut buf = [0u8; 5];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b" worl");
    }

    #[tokio::test]
    async fn mock_recv_stream_read_to_end() {
        let stream = OwnedRecvStream::mock(b"payload".to_vec());
        let data = stream.read_to_end(1024).await.unwrap();
        assert_eq!(data, b"payload");
    }

    #[tokio::test]
    async fn mock_connection_datagram_roundtrip() {
        let conn = mock::MockConnection::new(1200);
        conn.try_send_datagram(bytes::Bytes::from_static(b"hello"))
            .unwrap();
        assert_eq!(conn.sent_datagrams().len(), 1);
    }

    #[tokio::test]
    async fn mock_connection_datagram_too_large() {
        let conn = mock::MockConnection::new(4);
        let result = conn.try_send_datagram(bytes::Bytes::from_static(b"toolarge"));
        assert!(matches!(
            result,
            Err(TransportError::DatagramTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn mock_connection_open_bi() {
        let conn = mock::MockConnection::new(1200);
        let (mut send, _recv) = conn.open_bi(StreamPriority::Normal).await.unwrap();
        send.write(b"data").await.unwrap();
    }

    #[tokio::test]
    async fn mock_connection_metadata() {
        let conn = mock::MockConnection::new(1200);
        assert_eq!(conn.transport_type(), TransportType::Quic);
        assert_eq!(conn.max_datagram_size(), 1200);
    }
}
