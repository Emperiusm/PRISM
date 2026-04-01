# Plan 3: Transport Implementation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-transport` crate providing transport abstractions (`PrismConnection` trait, owned streams, framing), dual-connection `UnifiedConnection` facade, connection quality measurement (bandwidth, one-way delay, trend detection, composite scoring), transport utilities (MTU tracking, datagram coalescing, stream pooling, probing), and QUIC configuration with BBR/Cubic profiles, DSCP marking, and session-token-authenticated throughput binding.

**Architecture:** `prism-transport` depends on `prism-protocol` (channel IDs, priorities) and `prism-metrics` (metrics recording). Consumers code against the `PrismConnection` async trait — they never know whether the underlying transport is QUIC or WebSocket. `UnifiedConnection` routes channels to the correct underlying connection (latency vs throughput) based on `priority_category()`. Quality measurement is pure computation: `BandwidthEstimator` (sliding window), `OneWayDelayEstimator` (min-offset), `TrendDetector` (EMA + slope), composed into `ConnectionQuality` with a single 0.0–1.0 score and `QualityRecommendation`. QUIC uses dual endpoints per server (latency BBR + throughput Cubic) with separate UDP sockets and DSCP markings. The throughput connection authenticates via HMAC session token — no second Noise handshake. `OwnedSendStream`/`OwnedRecvStream` wrap quinn stream types directly — callers own streams, zero transport-layer bookkeeping.

**Tech Stack:** `quinn` (QUIC), `tokio` (async runtime), `async-trait` (dyn-compatible async traits), `bytes` (zero-copy buffers), `thiserror` (error types), `hmac`+`sha2` (session token), `socket2` (DSCP/buffer sizing), `uuid` (device ID), `prism-protocol` (channels), `prism-metrics` (recording)

**Spec refs:**
- Transport: `docs/superpowers/specs/2026-03-30-transport-design.md` (all sections)
- Architecture: `docs/superpowers/specs/2026-03-30-prism-architecture-design.md` (R4, R8, R14, R26, R27, R34, R37)

---

## File Structure

```
PRISM/
  crates/
    prism-transport/
      Cargo.toml
      src/
        lib.rs                      # re-exports
        connection.rs               # TransportError, TransportType, StreamPriority,
                                    # TransportMetrics, DelayAsymmetry, TransportEvent,
                                    # OwnedSendStream, OwnedRecvStream, PrismConnection trait,
                                    # MockConnection (#[cfg(test)])
        framing.rs                  # FramedWriter, FramedReader, MAX_MESSAGE_SIZE
        unified.rs                  # UnifiedConnection, ChannelRouting, ConnectionSlot

        quality/
            mod.rs                  # ConnectionQuality, QualityRecommendation, ProbeQuality,
                                    # BandwidthQuality, LossQuality
            bandwidth.rs            # BandwidthEstimator (sliding window, 100ms samples, 5s window)
            one_way_delay.rs        # OneWayDelayEstimator (min-offset per direction)
            trend.rs                # TrendDetector (5s/30s EMA + 60s linear regression slope)
            prober.rs               # ConnectionProber, ProbePayload, ProbeEcho, ProbeResult,
                                    # ActivityState
            mtu.rs                  # MtuTracker (cached max_datagram_size, 1s recheck)

        coalesce.rs                 # DatagramCoalescer (1ms batching, length-prefixed sub-datagrams)
        stream_pool.rs              # StreamPool (pre-opened bi streams, maintain loop)

        quic/
            mod.rs                  # re-exports
            config.rs               # latency_transport_config, throughput_transport_config
            socket.rs               # create_latency_socket, create_throughput_socket, set_dscp
            auth_token.rs           # generate_throughput_token, validate_throughput_token
            connection.rs           # QuicConnection impl PrismConnection
```

---

## Task 1: Crate Setup

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-transport/Cargo.toml`
- Create: `crates/prism-transport/src/lib.rs`
- Create: all placeholder source files

- [ ] **Step 1: Update workspace root Cargo.toml**

Add `prism-transport` to members and new dependencies:

```toml
[workspace]
resolver = "2"
members = [
    "crates/prism-protocol",
    "crates/prism-metrics",
    "crates/prism-security",
    "crates/prism-transport",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "CC0-1.0"

[workspace.dependencies]
# existing deps stay unchanged, add:
quinn = "0.11"
tokio = { version = "1", features = ["sync", "time", "net", "macros", "rt-multi-thread"] }
async-trait = "0.1"
hmac = "0.12"
socket2 = { version = "0.5", features = ["all"] }
rcgen = "0.13"
rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }

prism-transport = { path = "crates/prism-transport" }
```

Keep all existing workspace deps (`bytes`, `thiserror`, `serde`, `serde_json`, `snow`, `x25519-dalek`, `ed25519-dalek`, `uuid`, `rand`, `hex`, `aes-gcm`, `hkdf`, `sha2`, `arc-swap`, `tempfile`, `prism-protocol`, `prism-metrics`, `prism-security`) unchanged.

- [ ] **Step 2: Create crates/prism-transport/Cargo.toml**

```toml
[package]
name = "prism-transport"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
prism-protocol = { workspace = true }
prism-metrics = { workspace = true }
bytes = { workspace = true }
thiserror = { workspace = true }
quinn = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
hmac = { workspace = true }
sha2 = { workspace = true }
uuid = { workspace = true }
socket2 = { workspace = true }

[dev-dependencies]
rcgen = { workspace = true }
rustls = { workspace = true }
```

- [ ] **Step 3: Create lib.rs with module declarations**

```rust
pub mod connection;
pub mod framing;
pub mod unified;
pub mod coalesce;
pub mod stream_pool;
pub mod quality;
pub mod quic;
```

- [ ] **Step 4: Create placeholder source files**

Create each file with just enough to compile:

`crates/prism-transport/src/connection.rs`:
```rust
// Transport connection types, traits, and errors.
```

`crates/prism-transport/src/framing.rs`:
```rust
// Length-prefixed message framing.
```

`crates/prism-transport/src/unified.rs`:
```rust
// UnifiedConnection dual-connection facade.
```

`crates/prism-transport/src/coalesce.rs`:
```rust
// Datagram coalescing for small messages.
```

`crates/prism-transport/src/stream_pool.rs`:
```rust
// Pre-opened stream pool.
```

`crates/prism-transport/src/quality/mod.rs`:
```rust
pub mod bandwidth;
pub mod one_way_delay;
pub mod trend;
pub mod prober;
pub mod mtu;
```

`crates/prism-transport/src/quality/bandwidth.rs`:
```rust
// Sliding window bandwidth estimator.
```

`crates/prism-transport/src/quality/one_way_delay.rs`:
```rust
// One-way delay estimator with min-offset tracking.
```

`crates/prism-transport/src/quality/trend.rs`:
```rust
// Trend detector with EMA + linear regression slope.
```

`crates/prism-transport/src/quality/prober.rs`:
```rust
// Connection prober with adaptive frequency.
```

`crates/prism-transport/src/quality/mtu.rs`:
```rust
// MTU tracker with periodic recheck.
```

`crates/prism-transport/src/quic/mod.rs`:
```rust
pub mod config;
pub mod socket;
pub mod auth_token;
pub mod connection;
```

`crates/prism-transport/src/quic/config.rs`:
```rust
// QUIC transport configuration profiles.
```

`crates/prism-transport/src/quic/socket.rs`:
```rust
// UDP socket creation with DSCP and buffer sizing.
```

`crates/prism-transport/src/quic/auth_token.rs`:
```rust
// HMAC session token for throughput connection binding.
```

`crates/prism-transport/src/quic/connection.rs`:
```rust
// QuicConnection: PrismConnection impl over quinn::Connection.
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p prism-transport`
Expected: compiles with no errors (warnings OK for unused items)

- [ ] **Step 6: Commit**

```bash
git add crates/prism-transport/ Cargo.toml
git commit -m "chore: scaffold prism-transport crate with module stubs"
```

---

## Task 2: TransportError + TransportType + StreamPriority

**Files:**
- Modify: `crates/prism-transport/src/connection.rs`
- Modify: `crates/prism-transport/src/lib.rs`

- [ ] **Step 1: Write failing tests for TransportError**

Add to `crates/prism-transport/src/connection.rs`:

```rust
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
        // Higher priority = higher enum value, matching prism_protocol::ChannelPriority
        assert!(StreamPriority::Critical > StreamPriority::High);
        assert!(StreamPriority::High > StreamPriority::Normal);
        assert!(StreamPriority::Normal > StreamPriority::Low);
        assert!(StreamPriority::Low > StreamPriority::Background);
    }

    #[test]
    fn stream_priority_to_quinn_maps_correctly() {
        // Quinn: lower i32 = higher priority
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport`
Expected: FAIL — `TransportError`, `TransportType`, `StreamPriority` not defined

- [ ] **Step 3: Implement TransportError, TransportType, StreamPriority**

Add to the top of `crates/prism-transport/src/connection.rs`:

```rust
use thiserror::Error;
use prism_protocol::channel::ChannelPriority;

/// Transport-layer errors.
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

/// Underlying transport protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportType {
    Quic,
    WebSocket,
    Tcp,
}

/// Stream priority. Higher enum value = higher priority.
/// Matches `prism_protocol::ChannelPriority` ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StreamPriority {
    Background = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

impl StreamPriority {
    /// Convert to quinn's priority scale (lower i32 = higher priority).
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
```

- [ ] **Step 4: Update lib.rs re-exports**

```rust
pub mod connection;
pub mod framing;
pub mod unified;
pub mod coalesce;
pub mod stream_pool;
pub mod quality;
pub mod quic;

pub use connection::{
    TransportError, TransportType, StreamPriority,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p prism-transport`
Expected: 7 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/prism-transport/src/connection.rs crates/prism-transport/src/lib.rs
git commit -m "feat(transport): TransportError, TransportType, StreamPriority"
```

---

## Task 3: TransportMetrics + DelayAsymmetry + TransportEvent

**Files:**
- Modify: `crates/prism-transport/src/connection.rs`
- Modify: `crates/prism-transport/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module in `connection.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport`
Expected: FAIL — `DelayAsymmetry`, `TransportMetrics`, `TransportEvent` not defined

- [ ] **Step 3: Implement DelayAsymmetry, TransportMetrics, TransportEvent**

Add below the `StreamPriority` impl block in `connection.rs`:

```rust
use std::net::SocketAddr;

/// One-way delay asymmetry classification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelayAsymmetry {
    Unknown,
    Symmetric,
    DownstreamSlow { ratio: f32 },
    UpstreamSlow { ratio: f32 },
}

/// Snapshot of transport-level metrics.
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

/// Events emitted by a transport connection.
#[derive(Debug, Clone)]
pub enum TransportEvent {
    Connected { transport_type: TransportType, remote_addr: SocketAddr },
    Migrated { old_addr: SocketAddr, new_addr: SocketAddr },
    MetricsUpdated(TransportMetrics),
    Degraded { reason: String },
    Upgraded { from: TransportType, to: TransportType },
    Disconnected { reason: String },
}
```

- [ ] **Step 4: Update lib.rs re-exports**

```rust
pub use connection::{
    TransportError, TransportType, StreamPriority,
    TransportMetrics, DelayAsymmetry, TransportEvent,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p prism-transport`
Expected: 11 tests pass (7 from Task 2 + 4 new)

- [ ] **Step 6: Commit**

```bash
git add crates/prism-transport/src/connection.rs crates/prism-transport/src/lib.rs
git commit -m "feat(transport): TransportMetrics, DelayAsymmetry, TransportEvent"
```

---

## Task 4: OwnedSendStream + OwnedRecvStream + PrismConnection Trait + MockConnection

**Files:**
- Modify: `crates/prism-transport/src/connection.rs`
- Modify: `crates/prism-transport/src/lib.rs`

- [ ] **Step 1: Write failing tests for mock stream roundtrip**

Append to the `tests` module in `connection.rs`:

```rust
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
        // No panic = success; stream consumed
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
        conn.try_send_datagram(bytes::Bytes::from_static(b"hello")).unwrap();
        assert_eq!(conn.sent_datagrams().len(), 1);
    }

