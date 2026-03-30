# PRISM Transport — Subsystem Design Spec

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-30                     |
| Authors     | Ehsan + Claude                 |
| Parent spec | 2026-03-30-prism-architecture-design.md |
| Architecture reqs | R4, R8, R14, R26, R27, R34, R37 |

This document is the complete Transport design for PRISM across all phases. It covers the transport traits, QUIC implementation with dual connections, WebSocket/TCP fallback, transport probing, hot-switching, connection quality measurement, and all optimizations. The architecture spec defines *what* Transport owns; this spec defines *how*.

---

## Table of Contents

1. [Transport Traits](#1-transport-traits)
2. [Dual-Connection Architecture](#2-dual-connection-architecture)
3. [QUIC Configuration Profiles](#3-quic-configuration-profiles)
4. [QUIC Implementation](#4-quic-implementation)
5. [Stream Ownership & Framing](#5-stream-ownership--framing)
6. [Datagram Handling](#6-datagram-handling)
7. [WebSocket/TCP Fallback](#7-websockettcp-fallback)
8. [Transport Probing & Hot-Switching](#8-transport-probing--hot-switching)
9. [Connection Quality](#9-connection-quality)
10. [Phase Mapping](#10-phase-mapping)
11. [File Layout](#11-file-layout)
12. [Testing Strategy](#12-testing-strategy)
13. [Optimizations Index](#13-optimizations-index)

---

## 1. Transport Traits

Consumers (Session Manager, Display Engine, Platform Services) code against these traits. They never know whether the underlying transport is QUIC, WebSocket, or something else.

### 1.1 PrismConnection

```rust
/// A live connection. Abstracts over QUIC, WebSocket, etc.
/// Provides separate datagram (latency-sensitive) and stream (reliable) paths.
pub trait PrismConnection: Send + Sync {
    // === Datagram path (latency-sensitive) ===

    /// Non-blocking datagram send. Returns WouldBlock if buffer full.
    /// Display Engine uses this — drop frame rather than block.
    fn try_send_datagram(&self, data: Bytes) -> Result<(), TransportError>;

    /// Blocking datagram send. Retries on temporary failure.
    /// If datagram exceeds max size, spills to unidirectional stream.
    async fn send_datagram(&self, data: Bytes) -> Result<(), TransportError>;

    /// Receive next datagram. Separate from stream path to avoid
    /// bulk stream reads delaying latency-sensitive datagrams.
    async fn recv_datagram(&self) -> Result<Bytes, TransportError>;

    // === Stream path (reliable) ===

    /// Open a bidirectional stream. Caller OWNS the streams.
    /// No internal HashMap, no Mutex, no Transport-layer bookkeeping.
    async fn open_bi(&self, priority: StreamPriority)
        -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>;

    /// Open a unidirectional send stream (no receive side).
    async fn open_uni(&self, priority: StreamPriority)
        -> Result<OwnedSendStream, TransportError>;

    /// Accept an incoming bidirectional stream from the peer.
    async fn accept_bi(&self)
        -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>;

    /// Accept an incoming unidirectional stream from the peer.
    async fn accept_uni(&self)
        -> Result<OwnedRecvStream, TransportError>;

    // === Metadata ===

    fn metrics(&self) -> TransportMetrics;
    fn transport_type(&self) -> TransportType;
    fn max_datagram_size(&self) -> usize;
    fn events(&self) -> broadcast::Receiver<TransportEvent>;
    async fn close(&self);
}
```

### 1.2 Owned Streams

```rust
/// Owned send stream. Caller writes and closes. Zero contention.
pub struct OwnedSendStream {
    inner: SendStreamInner,
}

enum SendStreamInner {
    Quic(quinn::SendStream),
    WebSocket(Arc<WebSocketSender>),
}

impl OwnedSendStream {
    pub async fn write(&mut self, data: &[u8]) -> Result<(), TransportError>;
    pub fn set_priority(&mut self, priority: StreamPriority) -> Result<(), TransportError>;
    pub async fn finish(self) -> Result<(), TransportError>;
}

/// Owned receive stream.
pub struct OwnedRecvStream {
    inner: RecvStreamInner,
}

enum RecvStreamInner {
    Quic(quinn::RecvStream),
    WebSocket(Arc<WebSocketReceiver>),
}

impl OwnedRecvStream {
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), TransportError>;
    pub async fn read_to_end(&mut self, limit: usize) -> Result<Vec<u8>, TransportError>;
}
```

### 1.3 Stream Priority

Maps to QUIC stream priority. Higher = higher priority in scheduling.

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StreamPriority {
    Critical = 0,    // Control channel handshake, capability exchange
    High = 1,        // Display keyframes, audio
    Normal = 2,      // Clipboard, notifications
    Low = 3,         // FileShare, device forwarding
    Background = 4,  // Bulk transfers during active streaming
}
```

### 1.4 Transport Events

```rust
pub enum TransportEvent {
    Connected { transport_type: TransportType, remote_addr: SocketAddr },
    Migrated { old_addr: SocketAddr, new_addr: SocketAddr },
    MetricsUpdated(TransportMetrics),
    Degraded { reason: String },
    Upgraded { from: TransportType, to: TransportType },
    Disconnected { reason: String },
}
```

### 1.5 Transport Metrics

```rust
pub struct TransportMetrics {
    pub rtt_us: u64,
    pub rtt_variance_us: u64,
    pub loss_rate: f32,

    // Bandwidth
    pub theoretical_bandwidth_bps: u64,   // cwnd-based ceiling
    pub actual_send_bps: u64,             // measured over 5s window
    pub actual_recv_bps: u64,             // measured over 5s window

    // One-way delay
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

pub enum DelayAsymmetry {
    Unknown,
    Symmetric,
    DownstreamSlow { ratio: f32 },
    UpstreamSlow { ratio: f32 },
}
```

### 1.6 Transport Error

```rust
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
```

---

## 2. Dual-Connection Architecture

Each client-server pair uses two QUIC connections on separate UDP sockets with different congestion control and OS-level QoS markings.

```
Client                                              Server
  |                                                      |
  |  Latency Connection (BBR, DSCP EF)                  |
  |  UDP socket A ─────────────── UDP socket A           |
  |  Channels: Display, Input, Audio, Control            |
  |  Datagrams: yes                                      |
  |                                                      |
  |  Throughput Connection (Cubic, DSCP AF11)            |
  |  UDP socket B ─────────────── UDP socket B           |
  |  Channels: FileShare, Device, bulk Clipboard         |
  |  Datagrams: no (all reliable streams)                |
  |                                                      |
```

### 2.1 Why Separate Sockets

Same-socket dual connections share the OS UDP buffer. A large FileShare send queues behind display frames in the same buffer. Separate sockets with DSCP markings give the OS network stack the information to prioritize:

- **Latency socket:** DSCP EF (Expedited Forwarding, 0x2E). 4MB recv buffer, 2MB send buffer.
- **Throughput socket:** DSCP AF11 (Assured Forwarding, 0x0A). 16MB recv buffer, 4MB send buffer.

DSCP works on managed networks (corporate, home QoS routers). Ignored on public internet but never harmful. WireGuard/Tailscale preserves inner packet DSCP through the tunnel.

### 2.2 UnifiedConnection Facade

Consumers don't know about the dual-connection split. The `UnifiedConnection` routes channels to the correct connection automatically.

```rust
pub struct UnifiedConnection {
    latency: Box<dyn PrismConnection>,
    throughput: Option<Box<dyn PrismConnection>>,
    channel_routing: ChannelRouting,
}

struct ChannelRouting {
    routes: [ConnectionSlot; 4],  // indexed by priority category
}

#[derive(Clone, Copy)]
enum ConnectionSlot { Latency, Throughput }

impl ChannelRouting {
    fn default() -> Self {
        Self {
            routes: [
                ConnectionSlot::Latency,     // Critical: Input
                ConnectionSlot::Latency,     // High: Display, Audio
                ConnectionSlot::Latency,     // Normal: Control, Clipboard
                ConnectionSlot::Throughput,  // Low/Background: FileShare, Device
            ],
        }
    }
}

impl UnifiedConnection {
    pub fn for_channel(&self, channel_id: u16) -> &dyn PrismConnection {
        let priority = channel_priority_category(channel_id);
        match self.channel_routing.slot_for_priority(priority) {
            ConnectionSlot::Latency => &*self.latency,
            ConnectionSlot::Throughput => {
                self.throughput.as_deref().unwrap_or(&*self.latency)
            }
        }
    }

    pub fn latency(&self) -> &dyn PrismConnection { &*self.latency }
    pub fn throughput(&self) -> &dyn PrismConnection {
        self.throughput.as_deref().unwrap_or(&*self.latency)
    }
}
```

In single-connection mode (WebSocket fallback, throughput connection failed), `throughput` is `None` and everything routes to `latency`.

### 2.3 Throughput Connection Authentication

Single Noise NK handshake on the latency connection. Throughput connection authenticated via a session token:

```rust
struct HandshakeResponse {
    noise_response: [u8; 48],
    server_caps: ServerCapabilities,
    channel_assignments: Vec<ChannelAssignment>,
    first_keyframe: Option<Bytes>,
    throughput_addr: SocketAddr,
    throughput_token: [u8; 32],      // HMAC(session_secret, "throughput")
    throughput_token_expires: u64,
}

struct ThroughputInit {
    session_token: [u8; 32],
    device_id: Uuid,
}
```

Server validates: HMAC matches, not expired, device_id matches authenticated latency connection. One DH computation total, not two.

### 2.4 Endpoint Sharing

Server uses one quinn Endpoint for all latency connections, one for all throughput connections. 2 sockets for N clients, not 2N.

```rust
struct TransportServer {
    latency_endpoint: Endpoint,
    throughput_endpoint: Endpoint,
    connections: HashMap<ClientId, UnifiedConnection>,
}
```

---

## 3. QUIC Configuration Profiles

### 3.1 Latency Profile (BBR)

```rust
fn latency_transport_config(tombstone: Option<&SessionTombstone>) -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();

    config.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
    config.datagram_receive_buffer_size(Some(65536));           // 64KB
    config.max_idle_timeout(Some(Duration::from_secs(10).try_into().unwrap()));
    config.keep_alive_interval(Some(Duration::from_secs(5)));
    config.initial_max_data(4_194_304);                         // 4MB
    config.initial_max_stream_data_bidi_local(1_048_576);       // 1MB per stream
    config.initial_max_stream_data_bidi_remote(1_048_576);
    config.initial_max_stream_data_uni(1_048_576);
    config.initial_max_streams_bidi(16.into());
    config.initial_max_streams_uni(16.into());
    config.mtu_discovery_config(Some(quinn::MtuDiscoveryConfig::default()));

    // 0-RTT warmup: use remembered RTT for correct initial pacing
    if let Some(ts) = tombstone {
        config.initial_rtt(Duration::from_micros(ts.last_rtt_us));
    }

    config
}
```

### 3.2 Throughput Profile (Cubic)

```rust
fn throughput_transport_config() -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();

    config.congestion_controller_factory(Arc::new(quinn::congestion::CubicConfig::default()));
    config.datagram_receive_buffer_size(None);                  // no datagrams
    config.max_idle_timeout(Some(Duration::from_secs(300).try_into().unwrap()));
    config.keep_alive_interval(Some(Duration::from_secs(30)));
    config.initial_max_data(33_554_432);                        // 32MB
    config.initial_max_stream_data_bidi_local(8_388_608);       // 8MB per stream
    config.initial_max_stream_data_bidi_remote(8_388_608);
    config.initial_max_stream_data_uni(8_388_608);
    config.initial_max_streams_bidi(64.into());
    config.initial_max_streams_uni(64.into());

    config
}
```

### 3.3 Socket Configuration

```rust
fn create_latency_socket(addr: SocketAddr) -> Result<UdpSocket, TransportError> {
    let socket = UdpSocket::bind(addr)?;
    set_dscp(&socket, 0x2E);                                    // EF
    socket.set_recv_buffer_size(4 * 1024 * 1024).ok();          // 4MB
    socket.set_send_buffer_size(2 * 1024 * 1024).ok();          // 2MB
    enable_kernel_timestamps(&socket).ok();                      // SO_TIMESTAMPNS
    Ok(socket)
}

fn create_throughput_socket(addr: SocketAddr) -> Result<UdpSocket, TransportError> {
    let socket = UdpSocket::bind(addr)?;
    set_dscp(&socket, 0x0A);                                    // AF11
    socket.set_recv_buffer_size(16 * 1024 * 1024).ok();         // 16MB
    socket.set_send_buffer_size(4 * 1024 * 1024).ok();          // 4MB
    Ok(socket)
}
```

### 3.4 TLS Configuration (Bound to Noise Key)

Server generates a separate Ed25519 signing keypair for TLS. Binds it to the Noise Curve25519 identity via a signed attestation.

```rust
struct ServerIdentity {
    noise_key: Identity,                // Curve25519 (Noise NK)
    tls_key: ed25519_dalek::SigningKey, // Ed25519 (TLS cert)
    tls_cert: CertificateDer,
    binding_signature: [u8; 64],        // Noise key signs TLS key
}
```

Client verifies: known Noise public key signed the TLS cert's public key. MITM cannot produce a valid binding signature.

---

## 4. QUIC Implementation

### 4.1 QuicTransportServer

Single quinn Endpoint per connection type. Rate limiter before accepting. Session token for throughput binding.

```rust
pub struct QuicTransportServer {
    latency_endpoint: Endpoint,
    throughput_endpoint: Endpoint,
    identity: Arc<ServerIdentity>,
    rate_limiter: ConnectionRateLimiter,
}
```

Accept flow:
1. Rate limiter checks source IP (~5ns)
2. Accept on latency endpoint
3. Noise NK handshake + capability exchange
4. Generate session token for throughput
5. Send token in handshake response
6. Accept on throughput endpoint, validate token
7. Return `UnifiedConnection`

If throughput connection fails: return `UnifiedConnection` with `throughput: None`. All channels route to latency connection.

### 4.2 QuicConnection

Implements `PrismConnection`. Key details:

- `try_send_datagram()`: maps directly to quinn's synchronous `send_datagram()`. Returns immediately.
- `send_datagram()`: if datagram too large, spills to a unidirectional stream (write + finish).
- `recv_datagram()` and `recv_stream()` are separate methods. Session Manager spawns separate tasks for each, preventing bulk stream reads from blocking datagram processing.
- `metrics()`: reads quinn's connection stats. Augmented by BandwidthEstimator and OneWayDelayEstimator.
- Stream priority: set via `quinn::SendStream::set_priority(i32)`.

---

## 5. Stream Ownership & Framing

### 5.1 Caller-Owned Streams

Callers open streams via `conn.open_bi()` / `conn.open_uni()` and own the returned `OwnedSendStream` / `OwnedRecvStream` directly. No Transport-layer HashMap or Mutex. Zero contention.

Channel patterns:
- **Control channel:** `open_bi(Critical)` at session start, hold for session lifetime. Use FramedWriter/FramedReader.
- **Display keyframes:** `open_uni(High)` per keyframe, write IDR, `finish()`.
- **FileShare:** `open_bi(Low)` per transfer, write chunks, `finish()` when done.
- **Audio:** Datagrams (no streams).

### 5.2 Message Framing

Length-prefixed framing for long-lived streams. Not part of PrismConnection — a utility layer on top.

```rust
pub struct FramedWriter { stream: OwnedSendStream }

impl FramedWriter {
    pub async fn send(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.stream.write(&(data.len() as u32).to_le_bytes()).await?;
        self.stream.write(data).await?;
        Ok(())
    }
}

pub struct FramedReader { stream: OwnedRecvStream }

impl FramedReader {
    pub async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE { return Err(TransportError::MessageTooLarge(len)); }
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await?;
        Ok(data)
    }
}

const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16MB
```

### 5.3 Pre-Opened Stream Pool

Eliminates stream-open latency for frequent short-lived streams (FileShare transfers).

```rust
pub struct StreamPool {
    available: Vec<(OwnedSendStream, OwnedRecvStream)>,
    pool_size: usize,   // 4
}

impl StreamPool {
    pub async fn acquire(&mut self, conn: &dyn PrismConnection, priority: StreamPriority)
        -> Result<(OwnedSendStream, OwnedRecvStream), TransportError>
    {
        self.available.pop()
            .map(Ok)
            .unwrap_or_else(|| conn.open_bi(priority))
    }

    // NOTE: QUIC streams are one-use. Used streams cannot be returned to the pool.
    // Callers finish() used streams. The pool only holds UNUSED pre-opened streams.
    // Pool maintenance (below) replaces consumed streams in the background.

    pub async fn maintain(&mut self, conn: &dyn PrismConnection, priority: StreamPriority) {
        while self.available.len() < self.pool_size {
            if let Ok(pair) = conn.open_bi(priority).await { self.available.push(pair); }
            else { break; }
        }
    }
}
```

---

## 6. Datagram Handling

### 6.1 Overflow Strategy

When a datagram exceeds `max_datagram_size()`:
1. Check `conn.max_datagram_size()` (cached via MtuTracker, re-checked every 1s)
2. If fits: `try_send_datagram()` (non-blocking, zero-copy)
3. If too large: spill to unidirectional stream (`open_uni` + write + finish)

Display P-frames as datagrams bypass QUIC congestion control pacing — sent immediately with zero pacing delay. Display keyframes on reliable streams are subject to pacing (acceptable — keyframes are infrequent).

### 6.2 Datagram Coalescing

Small datagrams (input events: 32-64 bytes) are coalesced within a 1ms window to reduce syscall overhead.

```rust
pub struct DatagramCoalescer {
    buffer: BytesMut,
    max_size: usize,
    flush_interval: Duration,   // 1ms
    last_flush: Instant,
}
```

Multiple input events within 1ms are sent as one UDP packet with length-prefixed sub-datagrams. Receiver splits by length prefix. 50% reduction in syscalls at 1000Hz input polling. Latency impact: ~0ms for first event, ~0.5ms average for coalesced events.

### 6.3 MTU Tracking

```rust
pub struct MtuTracker {
    last_known: usize,
    check_interval: Duration,  // 1 second
    last_check: Instant,
}
```

Display Engine calls `current_max()` before each frame. Re-queries `conn.max_datagram_size()` at most once per second. If MTU shrinks (VPN connects), frames transparently spill to streams.

---

## 7. WebSocket/TCP Fallback (Phase 4)

For networks where UDP is blocked. Key design: stale datagram dropping to avoid queue-up.

### 7.1 Stale Datagram Dropping

On TCP, "datagrams" go through a single ordered stream. TCP retransmits cause old data to queue behind new data. Fix: keep only the latest datagram per channel.

- `try_send_datagram()`: queues to channel-indexed ring. Max 2 entries per channel — older evicted.
- Background flush loop: every 10ms, sends latest per channel via WebSocket Binary messages. Adds 0-10ms latency (5ms average) but prevents freeze-burst artifacts.

### 7.2 Message Type Prefix

WebSocket messages carry a 1-byte type prefix to distinguish emulated datagrams from stream data:
- `0x01`: emulated datagram
- `0x02`: stream data (with stream_id header)

### 7.3 Degradation Awareness

The degradation ladder knows `transport_type() == WebSocketTcp` and pre-reduces display quality. TCP mode is "already degraded" — no need to wait for quality metrics to trigger adaptation.

---

## 8. Transport Probing & Hot-Switching

### 8.1 Phase 1-3: No Probing Needed

Tailscale handles NAT traversal and relay fallback transparently. Direct LAN connects to IP:port. No probe cascade.

### 8.2 Phase 4: Probing Cascade

For non-Tailscale users. Tries transports with parallel racing for the top 2:

```
1. Race: QUIC/UDP:port + QUIC/UDP:443 (parallel, 500ms timeout each)
2. If both fail: try PRISM relay (sequential, 500ms)
3. If relay fails: try WebSocket/TCP:443 (sequential, 500ms)
4. If all fail: AllTransportsFailed error
```

Total worst case: ~1.5 seconds to fallback to WebSocket.

### 8.3 Phase 4: Hot-Switching

Upgrade from degraded transport to better one mid-session. Event-driven:

- **Network change event** (IP change, interface up): probe immediately, then every 3s for 30s
- **Periodic** (every 60s): probe only if currently degraded
- **Upgrade path**: establish new QUIC connection → authenticate with session token → atomically swap routing table → old connection drops when references die

In-flight data: old connection delivers queued packets. New connection carries new data. Receiver handles duplicates via sequence numbers.

### 8.4 Network Change Detection

Platform-specific:
- **Windows:** `NotifyIpInterfaceChange()` callback
- **Linux:** netlink socket (RTM_NEWADDR, RTM_DELADDR)
- **macOS:** `SCNetworkReachability` + SystemConfiguration framework

---

## 9. Connection Quality

### 9.1 Proactive Prober

Sends lightweight probe datagrams, measures true RTT independent of QUIC's smoothed estimate. Probe-echo dispatch via channel (Session Manager receives echoes, routes to prober — prober never touches the connection directly).

Adaptive frequency:
- Active streaming: every 2 seconds
- Active transfer: every 5 seconds
- Background sync: every 30 seconds
- Idle: every 60 seconds

### 9.2 Bandwidth Estimator

Sliding window actual bandwidth measurement. Tracks `(timestamp, cumulative_bytes)` samples at 100ms intervals over a 5-second window. Reports `actual_send_bps` and `actual_recv_bps` — real throughput, not theoretical cwnd-based estimates.

### 9.3 One-Way Delay Estimator

Tracks minimum timestamp offset between local and remote clocks per direction. Doesn't need clock synchronization — measures *changes* relative to the observed minimum. Detects asymmetric paths:
- `DownstreamSlow`: reduce display bitrate
- `UpstreamSlow`: reduce input reporting rate
- `Symmetric`: reduce both proportionally

### 9.4 Trend Detector

Exponential moving average (5s short, 30s long) with linear regression slope over 60s window. Detects:
- `Degrading`: short EMA 50%+ above long EMA — acute degradation
- `SlowlyDegrading`: positive slope — gradual decline, preempt before visible
- `Improving`: short EMA 30%+ below long EMA — recovering
- `Stable`: no significant divergence

### 9.5 Composite ConnectionQuality

Integrates all signals into a single score (0.0-1.0) and recommendation:

```rust
pub struct ConnectionQuality {
    pub rtt: ProbeQuality,
    pub bandwidth: BandwidthQuality,
    pub loss: LossQuality,
    pub asymmetry: DelayAsymmetry,
    pub score: f32,
    pub recommendation: QualityRecommendation,
}

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
```

Score formula: `rtt_score * 0.4 + loss_score * 0.35 + jitter_score * 0.25`

The degradation ladder in Display Engine consumes `recommendation` directly. Transport owns measurement. Display owns adaptation. Single source of truth.

---

## 10. Phase Mapping

| Component | Phase 1 | Phase 2 | Phase 3 | Phase 4 |
|-----------|---------|---------|---------|---------|
| QUIC Transport | Full: dual connection, DSCP, session token | No change | No change | No change |
| Traits | PrismConnection, OwnedSendStream/RecvStream | No change | No change | No change |
| UnifiedConnection | Facade + single-conn fallback | No change | No change | No change |
| Framing | FramedWriter/Reader | No change | No change | No change |
| StreamPool | 4 pre-opened streams | No change | No change | No change |
| Config Profiles | BBR latency + Cubic throughput | No change | + UdpIo trait for AF_XDP | No change |
| Socket Setup | DSCP, buffers, kernel timestamps | No change | No change | No change |
| Quality: Prober | Adaptive frequency | No change | No change | No change |
| Quality: Bandwidth | Sliding window | No change | No change | No change |
| Quality: One-way delay | Min-offset tracking | No change | No change | No change |
| Quality: Trend | EMA + slope | No change | No change | No change |
| Quality: Composite | Score + recommendation | No change | No change | No change |
| MTU Tracker | Periodic recheck | No change | No change | No change |
| Coalescer | 1ms input batching | No change | No change | No change |
| WebSocket/TCP | Not implemented | Not implemented | Not implemented | Full implementation |
| Probing Cascade | Not needed | Not needed | Not needed | Parallel racing |
| Hot-switching | Not needed | Not needed | Not needed | Event-driven |
| Network Watcher | Not needed | Not needed | Not needed | Platform-specific |

---

## 11. File Layout

```
crates/prism-transport/src/
    lib.rs                      # re-exports, PrismConnection + PrismTransport traits
    connection.rs               # trait defs, TransportMetrics, TransportError,
                                # OwnedSendStream, OwnedRecvStream, StreamPriority
    framing.rs                  # FramedWriter, FramedReader
    unified.rs                  # UnifiedConnection, ChannelRouting

    quic/
        mod.rs                  # QuicTransportServer, QuicTransportClient
        connection.rs           # QuicConnection impl
        config.rs               # latency/throughput profiles, TLS config
        socket.rs               # DSCP, buffer sizing, kernel timestamps
        auth_token.rs           # session token for throughput binding

    websocket/                  # Phase 4
        mod.rs                  # WebSocketTransport
        connection.rs           # WebSocketConnection, stale drop, flush loop
        framing.rs              # datagram/stream mux over single WS

    probing/                    # Phase 4
        mod.rs                  # ProbingTransport, parallel racing
        hot_switch.rs           # HotSwitchManager
        network_watcher.rs      # Platform-specific network change detection

    quality/
        mod.rs                  # ConnectionQuality, QualityRecommendation
        prober.rs               # ConnectionProber (adaptive frequency)
        bandwidth.rs            # BandwidthEstimator (sliding window)
        one_way_delay.rs        # OneWayDelayEstimator (min-offset)
        trend.rs                # TrendDetector (EMA + slope)
        mtu.rs                  # MtuTracker

    coalesce.rs                 # DatagramCoalescer (1ms batching)
    stream_pool.rs              # StreamPool (pre-opened recycling)
```

---

## 12. Testing Strategy

| Category | What | How |
|----------|------|-----|
| Unit: Framing | FramedWriter/Reader roundtrip, boundaries | Known inputs, exact bytes |
| Unit: Config | Transport configs valid | Build, verify fields |
| Unit: Socket | DSCP, buffer sizes | Create, read back |
| Unit: Coalescer | Batching, flush timing | Clock-controlled |
| Unit: StreamPool | Acquire/release/limits | Mock connection |
| Unit: Bandwidth | Sliding window accuracy | Known samples |
| Unit: Trend | EMA convergence, slope | Linear data |
| Unit: OneWayDelay | Min-offset, asymmetry | Asymmetric timestamps |
| Unit: Quality | Score, recommendations | Boundary inputs |
| Unit: MtuTracker | Periodic recheck | Mock varying MTU |
| Unit: Routing | Channel→connection map | Verify assignments |
| Integration: Dual-conn | Two connections, auth, data flow | Localhost |
| Integration: Session token | Throughput binds to latency | Auth + token verify |
| Integration: Single-conn fallback | Throughput fails, all routes to latency | Block port |
| Integration: Prober | Probe→echo→RTT | Inject delay |
| Integration: Overflow | Frame > MTU → uni stream | Oversize datagram |
| Integration: Stream lifecycle | Open, write, close | Verify receiver |
| Integration: FramedWriter | 100 messages on long-lived stream | Verify all intact |
| Integration: Coalescing | 5 events in 1ms → 1 packet | Verify batching |
| Integration: StreamPool | 10 transfers, < 10 opens | Count opens |
| Perf: Datagram throughput | Max dgram/sec localhost | Verify > 100K/sec |
| Perf: Stream throughput | Max MB/sec localhost | Verify > 500MB/sec |
| Perf: Probe overhead | CPU cost at 2s interval | Verify < 10us |
| Perf: Quality compute | ConnectionQuality::compute | Verify < 5us |
| Fuzz: Framing | Malformed length-prefix | cargo-fuzz |
| Phase 4: WebSocket | Stale drop, flush timing | Verify latest-only |
| Phase 4: Hot-switch | WS→QUIC mid-session | Unblock UDP, verify < 5s |
| Phase 4: Probing | Cascade with parallel racing | Block QUIC, verify fallback |

---

## 13. Optimizations Index

| ID | Optimization | Impact | Phase |
|----|-------------|--------|-------|
| T1 | GSO/GRO batch packet I/O | Fewer syscalls, smoother timing | 1 |
| T2 | Separate UDP sockets per connection type | No cross-connection contention | 1 |
| T3 | Kernel timestamps (SO_TIMESTAMPNS) | RTT ±50us vs ±5ms | 1 |
| T4 | 0-RTT warmup with remembered initial_rtt | Correct pacing from first packet | 1 |
| T5 | BBR latency + Cubic throughput | Latency-first display, throughput-first files | 1 |
| T7 | Proactive quality probing | Real-time RTT, not lagging estimate | 1 |
| T8 | Per-direction receive buffer sizing | 4MB datagram, 16MB stream | 1 |
| T9 | try_send_datagram (non-blocking) | Never queue stale frames | 1 |
| T10 | DSCP marking per socket | OS-level QoS | 1 |
| T11 | Datagrams bypass QUIC pacing | Zero delay for P-frames | 1 |
| T12 | UdpIo trait for AF_XDP | 10-50us latency (Linux) | 3+ |
| T13 | One-way delay estimation | Asymmetric path adaptation | 1 |
| T14 | Endpoint sharing across clients | 2 sockets for N clients | 1 |
| T15 | Parallel transport racing | Save 500ms fallback | 4 |
| T16 | 1ms datagram coalescing | 50% syscall reduction | 1 |
| T17 | Pre-opened stream pool | Zero open latency | 1 |
| T18 | Trend detection (EMA + slope) | Preemptive degradation | 1 |
| T19 | Adaptive probe frequency | Battery savings on mobile | 1 |
| T20 | MTU change detection | Transparent overflow spill | 1 |
| T21 | Composite quality score | Single truth for degradation | 1 |

---

*PRISM Transport Design v1.0 — CC0 Public Domain*
