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
4. [Connection Lifecycle & Reconnection](#4-connection-lifecycle--reconnection)
5. [Bandwidth Arbiter](#5-bandwidth-arbiter)
6. [Connection Profiles](#6-connection-profiles)
7. [Recv Loop & Dispatch](#7-recv-loop--dispatch)
8. [Observability: prism-metrics](#8-observability-prism-metrics)
9. [Observability: Frame Tracing](#9-observability-frame-tracing)
10. [Observability: Client Feedback](#10-observability-client-feedback)
11. [Observability: Collection & Export](#11-observability-collection--export)
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

## 4. Connection Lifecycle & Reconnection

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

## 5. Bandwidth Arbiter

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

## 6. Connection Profiles (R39)

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

## 7. Recv Loop & Dispatch

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

## 8. Observability: prism-metrics

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

## 9. Observability: Frame Tracing

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

## 10. Observability: Client Feedback (R17)

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

## 11. Observability: Collection & Export

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

## 12. Phase Mapping

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

---

## 13. File Layout

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

## 14. Testing Strategy

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

---

## 15. Optimizations Index

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

---

*PRISM Session Manager + Observability Design v1.0 — CC0 Public Domain*