    #[tokio::test]
    async fn mock_connection_datagram_too_large() {
        let conn = mock::MockConnection::new(4);
        let result = conn.try_send_datagram(bytes::Bytes::from_static(b"toolarge"));
        assert!(matches!(result, Err(TransportError::DatagramTooLarge { .. })));
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport`
Expected: FAIL — `OwnedSendStream`, `OwnedRecvStream`, `PrismConnection`, `mock` not defined

- [ ] **Step 3: Implement stream inner enums and OwnedSendStream**

Add to `connection.rs` (below TransportEvent, above tests):

```rust
use bytes::Bytes;
use async_trait::async_trait;
use std::sync::{Arc, Mutex as StdMutex, atomic::{AtomicBool, Ordering}};
use tokio::sync::broadcast;

// --- Stream inner types ---

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

// --- OwnedSendStream ---

/// Owned send stream. Caller writes and closes. Zero contention.
pub struct OwnedSendStream {
    inner: SendStreamInner,
}

impl OwnedSendStream {
    /// Wrap a quinn send stream.
    pub(crate) fn from_quic(stream: quinn::SendStream) -> Self {
        Self { inner: SendStreamInner::Quic(stream) }
    }

    /// Write data to the stream.
    pub async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        match &mut self.inner {
            SendStreamInner::Quic(s) => {
                s.write_all(data).await.map_err(|e| TransportError::StreamError(e.to_string()))
            }
            #[cfg(test)]
            SendStreamInner::Mock { buffer, .. } => {
                buffer.lock().unwrap().extend_from_slice(data);
                Ok(())
            }
        }
    }

    /// Set stream priority (maps to quinn stream priority).
    pub fn set_priority(&mut self, priority: StreamPriority) -> Result<(), TransportError> {
        match &mut self.inner {
            SendStreamInner::Quic(s) => {
                s.set_priority(priority.to_quinn_priority())
                    .map_err(|e| TransportError::StreamError(e.to_string()))
            }
            #[cfg(test)]
            SendStreamInner::Mock { .. } => Ok(()),
        }
    }

    /// Finish the stream (send FIN). Consumes the stream.
    pub async fn finish(self) -> Result<(), TransportError> {
        match self.inner {
            SendStreamInner::Quic(mut s) => {
                s.finish().map_err(|e| TransportError::StreamError(e.to_string()))
            }
            #[cfg(test)]
            SendStreamInner::Mock { finished, .. } => {
                finished.store(true, Ordering::Release);
                Ok(())
            }
        }
    }

    /// Create a mock send stream (test only). Returns the stream and a handle to written bytes.
    #[cfg(test)]
    pub(crate) fn mock() -> (Self, Arc<StdMutex<Vec<u8>>>) {
        let buffer = Arc::new(StdMutex::new(Vec::new()));
        let finished = Arc::new(AtomicBool::new(false));
        (
            Self { inner: SendStreamInner::Mock { buffer: buffer.clone(), finished } },
            buffer,
        )
    }
}

// --- OwnedRecvStream ---

/// Owned receive stream. Caller reads. Zero contention.
pub struct OwnedRecvStream {
    inner: RecvStreamInner,
}

impl OwnedRecvStream {
    /// Wrap a quinn recv stream.
    pub(crate) fn from_quic(stream: quinn::RecvStream) -> Self {
        Self { inner: RecvStreamInner::Quic(stream) }
    }

    /// Read exactly `buf.len()` bytes.
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), TransportError> {
        match &mut self.inner {
            RecvStreamInner::Quic(s) => {
                s.read_exact(buf).await.map_err(|e| TransportError::StreamError(e.to_string()))
            }
            #[cfg(test)]
            RecvStreamInner::Mock { cursor } => {
                use std::io::Read;
                cursor.lock().unwrap().read_exact(buf)
                    .map_err(|e| TransportError::StreamError(e.to_string()))
            }
        }
    }

    /// Read all remaining bytes up to `limit`.
    pub async fn read_to_end(self, limit: usize) -> Result<Vec<u8>, TransportError> {
        match self.inner {
            RecvStreamInner::Quic(mut s) => {
                s.read_to_end(limit).await.map_err(|e| TransportError::StreamError(e.to_string()))
            }
            #[cfg(test)]
            RecvStreamInner::Mock { cursor } => {
                use std::io::Read;
                let mut guard = cursor.lock().unwrap();
                let pos = guard.position() as usize;
                let remaining = guard.get_ref().len() - pos;
                if remaining > limit {
                    return Err(TransportError::MessageTooLarge(remaining));
                }
                let mut data = vec![0u8; remaining];
                guard.read_exact(&mut data)
                    .map_err(|e| TransportError::StreamError(e.to_string()))?;
                Ok(data)
            }
        }
    }

    /// Create a mock recv stream pre-loaded with data (test only).
    #[cfg(test)]
    pub(crate) fn mock(data: Vec<u8>) -> Self {
        Self {
            inner: RecvStreamInner::Mock {
                cursor: StdMutex::new(std::io::Cursor::new(data)),
            },
        }
    }
}
```

- [ ] **Step 4: Implement PrismConnection trait**

Add below OwnedRecvStream:

```rust
/// A live transport connection. Abstracts over QUIC, WebSocket, etc.
#[async_trait]
pub trait PrismConnection: Send + Sync {
    /// Non-blocking datagram send. Returns WouldBlock if buffer full.
    fn try_send_datagram(&self, data: Bytes) -> Result<(), TransportError>;

    /// Datagram send with overflow-to-stream fallback.
    async fn send_datagram(&self, data: Bytes) -> Result<(), TransportError>;

    /// Receive next datagram.
    async fn recv_datagram(&self) -> Result<Bytes, TransportError>;

    /// Open a bidirectional stream. Caller owns the returned streams.
    async fn open_bi(&self, priority: StreamPriority)
        -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>;

    /// Open a unidirectional send stream.
    async fn open_uni(&self, priority: StreamPriority)
        -> Result<OwnedSendStream, TransportError>;

    /// Accept an incoming bidirectional stream from the peer.
    async fn accept_bi(&self)
        -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>;

    /// Accept an incoming unidirectional stream from the peer.
    async fn accept_uni(&self) -> Result<OwnedRecvStream, TransportError>;

    /// Current transport metrics snapshot.
    fn metrics(&self) -> TransportMetrics;

    /// Underlying transport protocol.
    fn transport_type(&self) -> TransportType;

    /// Maximum datagram payload size.
    fn max_datagram_size(&self) -> usize;

    /// Subscribe to transport events.
    fn events(&self) -> broadcast::Receiver<TransportEvent>;

    /// Close the connection.
    async fn close(&self);
}
```

- [ ] **Step 5: Implement MockConnection**

Add below the PrismConnection trait:

```rust
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

        pub fn enqueue_datagram(&self, data: Bytes) {
            tokio::task::block_in_place(|| {
                // For test setup, push directly
            });
            // Simple: use std mutex for the queue in tests
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
            self.recv_queue.lock().await.pop_front()
                .ok_or(TransportError::ConnectionClosed)
        }

        async fn open_bi(&self, _priority: StreamPriority)
            -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>
        {
            let (send, _buf) = OwnedSendStream::mock();
            let recv = OwnedRecvStream::mock(vec![]);
            Ok((send, recv))
        }

        async fn open_uni(&self, _priority: StreamPriority)
            -> Result<OwnedSendStream, TransportError>
        {
            let (send, _buf) = OwnedSendStream::mock();
            Ok(send)
        }

        async fn accept_bi(&self)
            -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>
        {
            // Mock: never accepts — would hang forever. Tests shouldn't call this.
            std::future::pending().await
        }

        async fn accept_uni(&self) -> Result<OwnedRecvStream, TransportError> {
            std::future::pending().await
        }

        fn metrics(&self) -> TransportMetrics { TransportMetrics::default() }
        fn transport_type(&self) -> TransportType { TransportType::Quic }
        fn max_datagram_size(&self) -> usize { self.max_datagram_size }
        fn events(&self) -> broadcast::Receiver<TransportEvent> { self.event_tx.subscribe() }
        async fn close(&self) {}
    }
}
```

- [ ] **Step 6: Update lib.rs re-exports**

```rust
pub use connection::{
    TransportError, TransportType, StreamPriority,
    TransportMetrics, DelayAsymmetry, TransportEvent,
    OwnedSendStream, OwnedRecvStream, PrismConnection,
};
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p prism-transport`
Expected: 19 tests pass (11 previous + 8 new)

- [ ] **Step 8: Commit**

```bash
git add crates/prism-transport/src/connection.rs crates/prism-transport/src/lib.rs
git commit -m "feat(transport): PrismConnection trait, OwnedSendStream/RecvStream, MockConnection"
```

---

## Task 5: FramedWriter + FramedReader

**Files:**
- Modify: `crates/prism-transport/src/framing.rs`
- Modify: `crates/prism-transport/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Replace `crates/prism-transport/src/framing.rs`:

```rust
use crate::connection::{OwnedSendStream, OwnedRecvStream, TransportError};

/// Maximum message size for framed streams (16 MB).
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

// Implementation goes here after tests

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn framed_roundtrip_simple() {
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(b"hello world").await.unwrap();

        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        let msg = reader.recv().await.unwrap();
        assert_eq!(msg, b"hello world");
    }

    #[tokio::test]
    async fn framed_roundtrip_empty_message() {
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(b"").await.unwrap();

        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        let msg = reader.recv().await.unwrap();
        assert!(msg.is_empty());
    }

    #[tokio::test]
    async fn framed_roundtrip_multiple_messages() {
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(b"one").await.unwrap();
        writer.send(b"two").await.unwrap();
        writer.send(b"three").await.unwrap();

        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        assert_eq!(reader.recv().await.unwrap(), b"one");
        assert_eq!(reader.recv().await.unwrap(), b"two");
        assert_eq!(reader.recv().await.unwrap(), b"three");
    }

    #[tokio::test]
    async fn framed_reader_rejects_oversized_message() {
        // Craft a length prefix claiming 20MB
        let mut data = Vec::new();
        data.extend_from_slice(&(20_000_000u32).to_le_bytes());
        data.extend_from_slice(b"x"); // doesn't matter, rejected by length check

        let recv = OwnedRecvStream::mock(data);
        let mut reader = FramedReader::new(recv);
        let result = reader.recv().await;
        assert!(matches!(result, Err(TransportError::MessageTooLarge(20_000_000))));
    }

    #[tokio::test]
    async fn framed_roundtrip_binary_data() {
        let payload: Vec<u8> = (0..=255).collect();
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(&payload).await.unwrap();

        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        assert_eq!(reader.recv().await.unwrap(), payload);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport -- framed`
Expected: FAIL — `FramedWriter`, `FramedReader` not defined

- [ ] **Step 3: Implement FramedWriter and FramedReader**

Add above the `#[cfg(test)]` block in `framing.rs`:

```rust
/// Length-prefixed message writer for long-lived streams.
pub struct FramedWriter {
    stream: OwnedSendStream,
}

impl FramedWriter {
    pub fn new(stream: OwnedSendStream) -> Self {
        Self { stream }
    }

    /// Send a length-prefixed message.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.stream.write(&(data.len() as u32).to_le_bytes()).await?;
        self.stream.write(data).await?;
        Ok(())
    }

    /// Consume the writer, returning the underlying stream.
    pub fn into_inner(self) -> OwnedSendStream {
        self.stream
    }
}

/// Length-prefixed message reader for long-lived streams.
pub struct FramedReader {
    stream: OwnedRecvStream,
}

impl FramedReader {
    pub fn new(stream: OwnedRecvStream) -> Self {
        Self { stream }
    }

    /// Receive a length-prefixed message.
    pub async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge(len));
        }
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await?;
        Ok(data)
    }

    /// Consume the reader, returning the underlying stream.
    pub fn into_inner(self) -> OwnedRecvStream {
        self.stream
    }
}
```

