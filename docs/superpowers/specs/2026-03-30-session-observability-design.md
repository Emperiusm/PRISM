# PRISM Session Manager + Observability — Subsystem Design Spec

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-30                     |
| Authors     | Ehsan + Claude                 |
| Parent spec | 2026-03-30-prism-architecture-design.md |
| Architecture reqs | R1, R2, R5-R14, R37-R39, R47 |

This document covers Session Manager and Observability as a unified design across all phases. Session Manager is control plane only — it owns client lifecycle, channel ownership, routing, and bandwidth arbitration. Observability owns metrics recording, collection, frame tracing, and export. They share types via the `prism-metrics` crate.

---

## Table of Contents

1. [Crate Structure](#1-crate-structure)
2. [Session Manager Core](#2-session-manager-core)
3. [Channel Ownership](#3-channel-ownership)
4. [Capability Negotiation](#4-capability-negotiation)
5. [Control Channel Protocol](#5-control-channel-protocol)
6. [Connection Lifecycle & Reconnection](#6-connection-lifecycle--reconnection)
7. [Channel Dispatch](#7-channel-dispatch)
8. [Bandwidth Arbiter](#8-bandwidth-arbiter)
9. [Connection Profiles](#9-connection-profiles)
10. [Recv Loop](#10-recv-loop)
11. [Client-Side Session](#11-client-side-session)
12. [Graceful Shutdown](#12-graceful-shutdown)
13. [Observability: prism-metrics](#13-observability-prism-metrics)
14. [Observability: Frame Tracing](#14-observability-frame-tracing)
15. [Observability: Client Feedback](#15-observability-client-feedback)
16. [Observability: Collection & Export](#16-observability-collection--export)
17. [Phase Mapping](#17-phase-mapping)
18. [File Layout](#18-file-layout)
19. [Testing Strategy](#19-testing-strategy)
20. [Optimizations Index](#20-optimizations-index)
12. [Phase Mapping](#12-phase-mapping)
13. [File Layout](#13-file-layout)
14. [Testing Strategy](#14-testing-strategy)
15. [Optimizations Index](#15-optimizations-index)

---

## 1. Crate Structure

```
prism-metrics        → types, ring buffers, Observable trait, MetricsRecorder (no dependencies)
prism-session        → Session Manager, routing table, arbiter (depends on prism-metrics, prism-protocol)
prism-observability  → collection, aggregation, time-series, export (depends on prism-metrics)

All subsystems depend on prism-metrics to write metrics.
Only Session Manager depends on prism-observability to read aggregated data.
Display Engine, Transport, etc. depend ONLY on prism-metrics (lightweight).
```

---

## 2. Session Manager Core

### 2.1 Ownership

Session Manager is **control plane only** (R37). It never touches frame bytes. It owns:

- Client sessions (connect/disconnect/reconnect lifecycle)
- Routing table (which client gets which channel on which connection)
- Channel ownership (exclusive/shared/transferable)
- Bandwidth arbiter (predictive allocation)
- Capability negotiation
- Connection profiles
- Reconnection state (tombstones)

### 2.2 Core Types

```rust
pub struct SessionManager {
    clients: HashMap<ClientId, ClientSession>,
    routing_table: Arc<RoutingTable>,
    channel_registry: ChannelRegistry,
    arbiter: BandwidthArbiter,
    tombstones: TombstoneStore,
    profiles: HashMap<String, ConnectionProfile>,
    heartbeat: HeartbeatMonitor,
    metrics_collector: Arc<MetricsCollector>,
    event_tx: broadcast::Sender<SessionEvent>,
}

pub struct ClientSession {
    client_id: ClientId,
    device: Arc<PairingEntry>,
    connection: Arc<UnifiedConnection>,
    security_ctx: Arc<SecurityContext>,
    capabilities: ClientCapabilities,
    profile: ConnectionProfile,
    subscribed_channels: HashSet<u16>,
    state: SessionState,
    connected_at: Instant,
    last_activity: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Authenticating,
    Active,
    Suspended,
    Tombstoned,
}

pub type ClientId = Uuid;
```

### 2.3 Routing Table

Read-heavy, write-rare. Uses `arc-swap` for lock-free atomic pointer swap.

```rust
pub struct RoutingTable {
    inner: ArcSwap<RoutingSnapshot>,
}

pub struct RoutingSnapshot {
    channel_routes: HashMap<u16, Vec<RouteEntry>>,
    client_connections: HashMap<ClientId, Arc<UnifiedConnection>>,
    generation: u64,
}

pub struct RouteEntry {
    pub client_id: ClientId,
    pub connection: Arc<UnifiedConnection>,
    pub security_ctx: Arc<SecurityContext>,
}

impl RoutingTable {
    /// Lock-free read. Called on every frame by producers.
    /// Cost: one atomic load + Arc clone (~5ns).
    pub fn snapshot(&self) -> Arc<RoutingSnapshot> {
        self.inner.load_full()
    }

    /// Write. Called by Session Manager on connect/disconnect/ownership change.
    pub fn update(&self, new_snapshot: RoutingSnapshot) {
        self.inner.store(Arc::new(new_snapshot));
    }

    /// Incremental: batch multiple route changes into a single atomic swap.
    /// Most operations use this (6 AddRoutes on client connect = 1 swap).
    pub fn batch_update(&self, mutations: Vec<RoutingMutation>) {
        let current = self.inner.load_full();
        let mut new_snapshot = (*current).clone();
        for mutation in mutations {
            match mutation {
                RoutingMutation::AddRoute { channel_id, entry } => {
                    new_snapshot.channel_routes
                        .entry(channel_id).or_default().push(entry);
                }
                RoutingMutation::RemoveClient(client_id) => {
                    for routes in new_snapshot.channel_routes.values_mut() {
                        routes.retain(|r| r.client_id != client_id);
                    }
                    new_snapshot.client_connections.remove(&client_id);
                }
                RoutingMutation::TransferChannel { channel_id, from, to_entry } => {
                    if let Some(routes) = new_snapshot.channel_routes.get_mut(&channel_id) {
                        routes.retain(|r| r.client_id != from);
                        routes.push(to_entry);
                    }
                }
            }
        }
        new_snapshot.generation += 1;
        self.inner.store(Arc::new(new_snapshot));
    }
}

pub enum RoutingMutation {
    AddRoute { channel_id: u16, entry: RouteEntry },
    RemoveClient(ClientId),
    TransferChannel { channel_id: u16, from: ClientId, to_entry: RouteEntry },
}
```

### 2.4 Session Events

```rust
pub enum SessionEvent {
    ClientConnected { client_id: ClientId, device_name: String },
    ClientDisconnected { client_id: ClientId, reason: String },
    ClientReconnected { client_id: ClientId, was_tombstoned: bool },
    ChannelOwnershipChanged { channel_id: u16, new_owner: Option<ClientId> },
    ProfileChanged { client_id: ClientId, profile: String },
    QualityChanged { client_id: ClientId, quality: ConnectionQuality },
}
```

---

## 3. Channel Ownership

### 3.1 Ownership Model (R10)

```rust
pub struct ChannelRegistry {
    ownership: HashMap<u16, ChannelOwnership>,
}

pub enum ChannelOwnership {
    Exclusive { owner: Option<ClientId> },
    Shared { subscribers: HashSet<ClientId> },
    Transferable { owner: Option<ClientId>, transfer_policy: TransferPolicy },
}

#[derive(Clone)]
pub enum TransferPolicy {
    OnRequest,
    OwnerApproves,
    ServerDecides,
}
```

Default assignments:
- **Exclusive:** Display, Input, Camera
- **Shared:** Clipboard, Control, FileShare, Notify, Sensor, Audio
- **Transferable:** Touch (OnRequest)

### 3.2 Channel Grant Flow

```rust
pub enum ChannelGrantResult {
    Granted,
    AlreadyOwned,
    Denied { reason: String, current_owner: Option<ClientId> },
    Transferred { from: Option<ClientId> },
    PendingApproval { current_owner: Option<ClientId> },
}

impl ChannelRegistry {
    pub fn request_channel(
        &mut self, channel_id: u16, client_id: ClientId,
    ) -> Result<ChannelGrantResult, SessionError> {
        match self.ownership.get_mut(&channel_id) {
            Some(ChannelOwnership::Exclusive { owner }) => {
                if owner.is_none() {
                    *owner = Some(client_id);
                    Ok(ChannelGrantResult::Granted)
                } else if *owner == Some(client_id) {
                    Ok(ChannelGrantResult::AlreadyOwned)
                } else {
                    Ok(ChannelGrantResult::Denied {
                        reason: "exclusively owned by another client".into(),
                        current_owner: *owner,
                    })
                }
            }
            Some(ChannelOwnership::Shared { subscribers }) => {
                subscribers.insert(client_id);
                Ok(ChannelGrantResult::Granted)
            }
            Some(ChannelOwnership::Transferable { owner, transfer_policy }) => {
                if owner.is_none() {
                    *owner = Some(client_id);
                    Ok(ChannelGrantResult::Granted)
                } else {
                    match transfer_policy {
                        TransferPolicy::OnRequest | TransferPolicy::ServerDecides => {
                            let old = *owner;
                            *owner = Some(client_id);
                            Ok(ChannelGrantResult::Transferred { from: old })
                        }
                        TransferPolicy::OwnerApproves => {
                            Ok(ChannelGrantResult::PendingApproval { current_owner: *owner })
                        }
                    }
                }
            }
            None if channel_id >= EXTENSION_CHANNEL_START => {
                self.ownership.insert(channel_id, ChannelOwnership::Shared {
                    subscribers: HashSet::from([client_id]),
                });
                Ok(ChannelGrantResult::Granted)
            }
            None => Err(SessionError::UnknownChannel(channel_id)),
        }
    }

    pub fn release_all(&mut self, client_id: ClientId) {
        for ownership in self.ownership.values_mut() {
            match ownership {
                ChannelOwnership::Exclusive { owner } if *owner == Some(client_id) => {
                    *owner = None;
                }
                ChannelOwnership::Shared { subscribers } => {
                    subscribers.remove(&client_id);
                }
                ChannelOwnership::Transferable { owner, .. } if *owner == Some(client_id) => {
                    *owner = None;
                }
                _ => {}
            }
        }
    }
}
```

---

## 4. Capability Negotiation

### 4.1 Negotiation Algorithm

When a client connects, it sends `ClientCapabilities` (all channels it supports). The server intersects with its own capabilities and returns only mutually supported channels. Clients silently ignore missing channels — a Phase 3 client connecting to a Phase 1 server simply doesn't get Clipboard or FileShare.

```rust
pub struct CapabilityNegotiator {
    server_channels: HashMap<u16, ChannelCap>,
}

impl CapabilityNegotiator {
    pub fn negotiate(&self, client_caps: &ClientCapabilities) -> NegotiationResult {
        let mut granted = Vec::new();
        let mut rejected = Vec::new();

        for client_ch in &client_caps.channels {
            match self.server_channels.get(&client_ch.channel_id) {
                Some(server_ch) => {
                    let version = client_ch.channel_version.min(server_ch.channel_version);
                    granted.push(NegotiatedChannel {
                        channel_id: client_ch.channel_id,
                        version,
                        client_config: client_ch.config.clone(),
                        server_config: server_ch.config.clone(),
                    });
                }
                None => {
                    rejected.push(client_ch.channel_id);
                }
            }
        }

        let display_codec = self.negotiate_codec(client_caps);

        NegotiationResult {
            protocol_version: client_caps.protocol_version.min(1),
            channels: granted,
            rejected_channels: rejected,
            display_codec,
        }
    }

    fn negotiate_codec(&self, client: &ClientCapabilities) -> String {
        let client_codecs: HashSet<_> = client.performance.supported_codecs.iter().collect();
        let server_codecs = self.server_channels.get(&CHANNEL_DISPLAY)
            .and_then(|c| match &c.config {
                ChannelConfig::Display(d) => Some(&d.supported_codecs),
                _ => None,
            });

        // Priority: H.265 -> H.264 -> AV1 -> software
        for codec in ["h265", "h264", "av1"] {
            if client_codecs.contains(&codec.to_string()) {
                if let Some(server) = server_codecs {
                    if server.iter().any(|c| c == codec) {
                        return codec.to_string();
                    }
                }
            }
        }
        "h264".to_string()
    }
}

pub struct NegotiatedChannel {
    pub channel_id: u16,
    pub version: u16,
    pub client_config: ChannelConfig,
    pub server_config: ChannelConfig,
}

pub struct NegotiationResult {
    pub protocol_version: u16,
    pub channels: Vec<NegotiatedChannel>,
    pub rejected_channels: Vec<u16>,
    pub display_codec: String,
}
```

### 4.2 Version Compatibility

| Client Version | Server Version | Behavior |
|---------------|---------------|----------|
| v1 | v1 | Full compatibility |
| v2 | v1 | Server offers v1 channels only. Client uses v1 semantics. |
| v1 | v2 | Client offers v1 channels only. Server responds at v1. |
| v2 | v2 | Full v2 compatibility |

Per-channel versioning allows gradual evolution. A v2 Display channel can coexist with a v1 Clipboard channel.

---

## 5. Control Channel Protocol

### 5.1 Message Type Registry

All Control channel messages use the PRISM header with `channel_id = CHANNEL_CONTROL`. The `msg_type` field identifies the message:

```rust
pub mod control_msg {
    // Session management
    pub const HEARTBEAT: u8 = 0x01;
    pub const HEARTBEAT_ACK: u8 = 0x02;
    pub const CAPABILITY_UPDATE: u8 = 0x03;
    pub const PROFILE_SWITCH: u8 = 0x04;
    pub const SESSION_INFO: u8 = 0x10;
    pub const SHUTDOWN_NOTICE: u8 = 0x20;

    // Quality & probing
    pub const PROBE_REQUEST: u8 = 0x05;
    pub const PROBE_RESPONSE: u8 = 0x06;
    pub const CLIENT_FEEDBACK: u8 = 0x07;
    pub const CLIENT_ALERT: u8 = 0x08;
    pub const QUALITY_UPDATE: u8 = 0x0D;
    pub const REDUCE_SEND_RATE: u8 = 0x0E;

    // Overlay
    pub const OVERLAY_TOGGLE: u8 = 0x09;
    pub const OVERLAY_DATA: u8 = 0x0A;

    // Security (delegated to SecurityGate)
    pub const KEY_ROTATION: u8 = 0x0B;
    pub const CERT_RENEWAL: u8 = 0x0C;

    // Multi-client
    pub const CHANNEL_TRANSFER: u8 = 0x0F;
    pub const MONITOR_LAYOUT: u8 = 0x11;

    // Transport
    pub const THROUGHPUT_TOKEN: u8 = 0x12;
}
```

### 5.2 Message Payloads

| Msg Type | Payload | Direction | Transport |
|----------|---------|-----------|-----------|
| HEARTBEAT | Empty (header only, 16 bytes) | Both | Datagram |
| HEARTBEAT_ACK | Empty | Both | Datagram |
| PROBE_REQUEST | 8 bytes (send timestamp) | Both | Datagram |
| PROBE_RESPONSE | 8 bytes (echoed timestamp) | Both | Datagram |
| CLIENT_FEEDBACK | ClientFeedback struct (JSON) | Client→Server | Stream (framed) |
| CLIENT_ALERT | ClientAlert struct (JSON) | Client→Server | Datagram |
| OVERLAY_DATA | OverlayPacket (128 bytes binary) | Server→Client | Datagram |
| QUALITY_UPDATE | ConnectionQuality summary (JSON) | Server→Client | Stream (framed) |
| SHUTDOWN_NOTICE | ShutdownNotice struct (JSON) | Server→Client | Stream (framed) |
| CHANNEL_TRANSFER | Transfer request/ack (JSON) | Both | Stream (framed) |
| MONITOR_LAYOUT | MonitorLayout struct (JSON) | Server→Client | Stream (framed) |

Lightweight messages (heartbeat, probe, overlay, alert) use datagrams. Structured messages (feedback, quality, shutdown) use the long-lived Control stream with FramedWriter/FramedReader.

---

## 6. Connection Lifecycle & Reconnection

### 4.1 Connection Flow

```
Client connects
  → Transport accepts (QUIC dual-connection)
  → Security authenticates (Noise NK + pairing check)
  → Unknown/Blocked: silent drop (never reaches Session Manager)
  → Session Manager: new_session()
    → Check tombstone: returning client?
      → Yes: restore session state, apply per-channel recovery
      → No: fresh session
    → Capability negotiation
    → Channel assignment (request channels from ChannelRegistry)
    → Build routing table entry + SecurityContext
    → Atomic routing table swap
    → Send handshake response
    → SessionState::Active
    → Spawn recv loops (datagram + stream, per connection)
```

### 4.2 State Machine

```
                    [Authenticating]
                         |
                    auth success
                         |
                         v
                +---> [Active] <---------+
                |        |               |
                |   connection lost      |
                |   (no heartbeat 10s)   |
                |        v               |
                |   [Suspended]          |
                |    /         \         |
                |  QUIC         timeout  |
                |  migration    (60s)    |
                |  succeeds       |      |
                |  /              v      |
                +-+         [Tombstoned] |
                            /         \  |
                     0-RTT resume    timeout (5min)
                     within 5min         |
                       /                 v
                      +            [Removed]
```

### 4.3 Tombstones (R5, R7)

```rust
pub struct TombstoneStore {
    tombstones: HashMap<ClientId, Tombstone>,
    max_age: Duration,   // 5 minutes
}

pub struct Tombstone {
    client_id: ClientId,
    device_id: Uuid,
    created_at: Instant,
    channel_recovery: HashMap<u16, ChannelRecoveryState>,
    last_rtt_us: u64,
    last_bandwidth_bps: u64,
    last_max_data: u64,
    confirm_cache: ConfirmCache,
    profile: ConnectionProfile,
    subscribed_channels: HashSet<u16>,
}

pub enum ChannelRecoveryState {
    /// Display: fresh IDR. No state.
    SendIdr,
    /// Clipboard: replay from client's last seq.
    ClipboardReplay { changelog: VecDeque<ClipboardEntry>, max_entries: usize },
    /// FileShare: resume from last ACK'd offset.
    FileShareResume { transfers: Vec<TransferState> },
    /// Audio: reset decoder.
    AudioReset,
    /// Notifications: replay from last ID.
    NotificationReplay { last_forwarded_id: String, pending: VecDeque<Notification> },
    /// Camera: re-negotiate.
    CameraRenegotiate,
    /// Input, Sensor: no recovery.
    NoRecovery,
}
```

Tombstone lookup is by `device_id` (not `client_id`) since key rotation may change the client's identity. Tombstone is consumed (removed) on successful reconnection.

### 4.4 Heartbeat & Suspension

```rust
pub struct HeartbeatMonitor {
    clients: HashMap<ClientId, HeartbeatState>,
    check_interval: Duration,           // 1 second
    suspend_threshold: Duration,        // 10 seconds
    tombstone_threshold: Duration,      // 60 seconds
}

struct HeartbeatState {
    last_any_activity: Instant,
    missed_count: u32,
}
```

Any packet from a client resets its heartbeat timer — not just heartbeat messages. Display frames, input events, any datagram counts as "alive."

---

## 7. Channel Dispatch

### 7.1 Channel Dispatcher

Concrete mechanism for routing messages to channel handlers. Handlers register at startup.

```rust
pub struct ChannelDispatcher {
    handlers: HashMap<u16, Arc<dyn ChannelHandler>>,
    routing_table: Arc<RoutingTable>,
    bandwidth_tracker: Arc<ChannelBandwidthTracker>,
}

impl ChannelDispatcher {
    pub fn register(&mut self, handler: Arc<dyn ChannelHandler>) {
        self.handlers.insert(handler.channel_id(), handler);
    }

    pub async fn dispatch_datagram(&self, from: ClientId, header: PrismHeader, data: Bytes) {
        if let Some(handler) = self.handlers.get(&header.channel_id) {
            let packet = PrismPacket { header, payload: data.slice(HEADER_SIZE..) };
            match handler.handle(from, packet, &self.routing_table).await {
                Ok(control_msgs) => {
                    for msg in control_msgs {
                        self.handle_control_msg(msg).await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Channel 0x{:03X} handler error: {}", header.channel_id, e);
                }
            }
        }
    }

    pub async fn handle_stream(
        &self,
        from: ClientId,
        send: OwnedSendStream,
        recv: OwnedRecvStream,
    ) {
        let mut framed = FramedReader::new(recv);
        if let Ok(first_msg) = framed.recv().await {
            if first_msg.len() >= HEADER_SIZE {
                if let Ok(header) = PrismHeader::decode(&mut Bytes::from(first_msg.clone())) {
                    if let Some(handler) = self.handlers.get(&header.channel_id) {
                        handler.handle_stream(from, send, framed, &self.routing_table).await;
                    }
                }
            }
        }
    }
}
```

### 7.2 ChannelHandler Trait (Revised)

Adds `handle_stream()` for stream-based channels and `bandwidth_needs()` for arbiter integration:

```rust
pub trait ChannelHandler: Send + Sync {
    fn channel_id(&self) -> u16;

    /// Handle a datagram-delivered message.
    async fn handle(
        &self, from: ClientId, msg: PrismPacket, routes: &RoutingTable,
    ) -> Result<Vec<ControlMsg>, ChannelError>;

    /// Handle a stream-delivered channel. Called once per stream.
    /// Default: frame-by-frame dispatch via handle().
    async fn handle_stream(
        &self,
        from: ClientId,
        send: OwnedSendStream,
        recv: FramedReader,
        routes: &RoutingTable,
    ) {
        // Default: read framed messages, dispatch via handle()
        // Channels needing custom stream logic (FileShare, Control) override this.
    }

    /// Recovery state for reconnection (R5).
    fn reconnect_state(&self, client: ClientId) -> ChannelRecoveryState;

    /// Apply recovery after reconnect.
    async fn apply_reconnect(
        &self, client: ClientId, state: &ChannelRecoveryState, routes: &RoutingTable,
    ) -> Result<Vec<ControlMsg>, ChannelError>;

    /// Dynamic bandwidth needs for arbiter (default: no bandwidth needed).
    fn bandwidth_needs(&self) -> BandwidthNeeds {
        BandwidthNeeds { min_bps: 0, ideal_bps: 0, max_bps: 0, urgency: 0.0 }
    }
}
```

---

## 8. Bandwidth Arbiter

### 5.1 Architecture (R13, R14, R47)

Event-driven rebalance (immediate on reservation/release) + periodic tick (100ms for drift correction).

```rust
pub struct BandwidthArbiter {
    client_budgets: HashMap<ClientId, ClientBudget>,
    server_profile: ServerBandwidthProfile,
    starvation_detector: StarvationDetector,
    event_tx: broadcast::Sender<ArbiterEvent>,
}

pub struct ClientBudget {
    total_bps: u64,
    allocations: HashMap<u16, ChannelAllocation>,
    pending_reservations: Vec<ReservationRequest>,
}

pub struct ChannelAllocation {
    channel_id: u16,
    allocated_bps: u64,
    actual_usage_bps: u64,
    priority: ChannelPriority,
    min_bps: u64,       // dynamic — reported by channel handler
    max_bps: u64,       // dynamic
    ideal_bps: u64,     // dynamic
}
```

### 5.2 Dynamic Bandwidth Needs

Channel handlers report their needs dynamically:

```rust
pub trait BandwidthAware {
    fn bandwidth_needs(&self) -> BandwidthNeeds;
}

pub struct BandwidthNeeds {
    pub min_bps: u64,       // can't function below this
    pub ideal_bps: u64,     // full quality
    pub max_bps: u64,       // no benefit above this
    pub urgency: f32,       // 0.0 = can wait, 1.0 = keyframe pending
}
```

Display Engine examples:
- Static desktop: `min=100K, ideal=500K, max=2M`
- Active coding: `min=500K, ideal=2M, max=5M`
- Video playing: `min=2M, ideal=8M, max=20M`

### 5.3 Allocation Algorithm

Priority-weighted proportional allocation with minimum guarantees:

1. **Satisfy minimums:** Every channel gets `min_bps`. Remaining = total - sum(mins).
2. **Apply reservations (R47):** Pending reservations pre-allocate. If insufficient headroom, preempt lower-priority channels immediately (before congestion hits).
3. **Distribute remainder:** Weighted by priority (Critical:16, High:8, Normal:4, Low:2, Background:1). Capped at each channel's `max_bps`.

Priority weights:

| Priority | Weight | Channels |
|----------|--------|----------|
| Critical | 16 | Input |
| High | 8 | Display, Audio |
| Normal | 4 | Control, Clipboard |
| Low | 2 | FileShare, Device |
| Background | 1 | Sensor, Notify |

### 5.4 Zero-Cost Allocation Handles

Producers read their allocation via shared atomics — no HashMap lookup, no channel:

```rust
pub struct AllocationHandle {
    allocated_bps: AtomicU64,
    max_bps: AtomicU64,
    min_bps: AtomicU64,
}

impl AllocationHandle {
    /// Display Engine calls every frame. Cost: 1 atomic load (~1ns).
    #[inline(always)]
    pub fn allocated_bps(&self) -> u64 {
        self.allocated_bps.load(Ordering::Relaxed)
    }
}
```

Session Manager creates `Arc<AllocationHandle>` per (client, channel). One clone to producer, one to arbiter.

### 5.5 Predictive Features

**Reservation (R47):** Channel announces intent before consuming bandwidth.

```rust
impl BandwidthArbiter {
    pub fn reserve(
        &mut self, client_id: ClientId, channel_id: u16,
        estimated_bps: u64, duration: Option<Duration>,
    ) -> AllocationResult;

    pub fn release(&mut self, client_id: ClientId, channel_id: u16);
}

pub struct AllocationResult {
    pub granted_bps: u64,
    pub pacing_interval: Duration,
}
```

**Keyframe hints:** Display Engine notifies before sending IDR. Arbiter temporarily boosts display allocation for 100ms by reducing lower-priority channels.

**Frame previews:** Display Engine reports upcoming content (scene change, video region detected). Arbiter pre-adjusts before the frames are encoded.

### 5.6 Asymmetry Response

One-way delay estimator reports `DownstreamSlow` or `UpstreamSlow`.

- `DownstreamSlow`: reduce outgoing (server→client) allocations proportionally.
- `UpstreamSlow`: notify client via Control channel to reduce its send rate. No server-side adjustment needed.

### 5.7 Per-Channel Bandwidth Tracking

Fixed-size atomic counters indexed by `channel_id & 0xFF` (256 buckets, 4KB total — no collisions between core and mobile channels):

```rust
pub struct ChannelBandwidthTracker {
    send_counters: [AtomicU64; 256],
    recv_counters: [AtomicU64; 256],
    last_reset: AtomicU64,
    window_us: u64,   // 1 second
}

impl ChannelBandwidthTracker {
    #[inline(always)]
    pub fn record_send(&self, channel_id: u16, bytes: u32) {
        self.send_counters[(channel_id & 0xFF) as usize]
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
}
```

One `AtomicU64::fetch_add` per packet (~1ns). 256 buckets (4KB) avoids collisions between core channels (0x01-0x07) and mobile extensions (0xE1-0xE4).

### 5.8 Server Bandwidth Discovery

```rust
pub struct ServerBandwidthProfile {
    estimated_uplink_bps: u64,      // from speed test on first client
    measured_uplink_bps: u64,       // from actual throughput
    user_configured_bps: Option<u64>,
    effective_bps: u64,
}
```

Discovery: on first client connection, send 256KB burst and measure throughput (~500ms). Continuously refined from aggregate actual send throughput. User can override.

### 5.9 Multi-Client Fairness

When server uplink is shared across clients, distribute by activity weight:
- Active display streaming: weight 4
- Active file transfer: weight 2
- Companion/background: weight 1

### 5.10 Starvation Detection

Monitors per-channel: if `actual_usage < min_bps` while `allocated >= min_bps` for >5 seconds, emit starvation warning. Feeds observability and client overlay.

### 5.11 Arbiter Events

```rust
pub enum ArbiterEvent {
    AllocationChanged { client_id: ClientId, allocations: HashMap<u16, ChannelAllocation> },
    ClientShouldReduceSendRate { client_id: ClientId, suggested_reduction: f32 },
    StarvationWarning(StarvationWarning),
}
```

Display Engine subscribes and adjusts encoder bitrate on the next frame (~16ms response).

---

## 9. Connection Profiles (R39)

| Profile | Optimize For | Region Detection | Max FPS | Encoder Preset | Lossless Text |
|---------|-------------|-----------------|---------|----------------|---------------|
| Gaming | Input latency, FPS | Off | 120 | UltraLowLatency | No |
| Coding | Text sharpness, low BW | On | 60 | Quality | Yes |
| Media | Color accuracy, smooth video | On (video bias) | 60 | Quality | No |
| Mobile | Battery, low BW | Off | 30 | Balanced | No |
| Companion | Minimal resources | N/A | N/A | N/A | N/A |

```rust
pub struct ConnectionProfile {
    pub name: String,
    pub display: DisplayProfile,
    pub degradation: DegradationConfig,
    pub channel_priorities: Vec<(u16, ChannelPriority)>,
}

pub struct DisplayProfile {
    pub prefer_lossless_text: bool,
    pub max_fps: u8,
    pub region_detection: bool,
    pub encoder_preset: EncoderPreset,
    pub color_space: ColorSpace,
}
```

Client selects during handshake or switches mid-session via Control channel. Profiles pre-configure the degradation ladder, encoder, and arbiter priorities.

---

## 10. Recv Loop

Session Manager owns all receive I/O. Two tasks per connection (datagram + stream), one dispatch point.

```rust
async fn recv_loop(
    conn: &dyn PrismConnection,
    probe_echo_tx: mpsc::Sender<ProbeEcho>,
    channel_dispatcher: &ChannelDispatcher,
    bandwidth_tracker: &ChannelBandwidthTracker,
    heartbeat: &HeartbeatMonitor,
    client_id: ClientId,
) {
    loop {
        tokio::select! {
            Ok(datagram) = conn.recv_datagram() => {
                heartbeat.activity(client_id);
                if let Ok(header) = PrismHeader::decode(&mut datagram.clone()) {
                    bandwidth_tracker.record_recv(header.channel_id, header.payload_length);
                    match (header.channel_id, header.msg_type) {
                        (CHANNEL_CONTROL, MSG_PROBE_RESPONSE) => {
                            probe_echo_tx.try_send(ProbeEcho::from(&header, &datagram)).ok();
                        }
                        _ => {
                            channel_dispatcher.dispatch_datagram(header, datagram).await;
                        }
                    }
                }
            }
            Ok(stream_data) = conn.accept_bi() => {
                heartbeat.activity(client_id);
                // Spawn handler for this stream (long-lived or short-lived)
                let dispatcher = channel_dispatcher.clone();
                tokio::spawn(async move {
                    dispatcher.handle_stream(stream_data).await;
                });
            }
            Ok(uni_data) = conn.accept_uni() => {
                heartbeat.activity(client_id);
                let dispatcher = channel_dispatcher.clone();
                tokio::spawn(async move {
                    dispatcher.handle_uni_stream(uni_data).await;
                });
            }
        }
    }
}
```

Probe echoes route to the ConnectionProber via channel (prober never touches the connection). All other packets dispatch to channel handlers. Per-channel bandwidth tracked via atomic counter.

---

## 11. Client-Side Session

The client needs its own session logic: heartbeats, feedback, overlay rendering, server message handling.

```rust
pub struct ClientSessionState {
    server_caps: ServerCapabilities,
    negotiated_channels: Vec<NegotiatedChannel>,
    channel_assignments: Vec<ChannelAssignment>,
    profile: ConnectionProfile,
    overlay_enabled: bool,
    feedback_config: ClientFeedbackConfig,
    feedback_state: FeedbackState,
}

enum FeedbackState {
    Normal { next_report: Instant },
    Stressed { next_report: Instant },
}

impl ClientSessionState {
    async fn run(
        &mut self,
        control_stream: (FramedWriter, FramedReader),
        conn: &dyn PrismConnection,
    ) {
        let (mut writer, mut reader) = control_stream;
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
        let heartbeat_gen = HeartbeatGenerator::new();

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    // Zero-allocation heartbeat (S11)
                    conn.try_send_datagram(heartbeat_gen.next()).ok();
                }
                Ok(msg) = reader.recv() => {
                    self.handle_server_message(&msg, &mut writer).await;
                }
                _ = self.feedback_timer() => {
                    self.send_feedback(&mut writer).await;
                }
            }
        }
    }

    async fn handle_server_message(&mut self, data: &[u8], writer: &mut FramedWriter) {
        if data.len() < HEADER_SIZE { return; }
        if let Ok(header) = PrismHeader::decode(&mut Bytes::from(data.to_vec())) {
            match header.msg_type {
                control_msg::OVERLAY_DATA => { /* update overlay display */ }
                control_msg::QUALITY_UPDATE => { /* adjust decoders per recommendation */ }
                control_msg::REDUCE_SEND_RATE => { /* reduce input/clipboard send rate */ }
                control_msg::SHUTDOWN_NOTICE => { /* show notification, prepare reconnect */ }
                control_msg::CERT_RENEWAL => { /* store new TLS cert hash (browser) */ }
                control_msg::MONITOR_LAYOUT => { /* update multi-monitor layout */ }
                control_msg::CHANNEL_TRANSFER => { /* handle ownership change */ }
                _ => {}
            }
        }
    }

    fn feedback_timer(&self) -> tokio::time::Sleep {
        let interval = match &self.feedback_state {
            FeedbackState::Normal { .. } => self.feedback_config.normal_interval,
            FeedbackState::Stressed { .. } => self.feedback_config.stressed_interval,
        };
        tokio::time::sleep(interval)
    }
}
```

---

## 12. Graceful Shutdown

### 12.1 Shutdown Sequence

1. Session Manager sends `SHUTDOWN_NOTICE` to all clients (30s warning)
2. Clients show "Server shutting down in 30s" notification
3. Session Manager pauses new channel activations
4. Active FileShare transfers signaled to pause (clients can resume later)
5. After 30s (or all clients disconnect): close all connections
6. Quinn endpoint closes

```rust
pub struct ShutdownNotice {
    pub reason: String,           // "Server restarting", "User initiated"
    pub seconds_remaining: u32,
    pub will_restart: bool,       // client should attempt reconnect
}
```

### 12.2 Tombstone Persistence

If `will_restart` is true, tombstones are persisted to disk so clients can reconnect after server restart.

```rust
impl TombstoneStore {
    pub fn persist(&self, path: &Path) -> Result<(), std::io::Error> {
        let data = serde_json::to_vec(&self.tombstones)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn restore(path: &Path, max_age: Duration) -> Result<Self, std::io::Error> {
        if !path.exists() { return Ok(Self::new(max_age)); }
        let data = std::fs::read(path)?;
        let mut tombstones: HashMap<ClientId, Tombstone> = serde_json::from_slice(&data)?;
        tombstones.retain(|_, t| t.created_at.elapsed() < max_age);
        Ok(Self { tombstones, max_age })
    }
}
```

---

## 13. Observability: prism-metrics

### 8.1 MetricsRecorder (Const-Generic, Compile-Time Safe)

```rust
pub struct MetricsRecorder<const C: usize, const G: usize, const H: usize> {
    counters: [AtomicU64; C],
    gauges: [AtomicI64; G],
    histograms: [AtomicHistogram; H],
    labels: MetricLabels<C, G, H>,
}

pub struct MetricLabels<const C: usize, const G: usize, const H: usize> {
    counter_names: [&'static str; C],
    gauge_names: [&'static str; G],
    histogram_names: [&'static str; H],
}

impl<const C: usize, const G: usize, const H: usize> MetricsRecorder<C, G, H> {
    #[inline(always)]
    pub fn inc(&self, counter: usize, value: u64) {
        self.counters[counter].fetch_add(value, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn set(&self, gauge: usize, value: i64) {
        self.gauges[gauge].store(value, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn observe(&self, histogram: usize, value: u64) {
        self.histograms[histogram].record(value);
    }

    pub fn snapshot(&self) -> RecorderSnapshot {
        RecorderSnapshot {
            counters: self.counters.iter().map(|c| c.load(Ordering::Relaxed)).collect(),
            gauges: self.gauges.iter().map(|g| g.load(Ordering::Relaxed)).collect(),
            histograms: self.histograms.iter().map(|h| h.snapshot()).collect(),
            counter_names: self.labels.counter_names.to_vec(),
            gauge_names: self.labels.gauge_names.to_vec(),
            histogram_names: self.labels.histogram_names.to_vec(),
        }
    }
}

// Compile-time sized per subsystem
type TransportRecorder = MetricsRecorder<6, 3, 2>;
type DisplayRecorder = MetricsRecorder<7, 6, 4>;
type SessionRecorder = MetricsRecorder<6, 2, 0>;
type InputRecorder = MetricsRecorder<3, 0, 1>;
type ClipboardRecorder = MetricsRecorder<4, 0, 2>;
type FileShareRecorder = MetricsRecorder<4, 2, 2>;
```

### 8.2 Lock-Free Histogram

Logarithmic buckets (25 buckets, 1µs to ~34s). O(1) record, bounded memory. Percentile computation from cumulative bucket counts with linear interpolation.

```rust
pub struct AtomicHistogram {
    buckets: [AtomicU64; 25],
    sum: AtomicU64,
    count: AtomicU64,
    min: AtomicU64,
    max: AtomicU64,
}

impl AtomicHistogram {
    #[inline(always)]
    pub fn record(&self, value_us: u64) {
        let bucket = if value_us == 0 { 0 } else {
            (63 - value_us.leading_zeros() as usize).min(24)
        };
        self.buckets[bucket].fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value_us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.update_min(value_us);
        self.update_max(value_us);
    }

    pub fn snapshot(&self) -> HistogramSnapshot {
        let buckets: [u64; 25] = self.buckets.each_ref()
            .map(|b| b.load(Ordering::Relaxed));
        let count = self.count.load(Ordering::Relaxed);
        HistogramSnapshot {
            buckets,
            sum_us: self.sum.load(Ordering::Relaxed),
            count,
            avg_us: if count > 0 { self.sum.load(Ordering::Relaxed) / count } else { 0 },
            min_us: self.min.load(Ordering::Relaxed),
            max_us: self.max.load(Ordering::Relaxed),
            p50_us: Self::percentile(&buckets, count, 0.50),
            p95_us: Self::percentile(&buckets, count, 0.95),
            p99_us: Self::percentile(&buckets, count, 0.99),
        }
    }

    fn percentile(buckets: &[u64; 25], total: u64, pct: f64) -> u64 {
        if total == 0 { return 0; }
        let target = (total as f64 * pct) as u64;
        let mut cumulative = 0u64;
        for (i, &count) in buckets.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                let bucket_start = if i == 0 { 0 } else { 1u64 << i };
                let bucket_end = 1u64 << (i + 1);
                let fraction = if count > 0 {
                    (target - (cumulative - count)) as f64 / count as f64
                } else { 0.0 };
                return bucket_start + ((bucket_end - bucket_start) as f64 * fraction) as u64;
            }
        }
        0
    }
}
```

### 8.3 Rate Counter

Dual-counter with cached rate. Collector computes rate once per second. Overlay reads with one atomic load.

```rust
pub struct RateCounter {
    total: AtomicU64,
    prev_total: AtomicU64,
    prev_timestamp_us: AtomicU64,
    cached_rate: AtomicU64,
}

impl RateCounter {
    #[inline(always)]
    pub fn inc(&self, n: u64) {
        self.total.fetch_add(n, Ordering::Relaxed);
    }

    pub fn compute_rate(&self) { /* called by collector every second */ }

    #[inline(always)]
    pub fn rate(&self) -> u64 {
        self.cached_rate.load(Ordering::Relaxed)
    }
}
```

### 8.4 Per-Subsystem Metric Definitions

Each subsystem defines metrics as compile-time constants. Adding a counter without updating the const-generic type is a compile error.

**Transport:** bytes_sent, bytes_recv, datagrams_sent, datagrams_dropped, streams_opened, streams_closed. Gauges: rtt_us, loss_rate, bandwidth_bps. Histograms: rtt, send_latency.

**Display:** frames_captured, frames_encoded, frames_sent, frames_dropped, keyframes_sent, bytes_encoded, regions_classified. Gauges: current_fps, bitrate, degradation_level, region_count, resolution. Histograms: capture_time, classify_time, encode_time, frame_size.

**Session:** clients_connected/disconnected/reconnected, channel_grants/denials, arbiter_rebalances. Gauges: active_clients, tombstone_count.

**Input:** events_received, events_injected, events_dropped. Histograms: input_rtt.

**Clipboard:** syncs_sent/received, echoes_suppressed, filters_triggered. Histograms: sync_latency, payload_size.

**FileShare:** transfers_started/completed/failed, bytes_transferred. Gauges: throughput_bps, active_transfers. Histograms: transfer_time, chunk_size.

### 8.5 Per-Client Metric Isolation

Multi-client: each client gets its own recorder instances. Collector aggregates across clients for Prometheus (sum counters, max gauges).

```rust
pub struct ScopedRecorder<const C: usize, const G: usize, const H: usize> {
    clients: HashMap<ClientId, Arc<MetricsRecorder<C, G, H>>>,
}
```

Prometheus labels: `prism_display_frames_encoded{client="uuid"}`.

---

## 14. Observability: Frame Tracing

### 9.1 Frame Trace

End-to-end latency decomposition for a single display frame:

```rust
pub struct FrameTrace {
    pub frame_seq: u32,
    pub capture_start_us: u64,
    pub capture_end_us: u64,
    pub classify_end_us: u64,
    pub encode_start_us: u64,
    pub encode_end_us: u64,
    pub send_us: u64,
    pub network_recv_us: Option<u64>,  // client-reported
    pub decode_end_us: Option<u64>,    // client-reported
    pub render_end_us: Option<u64>,    // client-reported
}

pub struct FrameLatencyBreakdown {
    pub capture_us: u64,
    pub classify_us: u64,
    pub encode_us: u64,
    pub send_us: u64,
    pub network_us: Option<u64>,
    pub decode_us: Option<u64>,
    pub render_us: Option<u64>,
    pub total_us: u64,
}
```

### 9.2 Adaptive Trace Sampling

Not every frame is traced. Adaptive sampling ensures slow frames (the ones that matter) are always captured:

```rust
pub struct FrameTracer {
    uniform_rate: u32,               // 60 (once per second)
    uniform_counter: u32,
    slow_frame_threshold_us: u64,    // auto-tuned to p95
    traces_this_second: u32,
    max_traces_per_second: u32,      // 10
    second_start: Instant,
}

impl FrameTracer {
    pub fn should_trace(&mut self, last_frame_total_us: u64) -> bool {
        // Budget check
        if self.traces_this_second >= self.max_traces_per_second { return false; }
        // Always trace slow frames
        if last_frame_total_us > self.slow_frame_threshold_us {
            self.traces_this_second += 1;
            return true;
        }
        // Uniform sampling for baseline
        self.uniform_counter += 1;
        if self.uniform_counter >= self.uniform_rate {
            self.uniform_counter = 0;
            self.traces_this_second += 1;
            return true;
        }
        false
    }

    pub fn update_threshold(&mut self, p95_us: u64) {
        self.slow_frame_threshold_us = p95_us;
    }
}
```

---

## 15. Observability: Client Feedback (R17)

### 10.1 Tiered Feedback Frequency

```rust
pub struct ClientFeedbackConfig {
    pub normal_interval: Duration,              // 1 second
    pub stressed_interval: Duration,            // 200ms
    pub stress_threshold_queue_depth: u8,       // 3 frames
    pub stress_threshold_drop_rate: f32,        // 5%
}
```

Client monitors its own performance. When stressed (decoder queue building, frames dropping), switches to 200ms feedback. Server sees degradation within 200ms.

### 10.2 Feedback Payload

```rust
pub struct ClientFeedback {
    pub avg_decode_us: u64,
    pub avg_render_us: u64,
    pub frames_decoded: u64,
    pub frames_dropped: u64,
    pub frames_late: u64,
    pub decoder_queue_depth: u8,
    pub frame_traces: Vec<FrameTraceResponse>,
}

pub struct FrameTraceResponse {
    pub frame_seq: u32,
    pub network_recv_us: u64,
    pub decode_end_us: u64,
    pub render_end_us: u64,
}
```

### 10.3 Client Alerts (Immediate)

```rust
pub enum ClientAlert {
    DecoderOverloaded { queue_depth: u8, drop_rate: f32 },
    OutOfMemory,
    DisplayChanged { new_resolution: (u32, u32), new_scale: f32 },
}
```

Sent immediately, does not wait for feedback interval. Server reacts within one frame.

---

## 16. Observability: Collection & Export

### 11.1 MetricsCollector

```rust
pub struct MetricsCollector {
    recorders: Vec<(&'static str, Arc<dyn Observable>)>,
    frame_traces: RingBuffer<FrameLatencyBreakdown>,
    time_series: MetricsTimeSeries,
    collect_interval: Duration,   // 1 second
}
```

### 11.2 Time-Series History

Per-metric ring buffer. 300 samples at 1/sec = 5 minutes of history. ~240KB total memory.

```rust
pub struct MetricsTimeSeries {
    series: HashMap<MetricId, TimeSeriesRing>,
    sample_interval: Duration,   // 1 second
    max_samples: usize,          // 300
}

pub struct TimeSeriesRing {
    samples: VecDeque<TimeSample>,
    max_len: usize,
}
```

Supports sparkline graphs in client overlay: RTT, FPS, bitrate over last 5 minutes.

### 11.3 Client Overlay

Zero-copy 128-byte binary packet sent every 100ms (10fps) when overlay enabled:

```rust
#[repr(C, packed)]
pub struct OverlayPacket {
    pub fps: u8,
    pub degradation_level: u8,
    pub active_clients: u8,
    pub transport_type: u8,
    pub codec: [u8; 4],
    pub resolution_w: u16,
    pub resolution_h: u16,
    pub bitrate_kbps: u32,
    pub rtt_us: u32,
    pub loss_rate_permille: u16,
    pub capture_us: u32,
    pub classify_us: u32,
    pub encode_us: u32,
    pub send_us: u32,
    pub network_us: u32,
    pub decode_us: u32,
    pub render_us: u32,
    pub total_us: u32,
    pub display_kbps: u32,
    pub input_kbps: u16,
    pub audio_kbps: u16,
    pub fileshare_kbps: u32,
    pub total_kbps: u32,
    pub available_kbps: u32,
    pub _reserved: [u8; 56],
}
// Total: 128 bytes. memcpy, no serialization.
```

### 11.4 Prometheus Export (Optional)

HTTP endpoint at `localhost:9090/metrics`. Converts `RecorderSnapshot` to Prometheus text format on each scrape. Per-client labels.

---

## 17. Phase Mapping

| Component | Phase 1 | Phase 2 | Phase 3 | Phase 4 |
|-----------|---------|---------|---------|---------|
| Session Manager | Single-client (data structures support N) | No change | Multi-client active | No change |
| Routing Table | ArcSwap, lock-free reads | No change | No change | No change |
| Channel Ownership | Exclusive + Shared (single client) | No change | + Transferable active | No change |
| Capability Negotiation | R1 extensible tuples | No change | No change | No change |
| Reconnection | Tombstones, per-channel recovery | No change | No change | No change |
| Profiles | Gaming, Coding (2 profiles) | + Media | + Mobile, Companion | No change |
| Bandwidth Arbiter | Single-client, event-driven | No change | + Multi-client fairness | No change |
| Allocation Handles | Zero-cost atomic reads | No change | No change | No change |
| Bandwidth Tracker | Per-channel atomic counters | No change | No change | No change |
| Server BW Discovery | Speed test on first client | No change | No change | No change |
| prism-metrics | Full: const-generic recorders, histograms | No change | No change | No change |
| Frame Tracing | Adaptive sampling, server-side only | + Client feedback merging | No change | No change |
| Client Feedback | Tiered frequency + alerts | No change | No change | No change |
| Time-Series | 5-minute history | No change | No change | No change |
| Client Overlay | 128-byte binary packet | No change | No change | No change |
| Prometheus Export | Optional | No change | No change | No change |
| Capability Negotiation | Extensible intersection | No change | No change | No change |
| Control Protocol | 18 message types | No change | No change | No change |
| Channel Dispatch | Handler registration + dispatch | No change | No change | No change |
| Client Session | Heartbeat + feedback + overlay | No change | No change | No change |
| Graceful Shutdown | Tombstone persistence | No change | No change | No change |

---

## 18. File Layout

```
crates/prism-metrics/
    Cargo.toml
    src/
        lib.rs                  # re-exports, Observable trait
        recorder.rs             # MetricsRecorder<C,G,H>, const-generic
        histogram.rs            # AtomicHistogram, HistogramSnapshot, percentiles
        rate.rs                 # RateCounter (dual-counter with cached rate)
        snapshot.rs             # RecorderSnapshot (type-erased)

crates/prism-session/
    Cargo.toml
    src/
        lib.rs                  # re-exports
        manager.rs              # SessionManager core
        session.rs              # ClientSession, SessionState
        routing.rs              # RoutingTable, RoutingSnapshot, RouteEntry (ArcSwap)
        channel.rs              # ChannelRegistry, ChannelOwnership, ChannelGrantResult
        lifecycle.rs            # Connection flow, state machine transitions
        tombstone.rs            # TombstoneStore, Tombstone, ChannelRecoveryState
        heartbeat.rs            # HeartbeatMonitor, HeartbeatState
        profiles.rs             # ConnectionProfile, DisplayProfile, defaults
        recv_loop.rs            # Recv loop, dispatch, channel routing
        arbiter/
            mod.rs              # BandwidthArbiter
            allocation.rs       # ChannelAllocation, AllocationHandle, AllocationResult
            budget.rs           # ClientBudget, ServerBandwidthProfile
            needs.rs            # BandwidthAware trait, BandwidthNeeds
            reservation.rs      # ReservationRequest, predictive allocation
            starvation.rs       # StarvationDetector, StarvationWarning
            tracker.rs          # ChannelBandwidthTracker (atomic counters)
        events.rs               # SessionEvent, ArbiterEvent
        negotiation.rs          # CapabilityNegotiator, NegotiationResult
        control_msg.rs          # Control channel message type registry
        dispatch.rs             # ChannelDispatcher, handler registration
        shutdown.rs             # ShutdownNotice, graceful shutdown
        client_session.rs       # ClientSessionState (client-side logic)

crates/prism-observability/
    Cargo.toml
    src/
        lib.rs                  # re-exports
        collector.rs            # MetricsCollector, SystemSnapshot
        time_series.rs          # MetricsTimeSeries, TimeSeriesRing, TimeSample
        frame_trace.rs          # FrameTrace, FrameLatencyBreakdown, FrameTracer
        feedback.rs             # ClientFeedback, ClientFeedbackConfig, ClientAlert
        overlay.rs              # OverlayPacket, OverlayData
        export/
            mod.rs              # export trait
            prometheus.rs       # PrometheusExporter
```

---

## 19. Testing Strategy

| Category | What | How |
|----------|------|-----|
| Unit: ChannelRegistry | Request/release/transfer for all ownership types | Table-driven: exclusive conflict, shared fan-out, transfer |
| Unit: RoutingTable | Snapshot consistency under concurrent read/write | Spawn 10 readers + 1 writer, verify no torn reads |
| Unit: Tombstone | Create, claim, expire, GC | Time-controlled tests |
| Unit: Heartbeat | Activity resets timer, suspend/tombstone thresholds | Mock clock |
| Unit: Arbiter allocation | Min guarantees, priority weighting, reservation preemption | Known budgets, verify allocations |
| Unit: Arbiter starvation | Detect starved channel, emit warning | Feed low actual_usage with high allocated |
| Unit: AllocationHandle | Atomic read/write consistency | Concurrent read/write, verify eventual consistency |
| Unit: BandwidthTracker | Per-channel counting, snapshot_and_reset | Feed packets, verify per-channel BPS |
| Unit: MetricsRecorder | Inc/set/observe, snapshot, compile-time bounds | Out-of-bounds is compile error |
| Unit: Histogram | Record, percentiles, min/max, edge cases | Known distributions, verify p50/p95/p99 |
| Unit: RateCounter | Rate computation accuracy | Feed known events/sec, verify cached_rate |
| Unit: FrameTracer | Uniform sampling, slow-frame capture, budget | Feed frame times, verify trace decisions |
| Unit: OverlayPacket | Size = 128, field offsets | static_assert, transmute roundtrip |
| Integration: Session lifecycle | Connect, authenticate, negotiate, active, disconnect, tombstone, reconnect | Two endpoints, full flow |
| Integration: Multi-client | Phone + laptop connect, channel ownership, clipboard fan-out | Three endpoints |
| Integration: Channel transfer | Laptop has display, phone takes over | Two clients, verify transfer event |
| Integration: Arbiter + FileShare | Start transfer, verify display bitrate reduced, transfer completes, bitrate restores | Concurrent display + file |
| Integration: Reconnection | Disconnect, reconnect within tombstone, verify channel recovery | Network interruption sim |
| Integration: Client feedback | Client sends stressed feedback, server reduces quality | Inject slow decode times |
| Integration: Frame trace | Full pipeline trace, client reports back, breakdown computed | End-to-end with mock display |
| Perf: Routing table read | Concurrent snapshot reads under write pressure | Benchmark, verify > 10M reads/sec |
| Perf: AllocationHandle read | Cost of allocated_bps() | Benchmark, verify < 5ns |
| Perf: BandwidthTracker | record_send cost | Benchmark, verify < 5ns |
| Perf: MetricsRecorder | inc/set/observe cost | Benchmark, verify < 5ns each |
| Perf: Histogram record | Cost including min/max CAS | Benchmark, verify < 10ns |
| Perf: Collector snapshot | All subsystem snapshots + time-series record | Benchmark, verify < 100us |
| Unit: Negotiation | Phase 3 client → Phase 1 server, codec priority | Verify intersection, codec fallback |
| Unit: Control msg | All 18 message types parse correctly | Round-trip encode/decode |
| Unit: Dispatcher | Register handler, dispatch datagram, dispatch stream | Verify correct handler called |
| Unit: Shutdown | Shutdown notice, tombstone persist/restore | Verify clients notified, tombstones survive |
| Unit: Batch routing | 6 AddRoutes = 1 swap, RemoveClient cleans all | Verify generation increments once |
| Integration: Client session | Client heartbeat, feedback tiering, overlay toggle | Two endpoints, verify message flow |
| Integration: Graceful shutdown | Server shuts down, client reconnects | Tombstone persist + restore |
| Integration: Capability negotiation | Mismatched versions, missing channels | Verify intersection behavior |

---

## 20. Optimizations Index

| ID | Optimization | Impact | Phase |
|----|-------------|--------|-------|
| S1 | ArcSwap routing table (lock-free reads) | ~5ns per-frame routing lookup | 1 |
| S2 | AllocationHandle (shared atomics) | ~1ns allocation check per frame | 1 |
| S3 | Per-channel atomic bandwidth counters | ~1ns per-packet tracking | 1 |
| S4 | Event-driven arbiter rebalance | ~16ms response vs 100ms periodic | 1 |
| S5 | Predictive reservation (R47) | 0ms congestion onset vs ~500ms reactive | 1 |
| S6 | Keyframe hints to arbiter | Burst accommodation without congestion spike | 1 |
| S7 | Frame preview for content-aware allocation | Pre-adapt before encoding | 1 |
| M1 | RateCounter with cached rate | ~1ns overlay read vs ~10ns division | 1 |
| M2 | Per-client metric isolation | No cross-client interference | 1 |
| M3 | Zero-copy 128-byte overlay packet | No serialization, memcpy only | 1 |
| M4 | Const-generic MetricsRecorder | Compile-time bounds checking | 1 |
| M5 | Lock-free histogram with log buckets | ~5ns record, O(1) percentiles | 1 |
| M6 | Adaptive frame trace sampling | Always captures slow frames | 1 |
| M7 | Tiered client feedback (1s normal, 200ms stressed) | 200ms degradation detection | 1 |
| M8 | Time-series ring buffers (5min history) | Sparkline overlay, 240KB memory | 1 |
| S8 | Single-client routing cache (no atomic) | ~1ns reads in Phase 1 | 1 |
| S9 | Batch routing mutations (single swap) | 6x fewer Arc allocations on connect | 1 |
| S10 | Skip inactive clients in arbiter tick | Tick cost proportional to active clients | 1 |
| S11 | Pre-built heartbeat packets | Zero-allocation heartbeat path | 1 |
| S12 | ConnectionQuality caching (ArcSwap) | ~1ns quality read vs ~5µs recompute | 1 |

---

*PRISM Session Manager + Observability Design v1.0 — CC0 Public Domain*