- [ ] **Step 4: Update lib.rs re-exports**

Add:

```rust
pub use framing::{FramedWriter, FramedReader, MAX_MESSAGE_SIZE};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p prism-transport -- framed`
Expected: 5 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/prism-transport/src/framing.rs crates/prism-transport/src/lib.rs
git commit -m "feat(transport): FramedWriter/FramedReader with length-prefixed framing"
```

---

## Task 6: UnifiedConnection + ChannelRouting

**Files:**
- Modify: `crates/prism-transport/src/unified.rs`
- Modify: `crates/prism-transport/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Replace `crates/prism-transport/src/unified.rs`:

```rust
use crate::connection::{PrismConnection, TransportType};
use prism_protocol::channel::{
    self, CHANNEL_DISPLAY, CHANNEL_INPUT, CHANNEL_CONTROL, CHANNEL_FILESHARE,
    CHANNEL_SENSOR, CHANNEL_AUDIO, CHANNEL_CLIPBOARD, CHANNEL_DEVICE,
};

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::mock::MockConnection;

    #[test]
    fn default_routing_critical_to_latency() {
        let routing = ChannelRouting::default();
        assert_eq!(routing.slot_for_channel(CHANNEL_INPUT), ConnectionSlot::Latency);
    }

    #[test]
    fn default_routing_high_to_latency() {
        let routing = ChannelRouting::default();
        assert_eq!(routing.slot_for_channel(CHANNEL_DISPLAY), ConnectionSlot::Latency);
        assert_eq!(routing.slot_for_channel(CHANNEL_AUDIO), ConnectionSlot::Latency);
    }

    #[test]
    fn default_routing_normal_to_latency() {
        let routing = ChannelRouting::default();
        assert_eq!(routing.slot_for_channel(CHANNEL_CONTROL), ConnectionSlot::Latency);
        assert_eq!(routing.slot_for_channel(CHANNEL_CLIPBOARD), ConnectionSlot::Latency);
    }

    #[test]
    fn default_routing_low_to_throughput() {
        let routing = ChannelRouting::default();
        assert_eq!(routing.slot_for_channel(CHANNEL_FILESHARE), ConnectionSlot::Throughput);
        assert_eq!(routing.slot_for_channel(CHANNEL_DEVICE), ConnectionSlot::Throughput);
    }

    #[test]
    fn default_routing_background_to_throughput() {
        let routing = ChannelRouting::default();
        assert_eq!(routing.slot_for_channel(CHANNEL_SENSOR), ConnectionSlot::Throughput);
    }

    #[test]
    fn unified_for_channel_routes_display_to_latency() {
        let latency = MockConnection::new(1200);
        let throughput = MockConnection::new(1200);
        let unified = UnifiedConnection::new(
            Box::new(latency),
            Some(Box::new(throughput)),
        );
        let conn = unified.for_channel(CHANNEL_DISPLAY);
        // Display is High priority → latency connection
        assert_eq!(conn.transport_type(), TransportType::Quic);
    }

    #[test]
    fn unified_single_connection_fallback() {
        let latency = MockConnection::new(1200);
        let unified = UnifiedConnection::new(Box::new(latency), None);
        // Even Low priority channels route to latency when throughput is None
        let conn = unified.for_channel(CHANNEL_FILESHARE);
        assert_eq!(conn.transport_type(), TransportType::Quic);
    }

    #[test]
    fn unified_latency_accessor() {
        let latency = MockConnection::new(1200);
        let unified = UnifiedConnection::new(Box::new(latency), None);
        assert_eq!(unified.latency().max_datagram_size(), 1200);
    }

    #[test]
    fn unified_throughput_falls_back_to_latency() {
        let latency = MockConnection::new(1200);
        let unified = UnifiedConnection::new(Box::new(latency), None);
        // throughput() returns latency when no throughput connection
        assert_eq!(unified.throughput().max_datagram_size(), 1200);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport -- unified`
Expected: FAIL — `ChannelRouting`, `ConnectionSlot`, `UnifiedConnection` not defined

- [ ] **Step 3: Implement ChannelRouting and UnifiedConnection**

Add above the `#[cfg(test)]` block in `unified.rs`:

```rust
/// Which underlying connection a channel should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionSlot {
    Latency,
    Throughput,
}

/// Maps channel priority categories to connection slots.
pub struct ChannelRouting {
    /// Indexed by `priority_category(channel_id)` → 0..=4
    routes: [ConnectionSlot; 5],
}

impl ChannelRouting {
    /// Route a channel to its connection slot.
    pub fn slot_for_channel(&self, channel_id: u16) -> ConnectionSlot {
        let category = channel::priority_category(channel_id);
        self.routes[category]
    }
}

impl Default for ChannelRouting {
    fn default() -> Self {
        Self {
            routes: [
                ConnectionSlot::Throughput,  // Background (0): Sensor, Notify
                ConnectionSlot::Throughput,  // Low (1): FileShare, Device
                ConnectionSlot::Latency,     // Normal (2): Control, Clipboard
                ConnectionSlot::Latency,     // High (3): Display, Audio
                ConnectionSlot::Latency,     // Critical (4): Input
            ],
        }
    }
}

/// Dual-connection facade. Routes channels to the appropriate connection
/// based on priority. Falls back to latency if throughput is unavailable.
pub struct UnifiedConnection {
    latency: Box<dyn PrismConnection>,
    throughput: Option<Box<dyn PrismConnection>>,
    routing: ChannelRouting,
}

impl UnifiedConnection {
    pub fn new(
        latency: Box<dyn PrismConnection>,
        throughput: Option<Box<dyn PrismConnection>>,
    ) -> Self {
        Self {
            latency,
            throughput,
            routing: ChannelRouting::default(),
        }
    }

    /// Get the connection for a given channel ID.
    pub fn for_channel(&self, channel_id: u16) -> &dyn PrismConnection {
        match self.routing.slot_for_channel(channel_id) {
            ConnectionSlot::Latency => &*self.latency,
            ConnectionSlot::Throughput => {
                self.throughput.as_deref().unwrap_or(&*self.latency)
            }
        }
    }

    /// Direct access to the latency connection.
    pub fn latency(&self) -> &dyn PrismConnection {
        &*self.latency
    }

    /// Access the throughput connection, falling back to latency.
    pub fn throughput(&self) -> &dyn PrismConnection {
        self.throughput.as_deref().unwrap_or(&*self.latency)
    }
}
```

- [ ] **Step 4: Update lib.rs re-exports**

Add:

```rust
pub use unified::{UnifiedConnection, ChannelRouting, ConnectionSlot};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p prism-transport -- unified`
Expected: 9 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/prism-transport/src/unified.rs crates/prism-transport/src/lib.rs
git commit -m "feat(transport): UnifiedConnection dual-connection facade with ChannelRouting"
```

---

## Task 7: BandwidthEstimator

**Files:**
- Modify: `crates/prism-transport/src/quality/bandwidth.rs`

- [ ] **Step 1: Write failing tests**

Replace `crates/prism-transport/src/quality/bandwidth.rs`:

```rust
use std::time::{Duration, Instant};
use std::collections::VecDeque;

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_estimator_returns_zero() {
        let est = BandwidthEstimator::new();
        assert_eq!(est.send_bps(), 0);
        assert_eq!(est.recv_bps(), 0);
    }

    #[test]
    fn single_sample_returns_zero() {
        let mut est = BandwidthEstimator::new();
        est.record_send(1000);
        assert_eq!(est.send_bps(), 0); // Need at least 2 samples for a rate
    }

    #[test]
    fn two_samples_compute_bandwidth() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_send_at(0, now);
        est.record_send_at(125_000, now + Duration::from_secs(1)); // 125KB in 1s = 1Mbps
        let bps = est.send_bps();
        assert_eq!(bps, 1_000_000); // 125_000 * 8 / 1s
    }

    #[test]
    fn recv_bandwidth_tracked_separately() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_recv_at(0, now);
        est.record_recv_at(250_000, now + Duration::from_secs(1)); // 250KB/s = 2Mbps
        assert_eq!(est.recv_bps(), 2_000_000);
        assert_eq!(est.send_bps(), 0); // no send samples
    }

    #[test]
    fn old_samples_expire() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_send_at(0, now);
        est.record_send_at(125_000, now + Duration::from_secs(1));
        // Jump 10 seconds — beyond 5s window
        est.record_send_at(125_000, now + Duration::from_secs(10));
        est.record_send_at(250_000, now + Duration::from_secs(11));
        // Only the last two samples remain: 125KB in 1s = 1Mbps
        let bps = est.send_bps();
        assert_eq!(bps, 1_000_000);
    }

    #[test]
    fn multiple_samples_average() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        // 500KB over 2 seconds = 2Mbps
        est.record_send_at(0, now);
        est.record_send_at(250_000, now + Duration::from_secs(1));
        est.record_send_at(500_000, now + Duration::from_secs(2));
        let bps = est.send_bps();
        assert_eq!(bps, 2_000_000); // 500_000 * 8 / 2s
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport -- bandwidth`
Expected: FAIL — `BandwidthEstimator` not defined

- [ ] **Step 3: Implement BandwidthEstimator**

Add above the `#[cfg(test)]` block:

```rust
/// Sliding window bandwidth estimator.
/// Tracks (timestamp, cumulative_bytes) samples over a 5-second window.
pub struct BandwidthEstimator {
    send_samples: VecDeque<(Instant, u64)>,
    recv_samples: VecDeque<(Instant, u64)>,
    window: Duration,
}

/// Trim old samples from a window, keeping at least one.
fn trim_samples(samples: &mut VecDeque<(Instant, u64)>, window: Duration, now: Instant) {
    while let Some((t, _)) = samples.front() {
        if now.duration_since(*t) > window && samples.len() > 1 {
            samples.pop_front();
        } else {
            break;
        }
    }
}

impl BandwidthEstimator {
    pub fn new() -> Self {
        Self {
            send_samples: VecDeque::new(),
            recv_samples: VecDeque::new(),
            window: Duration::from_secs(5),
        }
    }

    /// Record cumulative bytes sent.
    pub fn record_send(&mut self, cumulative_bytes: u64) {
        self.record_send_at(cumulative_bytes, Instant::now());
    }

    /// Record cumulative bytes sent at a specific time (for testing).
    pub fn record_send_at(&mut self, cumulative_bytes: u64, at: Instant) {
        self.send_samples.push_back((at, cumulative_bytes));
        trim_samples(&mut self.send_samples, self.window, at);
    }

    /// Record cumulative bytes received.
    pub fn record_recv(&mut self, cumulative_bytes: u64) {
        self.record_recv_at(cumulative_bytes, Instant::now());
    }

    /// Record cumulative bytes received at a specific time (for testing).
    pub fn record_recv_at(&mut self, cumulative_bytes: u64, at: Instant) {
        self.recv_samples.push_back((at, cumulative_bytes));
        trim_samples(&mut self.recv_samples, self.window, at);
    }

    /// Current send bandwidth in bits per second.
    pub fn send_bps(&self) -> u64 {
        Self::compute_bps(&self.send_samples)
    }

    /// Current receive bandwidth in bits per second.
    pub fn recv_bps(&self) -> u64 {
        Self::compute_bps(&self.recv_samples)
    }

    fn compute_bps(samples: &VecDeque<(Instant, u64)>) -> u64 {
        if samples.len() < 2 {
            return 0;
        }
        let (first_time, first_bytes) = samples.front().unwrap();
        let (last_time, last_bytes) = samples.back().unwrap();
        let elapsed = last_time.duration_since(*first_time);
        if elapsed.is_zero() {
            return 0;
        }
        let bytes_delta = last_bytes.saturating_sub(*first_bytes);
        let bits = bytes_delta * 8;
        (bits as f64 / elapsed.as_secs_f64()) as u64
    }
}

impl Default for BandwidthEstimator {
    fn default() -> Self { Self::new() }
}

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p prism-transport -- bandwidth`
Expected: 6 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/prism-transport/src/quality/bandwidth.rs
git commit -m "feat(transport): BandwidthEstimator with sliding window measurement"
```

---

## Task 8: OneWayDelayEstimator

**Files:**
- Modify: `crates/prism-transport/src/quality/one_way_delay.rs`

- [ ] **Step 1: Write failing tests**

Replace `crates/prism-transport/src/quality/one_way_delay.rs`:

```rust
use crate::connection::DelayAsymmetry;

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_estimator_returns_unknown() {
        let est = OneWayDelayEstimator::new();
        assert_eq!(est.asymmetry(), DelayAsymmetry::Unknown);
        assert_eq!(est.downstream_delay_us(), None);
        assert_eq!(est.upstream_delay_us(), None);
    }

    #[test]
    fn first_sample_sets_baseline() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000); // offset = 100
        assert_eq!(est.downstream_delay_us(), Some(0)); // first sample = baseline
    }

    #[test]
    fn increasing_offset_shows_delay() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000); // offset = 100 (baseline)
        est.record_downstream(1_000_250, 1_000_100); // offset = 150, delay = 50
        assert_eq!(est.downstream_delay_us(), Some(50));
    }

    #[test]
    fn lower_offset_resets_baseline() {
        let mut est = OneWayDelayEstimator::new();
        est.record_downstream(1_000_100, 1_000_000); // offset = 100 (baseline)
        est.record_downstream(1_000_250, 1_000_100); // offset = 150, delay = 50
        est.record_downstream(1_000_050, 1_000_000); // offset = 50 (new baseline)
        assert_eq!(est.downstream_delay_us(), Some(0)); // reset to 0
    }

    #[test]
    fn symmetric_when_delays_similar() {
        let mut est = OneWayDelayEstimator::new();
        // Downstream: offset 100 (baseline), then 100 (no change) → delay 0
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_200, 1_000_100);
        // Upstream: same pattern → delay 0
        est.record_upstream(2_000_100, 2_000_000);
        est.record_upstream(2_000_200, 2_000_100);
        assert_eq!(est.asymmetry(), DelayAsymmetry::Symmetric);
    }

    #[test]
    fn downstream_slow_detected() {
        let mut est = OneWayDelayEstimator::new();
        // Downstream: offset 100, then 600 → delay 500
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_700, 1_000_100);
        // Upstream: offset 100, then 150 → delay 50
        est.record_upstream(2_000_100, 2_000_000);
        est.record_upstream(2_000_250, 2_000_100);
        match est.asymmetry() {
            DelayAsymmetry::DownstreamSlow { ratio } => {
                assert!(ratio > 2.0, "downstream should be significantly slower");
            }
            other => panic!("expected DownstreamSlow, got {:?}", other),
        }
    }

    #[test]
    fn upstream_slow_detected() {
        let mut est = OneWayDelayEstimator::new();
        // Downstream: small delay
        est.record_downstream(1_000_100, 1_000_000);
        est.record_downstream(1_000_150, 1_000_050);
        // Upstream: large delay
        est.record_upstream(2_000_100, 2_000_000);
        est.record_upstream(2_000_700, 2_000_100);
        match est.asymmetry() {
            DelayAsymmetry::UpstreamSlow { ratio } => {
                assert!(ratio > 2.0, "upstream should be significantly slower");
            }
            other => panic!("expected UpstreamSlow, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport -- one_way_delay`
Expected: FAIL — `OneWayDelayEstimator` not defined

- [ ] **Step 3: Implement OneWayDelayEstimator**

Add above the `#[cfg(test)]` block:

```rust
/// Tracks one-way delay in a single direction using min-offset technique.
struct DirectionEstimator {
    min_offset_us: Option<i64>,
    current_delay_us: i64,
}

impl DirectionEstimator {
    fn new() -> Self {
        Self { min_offset_us: None, current_delay_us: 0 }
    }

    fn update(&mut self, offset: i64) {
        match self.min_offset_us {
            None => {
                self.min_offset_us = Some(offset);
                self.current_delay_us = 0;
            }
            Some(min) => {
                if offset < min {
                    self.min_offset_us = Some(offset);
                }
                self.current_delay_us = offset - self.min_offset_us.unwrap();
            }
        }
    }

    fn delay_us(&self) -> Option<i64> {
        self.min_offset_us.map(|_| self.current_delay_us)
    }
}

/// One-way delay estimator using min-offset tracking per direction.
/// Doesn't need clock synchronization — measures changes relative to observed minimum.
pub struct OneWayDelayEstimator {
    downstream: DirectionEstimator,
    upstream: DirectionEstimator,
}

impl OneWayDelayEstimator {
    pub fn new() -> Self {
        Self {
            downstream: DirectionEstimator::new(),
            upstream: DirectionEstimator::new(),
        }
    }

    /// Record a downstream measurement (server → client).
    /// `local_recv_us`: local timestamp when packet was received.
    /// `remote_send_us`: remote timestamp when packet was sent.
    pub fn record_downstream(&mut self, local_recv_us: u64, remote_send_us: u64) {
        let offset = local_recv_us as i64 - remote_send_us as i64;
        self.downstream.update(offset);
    }

    /// Record an upstream measurement (client → server).
    /// `remote_recv_us`: remote timestamp when packet was received.
    /// `local_send_us`: local timestamp when packet was sent.
    pub fn record_upstream(&mut self, remote_recv_us: u64, local_send_us: u64) {
        let offset = remote_recv_us as i64 - local_send_us as i64;
        self.upstream.update(offset);
    }

    pub fn downstream_delay_us(&self) -> Option<i64> {
        self.downstream.delay_us()
    }

    pub fn upstream_delay_us(&self) -> Option<i64> {
        self.upstream.delay_us()
    }

    /// Classify the delay asymmetry between directions.
    pub fn asymmetry(&self) -> DelayAsymmetry {
        let down = match self.downstream.delay_us() {
            Some(d) => d,
            None => return DelayAsymmetry::Unknown,
        };
        let up = match self.upstream.delay_us() {
            Some(u) => u,
            None => return DelayAsymmetry::Unknown,
        };

        let threshold = 100; // microseconds: below this, consider symmetric
        if down <= threshold && up <= threshold {
            return DelayAsymmetry::Symmetric;
        }

        if down == 0 && up == 0 {
            return DelayAsymmetry::Symmetric;
        }

        let max = down.max(up) as f32;
        let min = down.min(up).max(1) as f32; // avoid division by zero
        let ratio = max / min;

        if ratio < 2.0 {
            DelayAsymmetry::Symmetric
        } else if down > up {
            DelayAsymmetry::DownstreamSlow { ratio }
        } else {
            DelayAsymmetry::UpstreamSlow { ratio }
        }
    }
}

impl Default for OneWayDelayEstimator {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p prism-transport -- one_way_delay`
Expected: 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/prism-transport/src/quality/one_way_delay.rs
git commit -m "feat(transport): OneWayDelayEstimator with min-offset asymmetry detection"
```

---

## Task 9: TrendDetector

**Files:**
- Modify: `crates/prism-transport/src/quality/trend.rs`

- [ ] **Step 1: Write failing tests**

Replace `crates/prism-transport/src/quality/trend.rs`:

```rust
use std::collections::VecDeque;

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_detector_is_stable() {
        let det = TrendDetector::new();
        assert_eq!(det.trend(), Trend::Stable);
    }

    #[test]
    fn stable_constant_input() {
        let mut det = TrendDetector::new();
        for _ in 0..60 {
            det.record(10.0);
        }
        assert_eq!(det.trend(), Trend::Stable);
    }

    #[test]
    fn degrading_sharp_increase() {
        let mut det = TrendDetector::new();
        // Establish baseline at 10
        for _ in 0..30 {
            det.record(10.0);
        }
        // Sharp jump to 20 (short EMA jumps, long EMA lags)
        for _ in 0..10 {
            det.record(20.0);
        }
        assert_eq!(det.trend(), Trend::Degrading);
    }

    #[test]
    fn improving_sharp_decrease() {
        let mut det = TrendDetector::new();
        // Establish baseline at 20
        for _ in 0..30 {
            det.record(20.0);
        }
        // Sharp drop to 5 (short EMA drops, long EMA lags)
        for _ in 0..10 {
            det.record(5.0);
        }
        assert_eq!(det.trend(), Trend::Improving);
    }

    #[test]
    fn slowly_degrading_gradual_increase() {
        let mut det = TrendDetector::new();
        // Gradual increase: not enough for EMA divergence but positive slope
        for i in 0..60 {
            det.record(10.0 + i as f64 * 0.5);
        }
        let trend = det.trend();
        // Should detect either Degrading or SlowlyDegrading
        assert!(
            trend == Trend::Degrading || trend == Trend::SlowlyDegrading,
            "expected degrading trend, got {:?}", trend
        );
    }

    #[test]
    fn short_and_long_ema_track_values() {
        let mut det = TrendDetector::new();
        for _ in 0..100 {
            det.record(42.0);
        }
        // After many samples, both EMAs converge to the input
        let (short, long) = det.ema_values();
        assert!((short - 42.0).abs() < 1.0);
        assert!((long - 42.0).abs() < 1.0);
    }

    #[test]
    fn slope_of_constant_is_near_zero() {
        let mut det = TrendDetector::new();
        for _ in 0..60 {
            det.record(10.0);
        }
        assert!(det.slope().abs() < 0.01);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport -- trend`
Expected: FAIL — `TrendDetector`, `Trend` not defined

- [ ] **Step 3: Implement TrendDetector**

Add above the `#[cfg(test)]` block:

```rust
/// Trend classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    /// No significant change.
    Stable,
    /// Short EMA 50%+ above long EMA — acute degradation.
    Degrading,
    /// Positive linear regression slope — gradual decline.
    SlowlyDegrading,
    /// Short EMA 30%+ below long EMA — recovering.
    Improving,
}

/// Trend detector using dual EMA (5s/30s) + linear regression slope over 60 samples.
pub struct TrendDetector {
    short_ema: f64,
    long_ema: f64,
    short_alpha: f64,
    long_alpha: f64,
    slope_window: VecDeque<f64>,
    slope_window_max: usize,
    initialized: bool,
}

impl TrendDetector {
    pub fn new() -> Self {
        Self {
            short_ema: 0.0,
            long_ema: 0.0,
            short_alpha: 2.0 / (5.0 + 1.0),   // ~0.333 for 5-sample EMA
            long_alpha: 2.0 / (30.0 + 1.0),    // ~0.0645 for 30-sample EMA
            slope_window: VecDeque::new(),
            slope_window_max: 60,
            initialized: false,
        }
    }

    /// Record a new quality metric sample (e.g., RTT, loss, etc.).
    pub fn record(&mut self, value: f64) {
        if !self.initialized {
            self.short_ema = value;
            self.long_ema = value;
            self.initialized = true;
        } else {
            self.short_ema = self.short_alpha * value + (1.0 - self.short_alpha) * self.short_ema;
            self.long_ema = self.long_alpha * value + (1.0 - self.long_alpha) * self.long_ema;
        }

        self.slope_window.push_back(value);
        if self.slope_window.len() > self.slope_window_max {
            self.slope_window.pop_front();
        }
    }

    /// Current trend classification.
    pub fn trend(&self) -> Trend {
        if !self.initialized {
            return Trend::Stable;
        }

        // Check EMA divergence
        if self.long_ema > 0.0 {
            let ratio = self.short_ema / self.long_ema;
            if ratio >= 1.5 {
                return Trend::Degrading;
            }
            if ratio <= 0.7 {
                return Trend::Improving;
            }
        }

        // Check slope
        let slope = self.slope();
        if slope > 0.1 && self.slope_window.len() >= 10 {
            return Trend::SlowlyDegrading;
        }

        Trend::Stable
    }

    /// Linear regression slope over the slope window.
    pub fn slope(&self) -> f64 {
        let n = self.slope_window.len() as f64;
        if n < 2.0 {
            return 0.0;
        }

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2 = 0.0;

        for (i, y) in self.slope_window.iter().enumerate() {
            let x = i as f64;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_x2 += x * x;
        }

        let denom = n * sum_x2 - sum_x * sum_x;
        if denom.abs() < f64::EPSILON {
            return 0.0;
        }

        (n * sum_xy - sum_x * sum_y) / denom
    }

    /// Current EMA values (short, long). Exposed for diagnostics.
    pub fn ema_values(&self) -> (f64, f64) {
        (self.short_ema, self.long_ema)
    }
}

impl Default for TrendDetector {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p prism-transport -- trend`
Expected: 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/prism-transport/src/quality/trend.rs
git commit -m "feat(transport): TrendDetector with EMA + linear regression slope"
```

---

## Task 10: ConnectionQuality + QualityRecommendation

**Files:**
- Modify: `crates/prism-transport/src/quality/mod.rs`

- [ ] **Step 1: Write failing tests**

Replace `crates/prism-transport/src/quality/mod.rs`:

```rust
pub mod bandwidth;
pub mod one_way_delay;
pub mod trend;
pub mod prober;
pub mod mtu;

use crate::connection::DelayAsymmetry;

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_quality_scores_1() {
        let q = ConnectionQuality::compute(
            1000,   // 1ms RTT
            200,    // 0.2ms jitter
            0.0,    // 0% loss
            100_000_000, // 100Mbps send
            100_000_000, // 100Mbps recv
            DelayAsymmetry::Symmetric,
        );
        assert!((q.score - 1.0).abs() < f32::EPSILON, "expected 1.0, got {}", q.score);
        assert_eq!(q.recommendation, QualityRecommendation::Optimal);
    }

    #[test]
    fn high_rtt_degrades_score() {
        let q = ConnectionQuality::compute(
            200_000, // 200ms RTT
            5000,    // 5ms jitter
            0.0,     // 0% loss
            50_000_000,
            50_000_000,
            DelayAsymmetry::Symmetric,
        );
        assert!(q.score < 0.5, "score should be low with 200ms RTT, got {}", q.score);
    }

    #[test]
    fn high_loss_degrades_score() {
        let q = ConnectionQuality::compute(
            5000,   // 5ms RTT
            500,    // 0.5ms jitter
            0.10,   // 10% loss
            50_000_000,
            50_000_000,
            DelayAsymmetry::Symmetric,
        );
        assert!(q.score < 0.5, "score should be low with 10% loss, got {}", q.score);
    }

    #[test]
    fn high_jitter_degrades_score() {
        let q = ConnectionQuality::compute(
            5000,    // 5ms RTT
            50_000,  // 50ms jitter
            0.0,     // 0% loss
            50_000_000,
            50_000_000,
            DelayAsymmetry::Symmetric,
        );
        assert!(q.score < 0.8, "score should degrade with 50ms jitter, got {}", q.score);
    }

    #[test]
    fn very_bad_quality_recommends_unusable() {
        let q = ConnectionQuality::compute(
            500_000,  // 500ms RTT
            100_000,  // 100ms jitter
            0.20,     // 20% loss
            1_000_000, // 1Mbps
            1_000_000,
            DelayAsymmetry::Symmetric,
        );
        assert_eq!(q.recommendation, QualityRecommendation::ConnectionUnusable);
    }

    #[test]
    fn moderate_loss_recommends_fec() {
        let q = ConnectionQuality::compute(
            10_000,  // 10ms RTT
            2000,    // 2ms jitter
            0.03,    // 3% loss
            50_000_000,
            50_000_000,
            DelayAsymmetry::Symmetric,
        );
        assert!(
            matches!(q.recommendation, QualityRecommendation::EnableFec { .. }),
            "expected FEC recommendation, got {:?}", q.recommendation
        );
    }

    #[test]
    fn score_is_bounded_0_to_1() {
        // Worst case
        let q = ConnectionQuality::compute(1_000_000, 1_000_000, 1.0, 0, 0, DelayAsymmetry::Unknown);
        assert!(q.score >= 0.0 && q.score <= 1.0, "score out of bounds: {}", q.score);

        // Best case
        let q = ConnectionQuality::compute(100, 10, 0.0, 1_000_000_000, 1_000_000_000, DelayAsymmetry::Symmetric);
        assert!(q.score >= 0.0 && q.score <= 1.0, "score out of bounds: {}", q.score);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport -- quality::tests`
Expected: FAIL — `ConnectionQuality`, `QualityRecommendation` not defined

- [ ] **Step 3: Implement ConnectionQuality and QualityRecommendation**

Add above the `#[cfg(test)]` block in `quality/mod.rs`:

```rust
/// Quality recommendation for the degradation ladder.
#[derive(Debug, Clone, PartialEq)]
pub enum QualityRecommendation {
    Optimal,
    ReduceBitrate { target_bps: u64 },
    ReduceResolution,
    ReduceFramerate,
    EnableFec { ratio: f32 },
    SwitchToStreamOnly,
    PauseNonEssential,
    ConnectionUnusable,
}

/// Sub-quality for RTT.
#[derive(Debug, Clone, Copy)]
pub struct ProbeQuality {
    pub rtt_us: u64,
    pub jitter_us: u64,
}

/// Sub-quality for bandwidth.
#[derive(Debug, Clone, Copy)]
pub struct BandwidthQuality {
    pub send_bps: u64,
    pub recv_bps: u64,
}

/// Sub-quality for loss.
#[derive(Debug, Clone, Copy)]
pub struct LossQuality {
    pub loss_rate: f32,
}

/// Composite connection quality score and recommendation.
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    pub rtt: ProbeQuality,
    pub bandwidth: BandwidthQuality,
    pub loss: LossQuality,
    pub asymmetry: DelayAsymmetry,
    pub score: f32,
    pub recommendation: QualityRecommendation,
}

impl ConnectionQuality {
    /// Compute composite quality from raw metrics.
    pub fn compute(
        rtt_us: u64,
        jitter_us: u64,
        loss_rate: f32,
        send_bps: u64,
        recv_bps: u64,
        asymmetry: DelayAsymmetry,
    ) -> Self {
        let rtt_score = Self::rtt_score(rtt_us);
        let loss_score = Self::loss_score(loss_rate);
        let jitter_score = Self::jitter_score(jitter_us);
        let score = rtt_score * 0.4 + loss_score * 0.35 + jitter_score * 0.25;

        let recommendation = Self::recommend(score, loss_rate, send_bps);

        Self {
            rtt: ProbeQuality { rtt_us, jitter_us },
            bandwidth: BandwidthQuality { send_bps, recv_bps },
            loss: LossQuality { loss_rate },
            asymmetry,
            score,
            recommendation,
        }
    }

    fn rtt_score(rtt_us: u64) -> f32 {
        match rtt_us {
            0..=5_000 => 1.0,
            5_001..=20_000 => 0.8,
            20_001..=50_000 => 0.6,
            50_001..=100_000 => 0.3,
            _ => 0.1,
        }
    }

    fn loss_score(loss_rate: f32) -> f32 {
        if loss_rate <= 0.001 { 1.0 }
        else if loss_rate <= 0.01 { 0.7 }
        else if loss_rate <= 0.05 { 0.4 }
        else { 0.1 }
    }

    fn jitter_score(jitter_us: u64) -> f32 {
        match jitter_us {
            0..=1_000 => 1.0,
            1_001..=5_000 => 0.8,
            5_001..=20_000 => 0.5,
            _ => 0.2,
        }
    }

    fn recommend(score: f32, loss_rate: f32, send_bps: u64) -> QualityRecommendation {
        if score >= 0.9 {
            QualityRecommendation::Optimal
        } else if score < 0.2 {
            QualityRecommendation::ConnectionUnusable
        } else if loss_rate > 0.02 && loss_rate <= 0.10 {
            QualityRecommendation::EnableFec { ratio: loss_rate * 2.0 }
        } else if score < 0.4 {
            QualityRecommendation::PauseNonEssential
        } else if score < 0.6 {
            QualityRecommendation::ReduceFramerate
        } else if send_bps > 0 && score < 0.7 {
            QualityRecommendation::ReduceBitrate { target_bps: send_bps * 3 / 4 }
        } else {
            QualityRecommendation::Optimal
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p prism-transport -- quality::tests`
Expected: 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/prism-transport/src/quality/mod.rs
git commit -m "feat(transport): ConnectionQuality composite score with QualityRecommendation"
```

---

## Task 11: MtuTracker + DatagramCoalescer

**Files:**
- Modify: `crates/prism-transport/src/quality/mtu.rs`
- Modify: `crates/prism-transport/src/coalesce.rs`

- [ ] **Step 1: Write failing tests for MtuTracker**

Replace `crates/prism-transport/src/quality/mtu.rs`:

```rust
use std::time::{Duration, Instant};

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_initial_value() {
        let tracker = MtuTracker::new(1200);
        assert_eq!(tracker.current_max(), 1200);
    }

    #[test]
    fn update_changes_value() {
        let mut tracker = MtuTracker::new(1200);
        tracker.update(1400);
        assert_eq!(tracker.current_max(), 1400);
    }

    #[test]
    fn needs_recheck_after_interval() {
        let mut tracker = MtuTracker::with_check_interval(1200, Duration::from_millis(10));
        assert!(!tracker.needs_recheck()); // just created
        std::thread::sleep(Duration::from_millis(15));
        assert!(tracker.needs_recheck());
        tracker.update(1200); // recheck resets timer
        assert!(!tracker.needs_recheck());
    }

    #[test]
    fn mtu_shrink_detected() {
        let mut tracker = MtuTracker::new(1400);
        tracker.update(1200);
        assert_eq!(tracker.current_max(), 1200);
    }
}
```

- [ ] **Step 2: Run MtuTracker tests to verify they fail**

Run: `cargo test -p prism-transport -- mtu`
Expected: FAIL — `MtuTracker` not defined

- [ ] **Step 3: Implement MtuTracker**

Add above `#[cfg(test)]` in `quality/mtu.rs`:

```rust
/// Cached MTU tracker. Re-queries the connection at most once per second.
pub struct MtuTracker {
    last_known: usize,
    check_interval: Duration,
    last_check: Instant,
}

impl MtuTracker {
    pub fn new(initial_mtu: usize) -> Self {
        Self {
            last_known: initial_mtu,
            check_interval: Duration::from_secs(1),
            last_check: Instant::now(),
        }
    }

    pub fn with_check_interval(initial_mtu: usize, interval: Duration) -> Self {
        Self {
            last_known: initial_mtu,
            check_interval: interval,
            last_check: Instant::now(),
        }
    }

    /// Current maximum datagram payload size.
    pub fn current_max(&self) -> usize {
        self.last_known
    }

    /// Whether enough time has passed to re-query the connection.
    pub fn needs_recheck(&self) -> bool {
        self.last_check.elapsed() >= self.check_interval
    }

    /// Update the cached MTU value (call after re-querying connection).
    pub fn update(&mut self, new_mtu: usize) {
        self.last_known = new_mtu;
        self.last_check = Instant::now();
    }
}
```

- [ ] **Step 4: Run MtuTracker tests to verify they pass**

Run: `cargo test -p prism-transport -- mtu`
Expected: 4 tests pass

- [ ] **Step 5: Write failing tests for DatagramCoalescer**

Replace `crates/prism-transport/src/coalesce.rs`:

```rust
use bytes::BytesMut;
use std::time::{Duration, Instant};

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_coalescer_produces_nothing() {
        let coal = DatagramCoalescer::new(1200);
        assert!(coal.is_empty());
    }

    #[test]
    fn push_and_flush_single() {
        let mut coal = DatagramCoalescer::new(1200);
        coal.push(b"hello");
        assert!(!coal.is_empty());
        let flushed = coal.flush();
        // Format: [4-byte LE length][data]
        assert_eq!(&flushed[0..4], &5u32.to_le_bytes());
        assert_eq!(&flushed[4..9], b"hello");
        assert!(coal.is_empty());
    }

    #[test]
    fn coalesce_multiple_small_messages() {
        let mut coal = DatagramCoalescer::new(1200);
        coal.push(b"aa");
        coal.push(b"bb");
        coal.push(b"cc");
        let flushed = coal.flush();
        // 3 sub-datagrams: each has 4-byte length + data
        assert_eq!(flushed.len(), (4 + 2) * 3);
    }

    #[test]
    fn should_flush_when_exceeds_max_size() {
        let mut coal = DatagramCoalescer::new(20); // small max
        coal.push(b"aaaaaaaaaa"); // 10 bytes + 4 prefix = 14
        assert!(!coal.should_flush()); // still fits
        coal.push(b"bbbbbbbbbb"); // another 14, total 28 > 20
        assert!(coal.should_flush());
    }

    #[test]
    fn should_flush_after_interval() {
        let mut coal = DatagramCoalescer::with_flush_interval(1200, Duration::from_millis(1));
        coal.push(b"data");
        std::thread::sleep(Duration::from_millis(2));
        assert!(coal.should_flush());
    }

    #[test]
    fn split_coalesced_datagram() {
        let mut coal = DatagramCoalescer::new(1200);
        coal.push(b"first");
        coal.push(b"second");
        let flushed = coal.flush();

        let messages = DatagramCoalescer::split(&flushed);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], b"first");
        assert_eq!(messages[1], b"second");
    }
}
```

- [ ] **Step 6: Run DatagramCoalescer tests to verify they fail**

Run: `cargo test -p prism-transport -- coalesce`
Expected: FAIL — `DatagramCoalescer` not defined

- [ ] **Step 7: Implement DatagramCoalescer**

Add above `#[cfg(test)]` in `coalesce.rs`:

```rust
/// Coalesces small datagrams within a flush window to reduce syscall overhead.
/// Each sub-datagram is length-prefixed (4-byte LE u32).
pub struct DatagramCoalescer {
    buffer: BytesMut,
    max_size: usize,
    flush_interval: Duration,
    last_flush: Instant,
}

impl DatagramCoalescer {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: BytesMut::new(),
            max_size,
            flush_interval: Duration::from_millis(1),
            last_flush: Instant::now(),
        }
    }

    pub fn with_flush_interval(max_size: usize, interval: Duration) -> Self {
        Self {
            buffer: BytesMut::new(),
            max_size,
            flush_interval: interval,
            last_flush: Instant::now(),
        }
    }

    /// Add a small datagram to the coalescing buffer.
    pub fn push(&mut self, data: &[u8]) {
        use bytes::BufMut;
        self.buffer.put_u32_le(data.len() as u32);
        self.buffer.put_slice(data);
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Whether the buffer should be flushed (size exceeded or time elapsed).
    pub fn should_flush(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.len() > self.max_size || self.last_flush.elapsed() >= self.flush_interval
    }

    /// Flush the coalesced buffer, returning the combined datagram.
    pub fn flush(&mut self) -> Vec<u8> {
        self.last_flush = Instant::now();
        self.buffer.split().to_vec()
    }

    /// Split a coalesced datagram back into individual messages.
    pub fn split(data: &[u8]) -> Vec<Vec<u8>> {
        let mut messages = Vec::new();
        let mut pos = 0;
        while pos + 4 <= data.len() {
            let len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
            pos += 4;
            if pos + len > data.len() {
                break;
            }
            messages.push(data[pos..pos + len].to_vec());
            pos += len;
        }
        messages
    }
}
```

- [ ] **Step 8: Run all tests to verify they pass**

Run: `cargo test -p prism-transport -- mtu coalesce`
Expected: 10 tests pass (4 MtuTracker + 6 DatagramCoalescer)

- [ ] **Step 9: Commit**

```bash
git add crates/prism-transport/src/quality/mtu.rs crates/prism-transport/src/coalesce.rs
git commit -m "feat(transport): MtuTracker with periodic recheck, DatagramCoalescer with 1ms batching"
```

---

## Task 12: StreamPool + ConnectionProber

**Files:**
- Modify: `crates/prism-transport/src/stream_pool.rs`
- Modify: `crates/prism-transport/src/quality/prober.rs`

- [ ] **Step 1: Write failing tests for StreamPool**

Replace `crates/prism-transport/src/stream_pool.rs`:

```rust
use crate::connection::{OwnedSendStream, OwnedRecvStream, PrismConnection, StreamPriority, TransportError};

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::mock::MockConnection;

    #[tokio::test]
    async fn empty_pool_opens_new_stream() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(4);
        assert_eq!(pool.available(), 0);
        let (_send, _recv) = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
    }

    #[tokio::test]
    async fn maintain_fills_pool() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(4);
        pool.maintain(&conn, StreamPriority::Low).await;
        assert_eq!(pool.available(), 4);
    }

    #[tokio::test]
    async fn acquire_from_maintained_pool() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(4);
        pool.maintain(&conn, StreamPriority::Low).await;
        let (_send, _recv) = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
        assert_eq!(pool.available(), 3);
    }

    #[tokio::test]
    async fn acquire_drains_pool_then_opens_new() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(2);
        pool.maintain(&conn, StreamPriority::Low).await;
        let _ = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
        let _ = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
        assert_eq!(pool.available(), 0);
        // Next acquire opens a new stream directly
        let _ = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
    }
}
```

- [ ] **Step 2: Run StreamPool tests to verify they fail**

Run: `cargo test -p prism-transport -- stream_pool`
Expected: FAIL — `StreamPool` not defined

- [ ] **Step 3: Implement StreamPool**

Add above `#[cfg(test)]` in `stream_pool.rs`:

```rust
/// Pre-opened stream pool. Eliminates stream-open latency for frequent short-lived streams.
/// QUIC streams are one-use: used streams cannot be returned. The pool holds only UNUSED pre-opened streams.
pub struct StreamPool {
    pool: Vec<(OwnedSendStream, OwnedRecvStream)>,
    pool_size: usize,
}

impl StreamPool {
    pub fn new(pool_size: usize) -> Self {
        Self {
            pool: Vec::with_capacity(pool_size),
            pool_size,
        }
    }

    /// Number of pre-opened streams available.
    pub fn available(&self) -> usize {
        self.pool.len()
    }

    /// Acquire a stream pair. Uses a pooled stream if available, otherwise opens new.
    pub async fn acquire(
        &mut self,
        conn: &dyn PrismConnection,
        priority: StreamPriority,
    ) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
        if let Some(pair) = self.pool.pop() {
            Ok(pair)
        } else {
            conn.open_bi(priority).await
        }
    }

    /// Replenish the pool to its target size.
    pub async fn maintain(&mut self, conn: &dyn PrismConnection, priority: StreamPriority) {
        while self.pool.len() < self.pool_size {
            match conn.open_bi(priority).await {
                Ok(pair) => self.pool.push(pair),
                Err(_) => break,
            }
        }
    }
}
```

- [ ] **Step 4: Run StreamPool tests to verify they pass**

Run: `cargo test -p prism-transport -- stream_pool`
Expected: 4 tests pass

- [ ] **Step 5: Write failing tests for ConnectionProber**

Replace `crates/prism-transport/src/quality/prober.rs`:

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_prober_generates_first_probe_immediately() {
        let mut prober = ConnectionProber::new();
        let probe = prober.generate_probe();
        assert!(probe.is_some());
        assert_eq!(probe.unwrap().seq, 0);
    }

    #[test]
    fn second_probe_respects_interval() {
        let mut prober = ConnectionProber::new();
        prober.generate_probe(); // first probe
        let probe = prober.generate_probe(); // too soon
        assert!(probe.is_none());
    }

    #[test]
    fn process_echo_computes_rtt() {
        let mut prober = ConnectionProber::new();
        let probe = prober.generate_probe().unwrap();
        let echo = ProbeEcho {
            seq: probe.seq,
            sender_timestamp_us: probe.sender_timestamp_us,
            responder_timestamp_us: 999_999, // peer's clock, doesn't matter for RTT
        };
        // Small sleep to get nonzero RTT
        std::thread::sleep(Duration::from_millis(1));
        let result = prober.process_echo(&echo, Instant::now());
        assert!(result.is_some());
        let rtt = result.unwrap().rtt;
        assert!(rtt >= Duration::from_millis(1));
    }

    #[test]
    fn process_unknown_echo_returns_none() {
        let mut prober = ConnectionProber::new();
        let echo = ProbeEcho { seq: 999, sender_timestamp_us: 0, responder_timestamp_us: 0 };
        assert!(prober.process_echo(&echo, Instant::now()).is_none());
    }

    #[test]
    fn adaptive_frequency_changes_interval() {
        let mut prober = ConnectionProber::new();
        prober.set_activity(ActivityState::Idle);
        assert_eq!(prober.probe_interval(), Duration::from_secs(60));
        prober.set_activity(ActivityState::ActiveStreaming);
        assert_eq!(prober.probe_interval(), Duration::from_secs(2));
    }

    #[test]
    fn probe_payload_roundtrip() {
        let payload = ProbePayload { seq: 42, sender_timestamp_us: 123_456_789 };
        let bytes = payload.to_bytes();
        let decoded = ProbePayload::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.sender_timestamp_us, 123_456_789);
    }

    #[test]
    fn probe_echo_roundtrip() {
        let echo = ProbeEcho {
            seq: 7,
            sender_timestamp_us: 100,
            responder_timestamp_us: 200,
        };
        let bytes = echo.to_bytes();
        let decoded = ProbeEcho::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.seq, 7);
        assert_eq!(decoded.sender_timestamp_us, 100);
        assert_eq!(decoded.responder_timestamp_us, 200);
    }

    #[test]
    fn latest_rtt_updates() {
        let mut prober = ConnectionProber::new();
        assert!(prober.latest_rtt().is_none());
        let probe = prober.generate_probe().unwrap();
        let echo = ProbeEcho {
            seq: probe.seq,
            sender_timestamp_us: probe.sender_timestamp_us,
            responder_timestamp_us: 0,
        };
        std::thread::sleep(Duration::from_millis(1));
        prober.process_echo(&echo, Instant::now());
        assert!(prober.latest_rtt().is_some());
    }
}
```

- [ ] **Step 6: Run ConnectionProber tests to verify they fail**

Run: `cargo test -p prism-transport -- prober`
Expected: FAIL — `ConnectionProber`, `ProbePayload`, etc. not defined

- [ ] **Step 7: Implement ConnectionProber**

Add above `#[cfg(test)]` in `quality/prober.rs`:

```rust
/// Activity state for adaptive probe frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    ActiveStreaming,
    ActiveTransfer,
    BackgroundSync,
    Idle,
}

/// Probe payload sent to the peer (12 bytes).
pub struct ProbePayload {
    pub seq: u32,
    pub sender_timestamp_us: u64,
}

impl ProbePayload {
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut buf = [0u8; 12];
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4..12].copy_from_slice(&self.sender_timestamp_us.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < 12 { return None; }
        Some(Self {
            seq: u32::from_le_bytes(buf[0..4].try_into().ok()?),
            sender_timestamp_us: u64::from_le_bytes(buf[4..12].try_into().ok()?),
        })
    }
}

/// Probe echo from the peer (20 bytes).
pub struct ProbeEcho {
    pub seq: u32,
    pub sender_timestamp_us: u64,
    pub responder_timestamp_us: u64,
}

impl ProbeEcho {
    pub fn to_bytes(&self) -> [u8; 20] {
        let mut buf = [0u8; 20];
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4..12].copy_from_slice(&self.sender_timestamp_us.to_le_bytes());
        buf[12..20].copy_from_slice(&self.responder_timestamp_us.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < 20 { return None; }
        Some(Self {
            seq: u32::from_le_bytes(buf[0..4].try_into().ok()?),
            sender_timestamp_us: u64::from_le_bytes(buf[4..12].try_into().ok()?),
            responder_timestamp_us: u64::from_le_bytes(buf[12..20].try_into().ok()?),
        })
    }
}

/// Result from processing a probe echo.
pub struct ProbeResult {
    pub rtt: Duration,
    pub local_send_us: u64,
    pub local_recv_us: u64,
    pub remote_timestamp_us: u64,
}

/// Adaptive connection prober. Generates probe payloads and computes RTT from echoes.
/// Never touches the connection directly — callers handle send/receive.
pub struct ConnectionProber {
    epoch: Instant,
    pending: HashMap<u32, Instant>,
    next_seq: u32,
    interval: Duration,
    last_probe: Option<Instant>,
    rtt: Option<Duration>,
}

impl ConnectionProber {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            pending: HashMap::new(),
            next_seq: 0,
            interval: Duration::from_secs(2), // default: active streaming
            last_probe: None,
            rtt: None,
        }
    }

    /// Generate a probe payload if enough time has elapsed since the last probe.
    pub fn generate_probe(&mut self) -> Option<ProbePayload> {
        if let Some(last) = self.last_probe {
            if last.elapsed() < self.interval {
                return None;
            }
        }

        let seq = self.next_seq;
        self.next_seq += 1;
        let now = Instant::now();
        let timestamp_us = now.duration_since(self.epoch).as_micros() as u64;
        self.pending.insert(seq, now);
        self.last_probe = Some(now);

        Some(ProbePayload { seq, sender_timestamp_us: timestamp_us })
    }

    /// Process a probe echo. Returns timing data for RTT and one-way delay estimation.
    pub fn process_echo(&mut self, echo: &ProbeEcho, recv_time: Instant) -> Option<ProbeResult> {
        let send_time = self.pending.remove(&echo.seq)?;
        let rtt = recv_time.duration_since(send_time);
        self.rtt = Some(rtt);

        let send_us = send_time.duration_since(self.epoch).as_micros() as u64;
        let recv_us = recv_time.duration_since(self.epoch).as_micros() as u64;

        Some(ProbeResult {
            rtt,
            local_send_us: send_us,
            local_recv_us: recv_us,
            remote_timestamp_us: echo.responder_timestamp_us,
        })
    }

    /// Set probe frequency based on activity state.
    pub fn set_activity(&mut self, state: ActivityState) {
        self.interval = match state {
            ActivityState::ActiveStreaming => Duration::from_secs(2),
            ActivityState::ActiveTransfer => Duration::from_secs(5),
            ActivityState::BackgroundSync => Duration::from_secs(30),
            ActivityState::Idle => Duration::from_secs(60),
        };
    }

    /// Current probe interval.
    pub fn probe_interval(&self) -> Duration {
        self.interval
    }

    /// Latest measured RTT.
    pub fn latest_rtt(&self) -> Option<Duration> {
        self.rtt
    }
}

impl Default for ConnectionProber {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 8: Run all tests to verify they pass**

Run: `cargo test -p prism-transport -- stream_pool prober`
Expected: 12 tests pass (4 StreamPool + 8 ConnectionProber)

- [ ] **Step 9: Commit**

```bash
git add crates/prism-transport/src/stream_pool.rs crates/prism-transport/src/quality/prober.rs
git commit -m "feat(transport): StreamPool pre-opened streams, ConnectionProber with adaptive frequency"
```

---

## Task 13: QUIC Config + Socket + Auth Token + QuicConnection

**Files:**
- Modify: `crates/prism-transport/src/quic/config.rs`
- Modify: `crates/prism-transport/src/quic/socket.rs`
- Modify: `crates/prism-transport/src/quic/auth_token.rs`
- Modify: `crates/prism-transport/src/quic/connection.rs`
- Modify: `crates/prism-transport/src/quic/mod.rs`
- Modify: `crates/prism-transport/src/lib.rs`

### Part A: QUIC Config Profiles

- [ ] **Step 1: Write failing tests for config profiles**

Replace `crates/prism-transport/src/quic/config.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_config_creates_successfully() {
        let config = latency_transport_config(None);
        // Just verify it doesn't panic — quinn doesn't expose config getters
        drop(config);
    }

    #[test]
    fn latency_config_with_initial_rtt() {
        let config = latency_transport_config(Some(Duration::from_millis(50)));
        drop(config);
    }

    #[test]
    fn throughput_config_creates_successfully() {
        let config = throughput_transport_config();
        drop(config);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-transport -- quic::config`
Expected: FAIL — `latency_transport_config`, `throughput_transport_config` not defined

- [ ] **Step 3: Implement QUIC config profiles**

Add above `#[cfg(test)]` in `quic/config.rs`:

```rust
/// Latency-optimized QUIC transport config (BBR congestion control).
/// Used for Display, Input, Audio, Control channels.
pub fn latency_transport_config(initial_rtt: Option<Duration>) -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();

    config.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
    config.datagram_receive_buffer_size(Some(65_536));
    config.max_idle_timeout(Some(Duration::from_secs(10).try_into().unwrap()));
    config.keep_alive_interval(Some(Duration::from_secs(5)));
    config.initial_max_data(4_194_304);                    // 4MB
    config.initial_max_stream_data_bidi_local(1_048_576);  // 1MB per stream
    config.initial_max_stream_data_bidi_remote(1_048_576);
    config.initial_max_stream_data_uni(1_048_576);
    config.initial_max_streams_bidi(16.into());
    config.initial_max_streams_uni(16.into());
    config.mtu_discovery_config(Some(quinn::MtuDiscoveryConfig::default()));

    if let Some(rtt) = initial_rtt {
        config.initial_rtt(rtt);
    }

    config
}

/// Throughput-optimized QUIC transport config (Cubic congestion control).
/// Used for FileShare, Device forwarding, bulk transfers.
pub fn throughput_transport_config() -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();

    config.congestion_controller_factory(Arc::new(quinn::congestion::CubicConfig::default()));
    config.datagram_receive_buffer_size(None); // no datagrams on throughput connection
    config.max_idle_timeout(Some(Duration::from_secs(300).try_into().unwrap()));
    config.keep_alive_interval(Some(Duration::from_secs(30)));
    config.initial_max_data(33_554_432);                     // 32MB
    config.initial_max_stream_data_bidi_local(8_388_608);    // 8MB per stream
    config.initial_max_stream_data_bidi_remote(8_388_608);
    config.initial_max_stream_data_uni(8_388_608);
    config.initial_max_streams_bidi(64.into());
    config.initial_max_streams_uni(64.into());

    config
}
```

- [ ] **Step 4: Run config tests to verify they pass**

Run: `cargo test -p prism-transport -- quic::config`
Expected: 3 tests pass

### Part B: Socket Setup

- [ ] **Step 5: Write failing tests for socket setup**

Replace `crates/prism-transport/src/quic/socket.rs`:

```rust
use crate::connection::TransportError;
use std::net::{SocketAddr, UdpSocket};

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_latency_socket_binds() {
        let socket = create_latency_socket("127.0.0.1:0".parse().unwrap()).unwrap();
        assert!(socket.local_addr().unwrap().port() > 0);
    }

    #[test]
    fn create_throughput_socket_binds() {
        let socket = create_throughput_socket("127.0.0.1:0".parse().unwrap()).unwrap();
        assert!(socket.local_addr().unwrap().port() > 0);
    }

    #[test]
    fn dscp_set_does_not_error() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        // DSCP set may silently fail on some OS configurations but should not error
        let _ = set_dscp(&socket, 0x2E);
    }
}
```

- [ ] **Step 6: Run socket tests to verify they fail**

Run: `cargo test -p prism-transport -- quic::socket`
Expected: FAIL — `create_latency_socket`, etc. not defined

- [ ] **Step 7: Implement socket setup**

Add above `#[cfg(test)]` in `quic/socket.rs`:

```rust
use socket2::SockRef;

/// Set DSCP value on a UDP socket via TOS field.
/// DSCP occupies the top 6 bits of the 8-bit TOS byte.
pub fn set_dscp(socket: &UdpSocket, dscp: u8) -> Result<(), std::io::Error> {
    let sock_ref = SockRef::from(socket);
    let tos = (dscp as u32) << 2;
    sock_ref.set_tos(tos)
}

/// Create a latency-optimized UDP socket.
/// DSCP EF (0x2E), 4MB recv buffer, 2MB send buffer.
pub fn create_latency_socket(addr: SocketAddr) -> Result<UdpSocket, TransportError> {
    let socket = UdpSocket::bind(addr)
        .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
    let _ = set_dscp(&socket, 0x2E);
    let _ = socket.set_nonblocking(true);
    let sock_ref = SockRef::from(&socket);
    let _ = sock_ref.set_recv_buffer_size(4 * 1024 * 1024);
    let _ = sock_ref.set_send_buffer_size(2 * 1024 * 1024);
    Ok(socket)
}

/// Create a throughput-optimized UDP socket.
/// DSCP AF11 (0x0A), 16MB recv buffer, 4MB send buffer.
pub fn create_throughput_socket(addr: SocketAddr) -> Result<UdpSocket, TransportError> {
    let socket = UdpSocket::bind(addr)
        .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
    let _ = set_dscp(&socket, 0x0A);
    let _ = socket.set_nonblocking(true);
    let sock_ref = SockRef::from(&socket);
    let _ = sock_ref.set_recv_buffer_size(16 * 1024 * 1024);
    let _ = sock_ref.set_send_buffer_size(4 * 1024 * 1024);
    Ok(socket)
}
```

- [ ] **Step 8: Run socket tests to verify they pass**

Run: `cargo test -p prism-transport -- quic::socket`
Expected: 3 tests pass

### Part C: Auth Token

- [ ] **Step 9: Write failing tests for auth token**

Replace `crates/prism-transport/src/quic/auth_token.rs`:

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip() {
        let secret = [42u8; 32];
        let device_id = Uuid::from_bytes([1; 16]);
        let expires_at = 1_000_000u64;
        let token = generate_throughput_token(&secret, &device_id, expires_at);
        assert!(validate_throughput_token(&secret, &device_id, expires_at, &token, 500_000));
    }

    #[test]
    fn token_wrong_secret_fails() {
        let secret = [42u8; 32];
        let wrong_secret = [99u8; 32];
        let device_id = Uuid::from_bytes([1; 16]);
        let token = generate_throughput_token(&secret, &device_id, 1_000_000);
        assert!(!validate_throughput_token(&wrong_secret, &device_id, 1_000_000, &token, 0));
    }

    #[test]
    fn token_wrong_device_fails() {
        let secret = [42u8; 32];
        let device_id1 = Uuid::from_bytes([1; 16]);
        let device_id2 = Uuid::from_bytes([2; 16]);
        let token = generate_throughput_token(&secret, &device_id1, 1_000_000);
        assert!(!validate_throughput_token(&secret, &device_id2, 1_000_000, &token, 0));
    }

    #[test]
    fn token_expired_fails() {
        let secret = [42u8; 32];
        let device_id = Uuid::from_bytes([1; 16]);
        let expires_at = 1_000u64;
        let token = generate_throughput_token(&secret, &device_id, expires_at);
        assert!(!validate_throughput_token(&secret, &device_id, expires_at, &token, 2_000)); // current > expires
    }

    #[test]
    fn different_tokens_for_different_devices() {
        let secret = [42u8; 32];
        let id1 = Uuid::from_bytes([1; 16]);
        let id2 = Uuid::from_bytes([2; 16]);
        let tok1 = generate_throughput_token(&secret, &id1, 1_000_000);
        let tok2 = generate_throughput_token(&secret, &id2, 1_000_000);
        assert_ne!(tok1, tok2);
    }
}
```

- [ ] **Step 10: Run auth token tests to verify they fail**

Run: `cargo test -p prism-transport -- auth_token`
Expected: FAIL — `generate_throughput_token`, `validate_throughput_token` not defined

- [ ] **Step 11: Implement auth token**

Add above `#[cfg(test)]` in `quic/auth_token.rs`:

```rust
/// Generate an HMAC session token binding a device to a throughput connection.
pub fn generate_throughput_token(
    session_secret: &[u8; 32],
    device_id: &Uuid,
    expires_at: u64,
) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(session_secret).unwrap();
    mac.update(b"throughput");
    mac.update(device_id.as_bytes());
    mac.update(&expires_at.to_le_bytes());
    let result = mac.finalize();
    let mut token = [0u8; 32];
    token.copy_from_slice(&result.into_bytes());
    token
}

/// Validate a throughput session token. Constant-time comparison.
pub fn validate_throughput_token(
    session_secret: &[u8; 32],
    device_id: &Uuid,
    expires_at: u64,
    token: &[u8; 32],
    current_time: u64,
) -> bool {
    if current_time > expires_at {
        return false;
    }
    let mut mac = HmacSha256::new_from_slice(session_secret).unwrap();
    mac.update(b"throughput");
    mac.update(device_id.as_bytes());
    mac.update(&expires_at.to_le_bytes());
    mac.verify_slice(token).is_ok()
}
```

- [ ] **Step 12: Run auth token tests to verify they pass**

Run: `cargo test -p prism-transport -- auth_token`
Expected: 5 tests pass

### Part D: QuicConnection

- [ ] **Step 13: Write failing test for QuicConnection**

Replace `crates/prism-transport/src/quic/connection.rs`:

```rust
use bytes::Bytes;
use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::connection::{
    PrismConnection, OwnedSendStream, OwnedRecvStream, StreamPriority,
    TransportError, TransportMetrics, TransportType, TransportEvent,
};

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Helper: create a pair of connected quinn endpoints on loopback.
    async fn loopback_pair() -> (quinn::Connection, quinn::Connection) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        let mut server_config = quinn::ServerConfig::with_single_cert(
            vec![cert_der.clone()],
            key_der.into(),
        ).unwrap();
        server_config.transport_config(Arc::new(super::super::config::latency_transport_config(None)));

        let server_endpoint = quinn::Endpoint::server(
            server_config,
            "127.0.0.1:0".parse().unwrap(),
        ).unwrap();
        let server_addr = server_endpoint.local_addr().unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der).unwrap();
        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).unwrap()
        ));

        let mut client_endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);

        let connect_future = client_endpoint.connect(server_addr, "localhost").unwrap();
        let accept_future = server_endpoint.accept().await.unwrap();

        let (client_conn, server_conn) = tokio::join!(
            async { connect_future.await.unwrap() },
            async { accept_future.await.unwrap() },
        );

        (client_conn, server_conn)
    }

    #[tokio::test]
    async fn quic_connection_metadata() {
        let (client_conn, _server_conn) = loopback_pair().await;
        let qc = QuicConnection::new(client_conn);
        assert_eq!(qc.transport_type(), TransportType::Quic);
        assert!(qc.max_datagram_size() > 0);
    }

    #[tokio::test]
    async fn quic_datagram_roundtrip() {
        let (client_conn, server_conn) = loopback_pair().await;
        let client_qc = QuicConnection::new(client_conn);
        let server_qc = QuicConnection::new(server_conn);

        client_qc.try_send_datagram(Bytes::from_static(b"hello")).unwrap();
        let received = server_qc.recv_datagram().await.unwrap();
        assert_eq!(received, Bytes::from_static(b"hello"));
    }

    #[tokio::test]
    async fn quic_stream_roundtrip() {
        let (client_conn, server_conn) = loopback_pair().await;
        let client_qc = QuicConnection::new(client_conn);
        let server_qc = QuicConnection::new(server_conn);

        // Client opens bi stream, writes data
        let (mut send, _recv) = client_qc.open_bi(StreamPriority::Normal).await.unwrap();
        send.write(b"stream data").await.unwrap();
        send.finish().await.unwrap();

        // Server accepts bi stream, reads data
        let (_send, recv) = server_qc.accept_bi().await.unwrap();
        let data = recv.read_to_end(1024).await.unwrap();
        assert_eq!(data, b"stream data");
    }

    #[tokio::test]
    async fn quic_uni_stream() {
        let (client_conn, server_conn) = loopback_pair().await;
        let client_qc = QuicConnection::new(client_conn);
        let server_qc = QuicConnection::new(server_conn);

        let mut send = client_qc.open_uni(StreamPriority::High).await.unwrap();
        send.write(b"unidirectional").await.unwrap();
        send.finish().await.unwrap();

        let recv = server_qc.accept_uni().await.unwrap();
        let data = recv.read_to_end(1024).await.unwrap();
        assert_eq!(data, b"unidirectional");
    }

    #[tokio::test]
    async fn quic_close() {
        let (client_conn, _server_conn) = loopback_pair().await;
        let qc = QuicConnection::new(client_conn);
        qc.close().await;
        // After close, datagram send should fail
        let result = qc.try_send_datagram(Bytes::from_static(b"after close"));
        assert!(result.is_err());
    }
}
```

- [ ] **Step 14: Run QuicConnection tests to verify they fail**

Run: `cargo test -p prism-transport -- quic::connection`
Expected: FAIL — `QuicConnection` not defined

- [ ] **Step 15: Implement QuicConnection**

Add above `#[cfg(test)]` in `quic/connection.rs`:

```rust
/// QUIC connection implementing PrismConnection.
/// Wraps a quinn::Connection with event broadcasting.
pub struct QuicConnection {
    connection: quinn::Connection,
    event_tx: broadcast::Sender<TransportEvent>,
}

impl QuicConnection {
    pub fn new(connection: quinn::Connection) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { connection, event_tx }
    }

    pub fn quinn_connection(&self) -> &quinn::Connection {
        &self.connection
    }
}

#[async_trait]
impl PrismConnection for QuicConnection {
    fn try_send_datagram(&self, data: Bytes) -> Result<(), TransportError> {
        let max = self.connection.max_datagram_size().unwrap_or(0);
        let size = data.len();
        self.connection.send_datagram(data).map_err(|e| {
            use quinn::SendDatagramError;
            match e {
                SendDatagramError::UnsupportedByPeer | SendDatagramError::Disabled => {
                    TransportError::DatagramUnsupported
                }
                SendDatagramError::TooLarge => {
                    TransportError::DatagramTooLarge { size, max }
                }
                SendDatagramError::ConnectionLost(_) => TransportError::ConnectionClosed,
            }
        })
    }

    async fn send_datagram(&self, data: Bytes) -> Result<(), TransportError> {
        // If datagram fits, try direct send
        if let Some(max) = self.connection.max_datagram_size() {
            if data.len() <= max {
                return self.try_send_datagram(data);
            }
        }
        // Spill to unidirectional stream
        let mut stream = self.connection.open_uni().await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        stream.write_all(&data).await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        stream.finish().map_err(|e| TransportError::StreamError(e.to_string()))?;
        Ok(())
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        self.connection.read_datagram().await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))
    }

    async fn open_bi(&self, priority: StreamPriority)
        -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>
    {
        let (mut send, recv) = self.connection.open_bi().await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        let _ = send.set_priority(priority.to_quinn_priority());
        Ok((OwnedSendStream::from_quic(send), OwnedRecvStream::from_quic(recv)))
    }

    async fn open_uni(&self, priority: StreamPriority)
        -> Result<OwnedSendStream, TransportError>
    {
        let mut send = self.connection.open_uni().await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        let _ = send.set_priority(priority.to_quinn_priority());
        Ok(OwnedSendStream::from_quic(send))
    }

    async fn accept_bi(&self)
        -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>
    {
        let (send, recv) = self.connection.accept_bi().await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        Ok((OwnedSendStream::from_quic(send), OwnedRecvStream::from_quic(recv)))
    }

    async fn accept_uni(&self) -> Result<OwnedRecvStream, TransportError> {
        let recv = self.connection.accept_uni().await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        Ok(OwnedRecvStream::from_quic(recv))
    }

    fn metrics(&self) -> TransportMetrics {
        let stats = self.connection.stats();
        TransportMetrics {
            rtt_us: stats.path.rtt.as_micros() as u64,
            rtt_variance_us: 0, // quinn doesn't expose variance directly
            bytes_sent: stats.udp_tx.bytes,
            bytes_received: stats.udp_rx.bytes,
            datagrams_sent: stats.udp_tx.datagrams,
            transport_type: TransportType::Quic,
            ..TransportMetrics::default()
        }
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Quic
    }

    fn max_datagram_size(&self) -> usize {
        self.connection.max_datagram_size().unwrap_or(0)
    }

    fn events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_tx.subscribe()
    }

    async fn close(&self) {
        self.connection.close(0u32.into(), b"close");
    }
}
```

- [ ] **Step 16: Update quic/mod.rs re-exports**

```rust
pub mod config;
pub mod socket;
pub mod auth_token;
pub mod connection;

pub use config::{latency_transport_config, throughput_transport_config};
pub use socket::{create_latency_socket, create_throughput_socket, set_dscp};
pub use auth_token::{generate_throughput_token, validate_throughput_token};
pub use connection::QuicConnection;
```

- [ ] **Step 17: Update lib.rs re-exports**

Add to the existing re-exports:

```rust
pub use unified::{UnifiedConnection, ChannelRouting, ConnectionSlot};
pub use framing::{FramedWriter, FramedReader, MAX_MESSAGE_SIZE};
pub use quality::{ConnectionQuality, QualityRecommendation};
pub use quic::QuicConnection;
```

- [ ] **Step 18: Run all tests to verify they pass**

Run: `cargo test -p prism-transport`
Expected: All tests pass (~65 total)

Note: The `loopback_pair()` tests require network access on loopback. If any quinn API has changed from the code above (method names, error types, etc.), adapt the implementation to match the actual quinn 0.11 API. The concepts are correct — the exact method signatures may need adjustment.

- [ ] **Step 19: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All crates pass (prism-protocol + prism-metrics + prism-security + prism-transport)

- [ ] **Step 20: Commit**

```bash
git add crates/prism-transport/src/quic/ crates/prism-transport/src/lib.rs
git commit -m "feat(transport): QUIC config profiles, socket setup, auth token, QuicConnection"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Crate scaffolding | 0 |
| 2 | TransportError + TransportType + StreamPriority | 7 |
| 3 | TransportMetrics + DelayAsymmetry + TransportEvent | 4 |
| 4 | OwnedSendStream + OwnedRecvStream + PrismConnection + MockConnection | 8 |
| 5 | FramedWriter + FramedReader | 5 |
| 6 | UnifiedConnection + ChannelRouting | 9 |
| 7 | BandwidthEstimator | 6 |
| 8 | OneWayDelayEstimator | 7 |
| 9 | TrendDetector | 7 |
| 10 | ConnectionQuality + QualityRecommendation | 7 |
| 11 | MtuTracker + DatagramCoalescer | 10 |
| 12 | StreamPool + ConnectionProber | 12 |
| 13 | QUIC Config + Socket + Auth Token + QuicConnection | 16 |
| **Total** | | **~98** |

**Phase 4 deferred:** WebSocket/TCP fallback, transport probing cascade, hot-switching, network change detection. These are covered in the transport spec but not implemented until Phase 4.

**Not in this plan (Plan 4: Session):** QuicTransportServer (accept loop, rate limiter), QuicTransportClient (connection establishment with Noise handshake), Session Manager integration.
