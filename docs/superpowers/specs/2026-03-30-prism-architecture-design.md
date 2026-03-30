# PRISM — Overall Architecture Design

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-30                     |
| Authors     | Ehsan + Claude                 |
| PRD ref     | docs/PRD.md                    |
| TRD ref     | docs/TRD.md                    |

This document defines the overall architecture for PRISM: how subsystems connect, shared abstractions, boundaries, the dependency graph, and cross-cutting concerns. Each subsystem gets its own detailed spec; this document is the contract between them.

The design is scenario-driven: 9 critical scenarios surface 48 architectural requirements. A reference section summarizes subsystem contracts for implementors.

---

## Table of Contents

1. [Revised Packet Header](#1-revised-packet-header)
2. [Critical Scenarios](#2-critical-scenarios)
3. [Subsystem Boundaries & Contracts](#3-subsystem-boundaries--contracts)
4. [Cross-Cutting Concerns](#4-cross-cutting-concerns)
5. [Hyper-Optimizations](#5-hyper-optimizations)
6. [Requirements Index](#6-requirements-index)
7. [Subsystem Spec Roadmap](#7-subsystem-spec-roadmap)

---

## 1. Revised Packet Header

The original TRD specifies a 12-byte header. This revision addresses two gaps: no sequence numbers for datagram ordering, and only 8-bit channel IDs (16 extension slots). The revised header remains 16 bytes — well under RDP's 20-30 byte overhead.

```
┌──────────────────┬──────────┬──────────┬──────────┬──────────────┬────────────────┐
│ 4 bits │ 12 bits  │ 8 bits   │ 16 bits  │ 32 bits  │ 32 bits      │ 32 bits        │
│ Ver    │ Channel  │ Msg Type │ Flags    │ Sequence │ Timestamp us │ Payload Length │
│        │ ID       │          │          │          │              │                │
└──────────────────┴──────────┴──────────┴──────────┴──────────────┴────────────────┘
Total: 16 bytes. Little-endian wire format.
```

### 1.1 Field Definitions

**Version (4 bits):** Protocol version. `0` = v1. Allows distinguishing header formats across protocol generations. Top 4 bits of the first 16-bit word.

**Channel ID (12 bits):** 4096 channels. Core channels 0x001-0x0FF. Mobile extensions 0x0E0-0x0EF. User extensions 0x100-0xFFF. Channel 0x000 is reserved (invalid — a zeroed header is always detectable as corrupt).

**Msg Type (8 bits):** Channel-specific message type. Each channel defines its own message type enum.

**Flags (16 bits):**

| Bit | Name | Meaning |
|-----|------|---------|
| 0 | `KEYFRAME` | Complete state (display IDR, clipboard full sync) |
| 1 | `PRIORITY` | High-priority delivery (input events, cursor updates) |
| 2 | `COMPRESSED` | Payload uses channel-specific compression |
| 3-15 | Reserved | Must be 0; available for future use |

Fragmentation flags are removed — QUIC handles stream framing natively. For datagrams, payloads that exceed QUIC's max datagram size (~1200 bytes minus header) spill to a short-lived reliable stream. No application-level fragmentation.

**Sequence (32 bits):** Per-channel monotonic counter. Wraps at 2^32. Purpose varies by transport:
- **Datagrams (Input, Display P-frames, Audio, Sensors):** Receiver discards out-of-order packets. A mouse event with seq 41 arriving after seq 42 is stale — drop it.
- **Reliable streams (FileShare, Clipboard, Control):** Redundant for ordering (QUIC guarantees it) but useful for debugging, logging, and cross-channel correlation.

**Timestamp us (32 bits):** Microsecond-precision relative timer. Epoch resets per session. Wraps every ~71 minutes — acceptable for RTT measurement (only deltas between recent timestamps matter). Channels needing wall-clock time (Notifications, FileShare metadata) carry it in the payload.

**Payload Length (32 bits):** Byte length of the payload following the header.

### 1.2 Wire Format Decisions

**Little-endian:** Both endpoints are almost certainly LE (x86 desktops, ARM phones, WASM). Avoids byte swapping on 95%+ of target hardware. `u32::to_le_bytes()` / `from_le_bytes()` is zero-cost on LE architectures.

**No fragmentation at PRISM level:** QUIC streams handle fragmentation and reassembly. For datagrams, enforce that payloads fit within QUIC's max datagram frame or promote to a reliable stream. This avoids reimplementing reliability poorly.

**Channel 0x000 reserved:** Zero-value is a common uninitialized-memory bug. A zeroed header routes to no valid channel.

---

## 2. Critical Scenarios

Nine scenarios that collectively drive every architectural decision. Each surfaces numbered requirements (R1-R48).

### Scenario 1: First Connection (Laptop to Desktop over LAN)

**Setup:** Windows desktop and laptop (Rust client). Both on the same Tailscale tailnet, never connected before.

```
Laptop                                                Desktop
  |                                                      |
  |  1. Resolve desktop's Tailscale IP (100.x.y.z)      |
  |                                                      |
  |  2. -- QUIC Initial + Noise NK Handshake ----------> |
  |        TLS ClientHello with ALPN "prism/1"           |
  |        + ephemeral key, encrypted static key,        |
  |          capability message                          |
  |                                                      |
  |        Capability msg contains:                      |
  |          - Protocol version: 1                       |
  |          - Channels: [(0x001, v1, config), ...]      |
  |          - Codecs: [H264, H265, AV1]                 |
  |          - Display: 2560x1440@60                     |
  |          - Performance profile                       |
  |          - Input devices: [keyboard, mouse]          |
  |                                                      |
  |  3. Server validates client pubkey against allowlist  |
  |     Unknown key: silent drop (no response)           |
  |     Known key: continue                              |
  |                                                      |
  |  4. <-- Handshake Response -------------------------  |
  |        Server capabilities                           |
  |        Negotiated codec: H265                        |
  |        Display: 2560x1440@60 (primary)               |
  |        Channel assignments (stream/datagram IDs)     |
  |        First IDR keyframe (embedded)                 |
  |                                                      |
  |  5. Client decodes keyframe: pixels on screen        |
  |     Total: 1 RTT (~40ms on 20ms network)             |
  |                                                      |
  |  6. == Steady state ==============================   |
  |     Display: P-frames as datagrams, IDRs on stream   |
  |     Input: keyboard/mouse as datagrams               |
  |     Control: heartbeat every 5s on stream            |
```

**Requirements surfaced:**

- **R1: Extensible capability negotiation.** Capability message is a list of `(channel_id, channel_version, channel_config)` tuples. Not a flat struct that changes every phase. Phase 1 negotiates codecs and display. Phase 3 adds clipboard. Phase 4 adds mobile. Same message format throughout.
- **R2: Server-authoritative channel assignments.** Server decides which QUIC stream IDs and datagram flows map to which PRISM channels. Client reads the assignment table from the handshake — never assumes channel 0x001 = stream 0.
- **R3: Silent drop indistinguishable from "host doesn't exist."** No ICMP unreachable, no TLS alert, no QUIC CONNECTION_CLOSE. The QUIC listener validates the Noise NK initial before sending any response.
- **R4: Direct LAN transport follows the same sequence.** The only difference is IP/key discovery (manual config vs Tailscale identity). `PrismTransport` trait produces a `QuicConnection` — everything above that is identical.

### Scenario 2: Reconnection After Network Drop

**Setup:** Active coding session. Wi-Fi drops for 3 seconds, then recovers.

```
Laptop                                                Desktop
  |                                                      |
  |  == Active session (display + input) ==============  |
  |                                                      |
  |  X Wi-Fi drops (t=0) X                               |
  |                                                      |
  |  Server-side (during outage):                        |
  |    Display: encoder runs, ring buffer last 500ms     |
  |    Input: nothing to buffer (client-originated)      |
  |    Clipboard: queues changes (small, rare)           |
  |    FileShare: pauses, chunk ACKs stop                |
  |    Control: heartbeat timeout counting               |
  |                                                      |
  |  Wi-Fi recovers (t=3s)                               |
  |                                                      |
  |  -- QUIC 0-RTT Resumption ----------------------->   |
  |     Cached TLS + Noise session                       |
  |     Client sends: last display seq, last clip seq    |
  |                                                      |
  |  <-- Recovery Response ----------------------------  |
  |     Fresh IDR keyframe (display clean slate)         |
  |     Clipboard: delta since client's last seq         |
  |     FileShare: resume ACK (bytes confirmed)          |
  |     Channel assignments: unchanged                   |
  |                                                      |
  |  Client decodes IDR: screen is current               |
  |  FileShare resumes from last ACK'd chunk             |
  |  Total: 0-RTT + IDR decode                           |
```

**Requirements surfaced:**

- **R5: Per-channel recovery state.** Each channel defines reconnection behavior:
  - Display: IDR keyframe (full reset). No replay.
  - Input: No recovery. Client starts sending.
  - Clipboard: Replay from client's last known sequence. Server maintains short changelog.
  - FileShare: Resume from last ACK'd byte offset. Both sides persist transfer state.
  - Audio: Reset. Opus decoder state flushed.
  - Control: Re-exchange capabilities if version supports hot-upgrade.
- **R6: Server-side frame buffer policy.** Ring buffer of ~500ms. Older frames dropped. On reconnection, only latest IDR sent. Caps server memory during extended outages.
- **R7: Heartbeat timeout produces a session tombstone.** Client doesn't reconnect within 60s (configurable) — server tears down session but keeps a tombstone (session ID + last state) for 5 more minutes. Reconnection within tombstone window gets "session expired, full re-handshake required" instead of silent drop.
- **R8: Connection migration is distinct from reconnection.** QUIC connection migration (IP change) is seamless — no session interruption. Network drop requires 0-RTT resume. Client handles both paths.

### Scenario 3: Phone Joins Active Laptop Session (Multi-Client)

**Setup:** Laptop connected for display remoting. Android phone wants clipboard sync and notifications without disconnecting the laptop.

```
Laptop ====== Display + Input ====== Desktop
                                        |
Phone                                   |
  |                                     |
  |  1. -- QUIC Handshake ----------->  |
  |     Channels: [Clipboard, Notify,   |
  |       FileShare]                    |
  |     No display. Touch: Companion.   |
  |                                     |
  |  2. Server: multi-client allowed.   |
  |     Laptop keeps: Display, Input    |
  |     Phone gets: Clipboard, Notify,  |
  |       FileShare                     |
  |     Shared: Clipboard (both get it) |
  |                                     |
  |  3. <-- Handshake Response -------  |
  |     Channel assignments             |
  |     Clipboard: full current state   |
  |     Notify: filter config           |
  |                                     |
  |  == Steady state ================   |
  |  Clipboard syncs to BOTH clients    |
  |  Notifications forward to phone     |
  |                                     |
  |  4. User copies on desktop          |
  |     -> Clipboard to laptop          |
  |     -> Clipboard to phone           |
  |     Both have it within 500ms       |
```

**Requirements surfaced:**

- **R9: Multi-client is first-class.** Server maintains a session per client. Each client subscribes to channels independently. Server routes data to all subscribers.
- **R10: Channel ownership model.** Some channels are exclusive, others shared:
  - **Exclusive:** Display, Input, Camera (one consumer at a time)
  - **Shared:** Clipboard, Notify, FileShare, Sensor, Control (fan out to all)
  - **Transferable:** Phone can take over Display from laptop via Control message. Laptop gets "display transferred" notification.
- **R11: Clipboard fan-out with source tagging.** Each update carries `source_device_id` so the originator suppresses its own echo. Replaces hash-based dedup (which has race conditions) with deterministic source filtering.
- **R12: Per-client notification routing.** Phone notifications go to desktop (always) and laptop (only if active display client). Prevents duplicate popups. Routing is server-side.

### Scenario 4: File Transfer During Active Display Streaming

**Setup:** Laptop connected with display streaming. User sends a 2GB video to the phone (Companion mode).

```
Desktop                          Phone (Companion mode)
  |                                       |
  |  1. -- FileShare::Offer ----------->  |
  |     {name: "video.mp4", size: 2GB}   |
  |                                       |
  |  2. <-- FileShare::Accept ----------  |
  |     {destination: "Downloads/"}       |
  |                                       |
  |  3. == Chunked transfer on stream 7 = |
  |     64KB chunks, growing to 1MB       |
  |                                       |
  |  Meanwhile, same connection:          |
  |  Laptop == Display datagrams ======   |
  |  Laptop == Input datagrams ========   |
  |  QUIC multiplexing: no HOL blocking   |
  |                                       |
  |  4. Bandwidth arbiter detects file    |
  |     transfer consuming 80% of link.   |
  |     Pre-reduces display bitrate 40%.  |
  |     Paces FileShare to leave room.    |
  |                                       |
  |  5. Transfer completes                |
  |     -- FileShare::Complete -------->  |
  |     Display bitrate restores          |
```

**Requirements surfaced:**

- **R13: Bandwidth arbiter.** Server-side component monitoring total link utilization across all channels and clients. Adjusts display bitrate and FileShare pacing. Uses QUIC RTT estimates and ACK timing as signals.
- **R14: Channel priority enforcement.** Priority order: Input > Display > Audio > Control > Clipboard > FileShare > Device. Lower-priority channels yield when bandwidth is scarce.
- **R15: FileShare adapts to available bandwidth.** Chunk sizing accounts for concurrent display streaming. Initial size based on available bandwidth, not raw link speed.

### Scenario 5: Network Degradation (LAN to Cellular)

**Setup:** Laptop on Tailscale. User leaves office — LAN (2ms RTT, 1Gbps) transitions to cellular (80ms RTT, 20Mbps, 2% loss).

```
Phase 1: LAN (2ms RTT)
  H.265, 2560x1440@60, 20 Mbps, <16ms latency

Phase 2: QUIC connection migration (Wi-Fi to cellular)
  IP changes, QUIC migrates seamlessly
  RTT: 2ms -> 80ms. Bandwidth: 1Gbps -> 20Mbps. Loss: 0% -> 2%

Phase 3: Degradation ladder activates
  Step 1: Reduce bitrate (20Mbps -> 5Mbps)
  Step 2: Reduce resolution (1440p -> 1080p -> 720p)
  Step 3: Reduce framerate (60 -> 30 -> 15 fps)
  Step 4: Increase keyframe interval
  Step 5: Switch codec (H.265 -> H.264 if decode faster)
  Step 6: Disable region detection (simpler encode)
  Step 7: Pause non-essential channels (FileShare, Device)
  Step 8: Clipboard to on-demand only
  Step 9: Last resort: audio-only (screen frozen, input flows)

Phase 4: Stabilized on cellular
  H.265, 1080p@30, 4Mbps, ~90ms latency
```

**Requirements surfaced:**

- **R16: Explicit degradation ladder.** Defined sequence with trigger thresholds (RTT, loss, bandwidth) and recovery conditions. Hysteresis prevents flapping — must hold above threshold for N seconds before stepping back up.
- **R17: Client-side quality feedback.** Client reports decode time, frame drop rate, render latency via Control channel. Server can't observe client-side performance. Both network metrics and client feedback drive degradation.
- **R18: Encoder reconfiguration speed.** Bitrate changes are cheap (dynamic, no reinit). Resolution changes require new encoder session + IDR. Codec changes require full pipeline reconfiguration. Adaptation logic knows which changes are cheap vs expensive.
- **R19: User override.** User can pin quality settings ("always 1080p60") or set minimum quality ("never below 720p"). Degradation ladder respects these as constraints.

### Scenario 6: Region Detection and Mixed-Content Display

**Setup:** VS Code (text), YouTube in Chrome (video), and Slack (mixed) on a 4K monitor.

```
+--------------------------------------------------+
|                    4K Desktop                      |
|  +-----------------+  +-----------------------+   |
|  |   VS Code        |  |   Chrome              |   |
|  |  [TEXT REGION]   |  |  [VIDEO REGION]       |   |
|  |  Lossless QOI    |  |  H.265 8Mbps 60fps   |   |
|  |  ~200 KB/frame   |  |  [TEXT: address bar]  |   |
|  |  Only on change  |  |                       |   |
|  +-----------------+  +-----------------------+   |
|  +------------------------------------------------+  |
|  |   Slack                                       |  |
|  |   [STATIC: sidebar] [TEXT: messages]          |  |
|  |   [IMAGE: inline preview, video-encode]       |  |
|  +------------------------------------------------+  |
|  [STATIC: taskbar, cached, near-zero bandwidth]   |
+--------------------------------------------------+
```

**Two-tier detection:**

**Tier 1 (Phase 1): Window-level classification.** WGC per-window capture. Classify entire windows by update frequency. VS Code: low update → text. Chrome with video: high update → video. No computer vision needed — just frame diff rate per window.

**Tier 2 (Phase 2+): Sub-window region detection.** Within a window, divide into 64x64 blocks. Motion detection via GPU compute shader (see H9). Text heuristic: high contrast, limited palette, horizontal clustering. **Fallback bias: lossless.** Uncertain regions default to lossless — wastes bandwidth but never blurs text.

**Requirements surfaced:**

- **R20: Region map is first-class.** Each display frame includes a region map: list of `(rect, encoding, decoder_slot)` tuples. Client uses this to route regions to decoders.
- **R21: Fixed decoder pool on client.** N decoder instances (e.g., 2 video + 1 lossless). Regions assigned to slots. Decoders reused, not created/destroyed per frame. Pool size negotiated during handshake.
- **R22: Lossless-by-default for uncertain regions.** Below confidence threshold, encode lossless. Blurred text is worse UX than extra bandwidth on a UI element.
- **R23: Damage rects for static regions.** Static regions sent once and cached. Only changed pixels sent. Client composites cached + fresh regions. Typical desktop: 40-60% static pixels → 40-60% bandwidth savings.

### Scenario 7: Headless Server (No Monitor)

**Setup:** Cloud VM or home lab with no physical display. Client connects requesting 2560x1440@120.

```
Client                                        Headless Server
  |                                                 |
  |  -- Handshake: display 2560x1440@120 -------->  |
  |                                                 |
  |  Server: no physical monitor detected           |
  |  Creates virtual display via IDD (Windows)      |
  |  Resolution matches client request              |
  |                                                 |
  |  WGC/DDA captures from virtual display          |
  |  Everything else identical to Scenario 1        |
```

**Requirements surfaced:**

- **R24: Virtual display creation on demand.** Server detects "no monitor" and creates virtual display matching client's request. Windows: IDD. Linux: virtual KMS framebuffer. macOS: CGVirtualDisplay (macOS 14+).
- **R25: Multi-client virtual displays.** Two clients with different resolutions → two virtual displays, each with own capture-encode pipeline. Combined with R9 (multi-client), a single server hosts multiple independent sessions.

### Scenario 8: Hostile Network (Corporate Firewall, Lossy Wi-Fi)

**Setup:** UDP blocked entirely, or 5-10% packet loss.

```
Case A: UDP blocked
  Transport probe cascade:
  1. QUIC/UDP -> blocked
  2. QUIC/UDP:443 -> blocked
  3. Tailscale DERP relay (HTTPS 443, TCP) -> works
  4. No Tailscale: WebSocket/TCP:443 -> works (highest latency)

Case B: 5-10% packet loss
  1. Loss detected via ACK gaps
  2. Increase IDR frequency (every 500ms vs 5s)
  3. Input: already loss-tolerant (latest-value-wins)
  4. Loss > 8%: enable FEC (20% redundancy)
  5. Loss > 15%: display to reliable stream (higher latency, no artifacts)
```

**Requirements surfaced:**

- **R26: TCP fallback transport.** When UDP is blocked, PRISM functions over TCP. Tailscale DERP (already TCP) or raw WebSocket tunneling. Higher latency but working.
- **R27: Automatic transport probing.** Try transports in order: QUIC/UDP → QUIC/UDP:443 → DERP → WebSocket/TCP. 500ms timeout per attempt. User doesn't configure transport manually.
- **R28: FEC as degradation step.** Triggered by loss rate, not manually enabled. Reed-Solomon or XOR-based, 5-25% redundancy ratio. Part of the degradation ladder, not a separate Phase 4 feature.

### Scenario 9: Browser Client on Underpowered Hardware

**Setup:** 2018 Chromebook or iPad. Limited hardware decode, weak CPU.

```
Chromebook (2018):
  HW decode: H.264 only
  CPU: too slow for software H.265
  GPU: limited compositing

Negotiation:
  1. VideoDecoder.isConfigSupported("hev1") -> false
  2. VideoDecoder.isConfigSupported("avc1") -> true
  3. H.264, single decoder, no region split
  4. Full-frame encoding, no region detection
  5. 1080p@30 max

Result: works like standard RDP quality. Better than nothing.
```

**Requirements surfaced:**

- **R29: Client performance profile in handshake.** Beyond codecs: max decoder instances, estimated decode throughput, region compositing capability. Server adapts encoding to weakest link.
- **R30: "Simple mode" server-side.** Low-capability client → single-stream full-frame encoding. No region detection, no multi-decoder, no compositor. Always-available lowest common denominator.

---

## 3. Subsystem Boundaries & Contracts

### 3.1 Subsystem Map

```
+------------------------------------------------------------------+
|                        PRISM Server                               |
|                                                                   |
|  +--------------+  +--------------+  +--------------------------+ |
|  |  Transport   |  |  Session     |  |  Platform Services       | |
|  |              |  |  Manager     |  |                          | |
|  |  - QUIC      |  |  (ctrl only) |  |  - Audio (Opus)          | |
|  |  - Tailscale |  |              |  |  - Clipboard (sync)      | |
|  |  - Direct    |  |  - Multi-    |  |  - FileShare             | |
|  |  - TCP fbk   |  |    client    |  |  - USB/IP                | |
|  |  - Probing   |  |  - Channel   |  |  - Notifications         | |
|  |              |  |    ownership |  |  - Camera, Sensor, Touch | |
|  |              |  |  - BW arbiter|  |                          | |
|  +--------------+  +--------------+  +--------------------------+ |
|                                                                   |
|  +-------------------------------+  +---------------------------+ |
|  |  Display Engine               |  |  Security                 | |
|  |                               |  |                           | |
|  |  - Capture (WGC/DDA/SCK/PW)  |  |  - Noise NK handshake    | |
|  |  - Region classifier (GPU)   |  |  - Key management        | |
|  |  - Encoder pool (parallel)   |  |  - Allowlist             | |
|  |  - Frame pipeline (ring buf) |  |  - Content filters       | |
|  |  - Degradation ladder        |  |  - Transport probing     | |
|  |  - Virtual display (headless)|  |                           | |
|  +-------------------------------+  +---------------------------+ |
|                                                                   |
|  +--------------------------------------------------------------+ |
|  |  Observability                                                | |
|  |  - Per-frame latency breakdown    - Metrics ring buffer      | |
|  |  - Client overlay data            - Optional Prometheus/StatsD| |
|  +--------------------------------------------------------------+ |
+------------------------------------------------------------------+
```

### 3.2 Data Flow: Control Plane / Data Plane Split

The Session Manager is control plane only. Latency-critical data goes directly from producer to transport via a shared routing table. (R37)

```
Control Plane (infrequent):
  Session Manager writes routing table
  Handles: connections, disconnections, ownership transfers,
           capability negotiation, arbiter decisions

Data Plane (per-frame, per-event):
  Producers (Display Engine, Platform Services) read routing table
  Write directly to Transport send buffers
  Security filters are inlined in RouteEntry (no extra hop)
  No intermediate routing through Session Manager

  Inbound path:
    Transport -> Security Gate (auth + filter) -> Session Manager (route)
    -> dispatched to appropriate ChannelHandler

  Outbound path (latency-critical):
    Producer -> Routing Table (read) -> Security filter (inline) -> Transport
    (Session Manager not in path)

                   +---------------------+
                   |   Transport         |
                   |   (QUIC/DERP/WS)    |
                   +----------^----------+
                              | outbound: direct via routing table
                              | inbound: through Security Gate
  +----------+    +-----------+----------+    +-----------------+
  | Display  |--->| Routing Table        |<---| Platform Svcs   |
  | Engine   |    | routes + inline      |    | (Clip,File,...) |
  |          |    | security filters     |    |                 |
  +----------+    +-----------^----------+    +-----------------+
                              | writes routes
                   +----------+----------+
                   |   Session Manager   |
                   |   (control plane)    |
                   +----------+----------+
                              | auth decisions
                   +----------+----------+
                   |   Security Gate     |
                   |   (auth + filters)   |
                   +---------------------+
```

### 3.3 Subsystem 1: Transport

```rust
/// Transport owns: connection establishment, probing, migration, raw I/O.
/// Transport does NOT know about channels, sessions, or PRISM framing.

trait PrismTransport: Send + Sync + 'static {
    /// Probe available transports and connect.
    /// Order: QUIC/UDP -> QUIC/UDP:443 -> DERP -> WebSocket/TCP
    /// 500ms timeout per attempt. Returns best available.
    async fn connect(target: &PrismTarget) -> Result<PrismConnection>;

    /// Accept incoming connections (server side)
    async fn accept(&self) -> Result<PrismConnection>;
}

/// Unified connection regardless of underlying transport
trait PrismConnection: Send + Sync {
    /// Send on a reliable stream
    async fn send_stream(&self, stream_id: u64, data: &[u8]) -> Result<()>;

    /// Send unreliable datagram.
    /// On TCP-backed transports, silently promoted to reliable stream.
    async fn send_datagram(&self, data: &[u8]) -> Result<()>;

    /// Receive from any stream/datagram
    async fn recv(&self) -> Result<PrismPacket>;

    /// Current transport metrics
    fn metrics(&self) -> TransportMetrics;

    /// Transport type (for degradation decisions)
    fn transport_type(&self) -> TransportType;
}

enum TransportType { QuicUdp, QuicUdp443, DerpRelay, WebSocketTcp }

struct TransportMetrics {
    rtt_us: u64,
    rtt_variance_us: u64,
    loss_rate: f32,           // 0.0-1.0
    bandwidth_bps: u64,
    transport_type: TransportType,
}
```

**Key invariant:** Everything above Transport doesn't know whether the connection is QUIC, DERP, or WebSocket. `send_datagram()` on TCP is silently promoted to reliable send. The degradation ladder uses `transport_type()` to know "TCP mode = already degraded."

### 3.4 Subsystem 2: Session Manager (Control Plane Only)

```rust
/// Session Manager owns: multi-client tracking, channel ownership,
/// capability negotiation, bandwidth arbitration, reconnection state.
/// Session Manager NEVER touches frame/event data directly.

struct SessionManager {
    clients: HashMap<ClientId, ClientSession>,
    channels: HashMap<u16, ChannelState>,
    routing_table: Arc<RoutingTable>,  // shared with data plane
    arbiter: BandwidthArbiter,
    tombstones: HashMap<SessionId, Tombstone>,  // R7
}

struct ClientSession {
    client_id: ClientId,
    public_key: [u8; 32],
    connection: Arc<dyn PrismConnection>,
    capabilities: ClientCapabilities,
    profile: ConnectionProfile,              // R39
    subscribed_channels: HashSet<u16>,
    last_seen: Instant,
}

struct ClientCapabilities {
    protocol_version: u16,                   // R1
    channels: Vec<ChannelCap>,               // extensible list
    performance: PerformanceProfile,         // R29
}

struct PerformanceProfile {
    max_decode_resolution: (u32, u32),
    max_decoder_instances: u8,               // R21
    supported_codecs: Vec<CodecId>,
    can_composite_regions: bool,             // R30
    estimated_decode_fps: u16,
}

struct ChannelCap {
    channel_id: u16,
    channel_version: u16,
    config: ChannelConfig,  // channel-specific params
}

// R10: Channel ownership
enum ChannelOwnership {
    Exclusive(ClientId),                     // Display, Input, Camera
    Shared(Vec<ClientId>),                   // Clipboard, Notify, FileShare
    Transferable {
        current: ClientId,
        transfer_policy: TransferPolicy,
    },
}

// R39: Connection profiles
struct ConnectionProfile {
    name: String,                            // "Gaming", "Coding", etc.
    display: DisplayProfile,
    degradation: DegradationConfig,
    channel_priorities: Vec<(u16, Priority)>,
}

struct DisplayProfile {
    prefer_lossless_text: bool,
    max_fps: u8,
    region_detection: bool,
    encoder_preset: EncoderPreset,
    color_space: ColorSpace,
}

// R37: Shared routing table
struct RoutingTable {
    routes: Arc<RwLock<HashMap<u16, Vec<RouteEntry>>>>,
}

struct RouteEntry {
    client_id: ClientId,
    conn: Arc<dyn PrismConnection>,
    filter: Option<Arc<dyn SecurityFilter>>,
}

// R47: Predictive bandwidth reservation
struct BandwidthArbiter {
    total_bandwidth: u64,
    reservations: HashMap<u16, BandwidthReservation>,
}
```

**Key invariant:** Session Manager writes the routing table. Producers read it. Frame data never flows through Session Manager. Adding more clients requires zero changes to Display Engine or Platform Services.

### 3.5 Subsystem 3: Display Engine

```rust
/// Display Engine owns: capture, region classification, encoding,
/// frame pipeline, degradation ladder, virtual displays.

trait PlatformCapture: Send + Sync {
    /// Start capturing. Fires on_frame on content change only (R31).
    async fn start(&mut self, config: CaptureConfig) -> Result<()>;

    /// Callback for damage events
    fn on_frame(&self, callback: impl Fn(CapturedFrame) + Send);

    /// Input-triggered immediate capture (R32)
    fn trigger_capture(&self);

    /// Virtual display for headless servers (R24)
    fn create_virtual_display(&self, config: DisplayConfig) -> Result<DisplayId>;
}

struct CapturedFrame {
    texture: PlatformTexture,        // GPU-resident, never CPU-copied
    damage_rects: Vec<Rect>,
    capture_time_us: u64,
    display_id: DisplayId,           // R40: per-monitor
}

// R20: Region map as first-class protocol concept
struct FrameRegionMap {
    frame_seq: u32,
    display_id: DisplayId,           // R40
    regions: Vec<Region>,
}

struct Region {
    rect: Rect,
    encoding: RegionEncoding,
    decoder_slot: u8,                // R21
}

enum RegionEncoding {
    Video { codec: CodecId, bitstream: Bytes },
    Lossless { format: LosslessFormat, data: Bytes },
    DamageRect { delta: Bytes },     // R23
    Unchanged,                       // R46: client uses cached texture
}

// R16: Degradation ladder
struct DegradationLadder {
    current_level: u8,
    levels: Vec<DegradationLevel>,
    hysteresis_sec: f32,
}

struct DegradationLevel {
    // Trigger conditions (ANY triggers step-down)
    max_rtt_ms: u32,
    max_loss_pct: f32,
    min_bandwidth_mbps: f32,
    max_client_decode_ms: f32,       // R17
    // Encoder settings
    max_bitrate_mbps: f32,
    max_resolution: (u32, u32),
    max_fps: u8,
    codec: CodecId,
    region_detection: bool,
    fec_ratio: f32,                  // R28
}

// R43: Parallel encoder pool
struct EncoderPool {
    encoders: Vec<HwEncoder>,
    max_concurrent: usize,           // from GPU capability query
}

// R36: Zero-allocation frame pipeline
struct FrameRing {
    buffer: MmapMut,                 // 64MB, huge pages
    slot_size: usize,
    write_idx: AtomicU32,
    read_idx: [AtomicU32; 2],        // encoder + network consumers
}
```

**Key invariant:** Display Engine produces `EncodedFrame` messages and writes them directly to Transport via the routing table. It receives `TransportMetrics` and `ClientFeedback` from Session Manager. It receives input-triggered capture kicks. It never interacts with PrismConnection directly (reads routing table).

### 3.6 Subsystem 4: Platform Services

```rust
/// Each platform service is independent. They all implement ChannelHandler
/// and plug into Session Manager. Adding a new service = implement trait + register.

trait ChannelHandler: Send + Sync {
    /// Channel this handler serves
    fn channel_id(&self) -> u16;

    /// Process incoming message from a client.
    /// For data-plane responses (e.g., clipboard fan-out, file chunks),
    /// the handler writes directly to the routing table — NOT returning
    /// data through Session Manager. OutgoingMsg is only for control-plane
    /// responses (e.g., capability changes, error signals).
    async fn handle(
        &self,
        from: ClientId,
        msg: PrismPacket,
        routes: &RoutingTable,
    ) -> Result<Vec<ControlMsg>>;

    /// Recovery state for reconnection (R5)
    fn reconnect_state(&self, client: ClientId) -> ReconnectData;

    /// Apply recovery after reconnect — writes directly to routing table
    async fn apply_reconnect(
        &self,
        client: ClientId,
        client_state: &ReconnectData,
        routes: &RoutingTable,
    ) -> Result<Vec<ControlMsg>>;
}

enum ReconnectData {
    Display,                                 // send IDR
    Clipboard { last_seq: u64 },             // replay from seq
    FileShare { transfers: Vec<TransferResume> },
    Audio,                                   // reset decoder
    Notifications { last_id: String },
    None,                                    // Input, Sensor: no recovery
}

// R11: Clipboard with source tagging
struct ClipboardUpdate {
    sequence: u64,
    source_device: ClientId,                 // suppress echo on originator
    content_type: ClipContentType,
    payload: Bytes,
}

// R41: Extensions use the same trait
trait PrismExtension: ChannelHandler {
    fn manifest(&self) -> ExtensionManifest;
}

struct ExtensionManifest {
    channel_id: u16,                         // 0x100+
    name: String,
    version: u16,
    transport_preference: TransportPref,
    ownership: ChannelOwnership,
    requires: Vec<u16>,                      // dependency channels
}
```

**Key invariant:** Each service is independent. Clipboard doesn't know about FileShare. They all implement `ChannelHandler` and register with Session Manager. Adding a new service (or third-party extension) requires no changes to any other subsystem.

### 3.7 Subsystem 5: Security

```rust
/// Security owns: Noise NK handshake, key management, allowlist,
/// pre-authentication (R3), content filters.
/// Sits between Transport and everything else.

trait SecurityGate: Send + Sync {
    /// Validate BEFORE any QUIC response (R3).
    /// Returns None = silent drop.
    async fn authenticate(&self, initial: &QuicInitial) -> Option<AuthenticatedClient>;

    /// Filter outgoing data per channel per client
    fn filter_outgoing(&self, client: ClientId, channel: u16, data: &[u8]) -> FilterResult;

    /// Filter incoming data per channel per client
    fn filter_incoming(&self, client: ClientId, channel: u16, data: &[u8]) -> FilterResult;
}

enum FilterResult {
    Allow,
    Redact(Bytes),               // modified payload (masked password in clipboard)
    Block,                       // silently dropped
    Confirm(String),             // requires user approval
}

struct AuthenticatedClient {
    public_key: [u8; 32],
    device_name: String,
    noise_session: NoiseSession, // for 0-RTT resumption
}
```

**Key invariant:** Every packet passes through SecurityGate before reaching any subsystem. Session Manager never sees unauthenticated data. Content filters (clipboard passwords, banking notifications) are enforced here, not in individual services.

### 3.8 Subsystem 6: Observability

```rust
/// Every subsystem implements Observable.
/// Metrics are collected in ring buffers and available via Control channel.

trait Observable {
    fn metrics(&self) -> MetricsSnapshot;
}

struct FrameLatencyBreakdown {
    capture_us: u32,
    classify_us: u32,
    encode_us: u32,
    send_us: u32,
    network_us: u32,
    decode_us: u32,       // client-reported (R17)
    render_us: u32,       // client-reported
    total_us: u32,
}

// Metrics each subsystem emits:
// Transport:  RTT, loss, bandwidth, transport type, bytes in/out
// Display:    capture_time, encode_time, frame_size, region_count,
//             degradation_level, codec, resolution, actual fps
// Input:      event_count, round_trip_us
// Clipboard:  sync_latency, payload_size, echoes_suppressed
// FileShare:  throughput, active_transfers, bytes_remaining
// Session:    connected_clients, channel_subs, arbiter_decisions
```

**Key invariant:** Observability is a subsystem, not an afterthought. Every component emits structured metrics. Client can request real-time latency breakdown overlay via Control channel. Optional external export (Prometheus/StatsD) for production monitoring.

### 3.9 Dependency Graph

```
Security         <- depends on nothing (standalone)
Transport        <- depends on Security (pre-auth)
Session Manager  <- depends on Transport + Security
Display Engine   <- depends on nothing (events from Session Manager)
Platform Services <- depends on nothing (each independent)
Observability    <- depends on nothing (reads from all subsystems)

Build order:
  Security -> Transport -> Session Manager
  -> Display Engine + Platform Services + Observability (parallel)
```

---

## 4. Cross-Cutting Concerns

### 4.1 Connection Profiles (R39)

Preset combinations of settings for different use cases:

| Profile | Optimize For | Region Detection | Max FPS | Encoder Preset | Lossless Text |
|---------|-------------|-----------------|---------|----------------|---------------|
| Gaming | Input latency, FPS | Off | 120 | UltraLowLatency | No |
| Coding | Text sharpness, low BW | On | 60 | Quality | Yes |
| Media | Color accuracy, smooth video | On (video bias) | 60 | Quality | No |
| Mobile | Battery, low BW | Off | 30 | Balanced | No |
| Companion | Minimal resources | N/A | N/A | N/A | N/A |

Client selects profile during handshake or switches mid-session via Control channel. Users can customize profiles. Profiles pre-configure the degradation ladder thresholds, encoder settings, and channel priorities.

### 4.2 Multi-Monitor (R40)

Each physical (or virtual) monitor gets an independent pipeline:

```
Monitor 1: WGC capture -> region classify -> NVENC encoder 1 -> send
Monitor 2: WGC capture -> region classify -> NVENC encoder 2 -> send
```

Monitor layout exchanged during handshake:

```rust
struct MonitorLayout {
    monitors: Vec<MonitorDescriptor>,
}

struct MonitorDescriptor {
    display_id: DisplayId,
    name: String,
    resolution: (u32, u32),
    position: (i32, i32),        // relative position in virtual desktop
    scale_factor: f32,
    refresh_rate: u8,
    primary: bool,
}
```

Client can selectively enable/disable monitors and request per-monitor resolution scaling.

### 4.3 Per-Channel Recovery (R5)

| Channel | Recovery Action | State Preserved |
|---------|----------------|-----------------|
| Display | Fresh IDR keyframe | None (latest frame is all that matters) |
| Input | None | None (client resumes sending) |
| Clipboard | Replay from last seq | Server keeps changelog (last 100 entries) |
| FileShare | Resume from ACK'd offset | Both sides persist transfer state |
| Audio | Decoder reset | None (fresh Opus frames) |
| Notifications | Replay from last ID | Server keeps last 50 notifications |
| Camera | Restart stream | Re-negotiate resolution/codec |
| Sensor | Resume sending | None (latest-value-wins) |
| Control | Re-exchange capabilities | Session config preserved in tombstone |

### 4.4 Channel Priority (R14)

| Priority | Channels | Behavior Under Congestion |
|----------|----------|--------------------------|
| Critical | Input (0x002) | Never throttled. Datagrams always sent. |
| High | Display (0x001), Audio (0x003) | Bitrate reduced before throttled. |
| Normal | Control (0x006), Clipboard (0x004) | Deferred during severe congestion. |
| Low | FileShare (0x007), Device (0x005) | Paused during congestion. Auto-resumes. |
| Background | Sensor (0x0E3), Notify (0x0E1) | Reduced update rate during congestion. |

---

## 5. Hyper-Optimizations

Techniques that push PRISM below the 16ms target toward the physical limits. Organized by the pipeline stage they optimize.

### 5.1 Capture Stage

**H1: Zero-Copy Capture-to-Encode (R36)**

WGC/DDA captures a D3D11 Texture2D in GPU memory. Register it directly as NVENC/AMF/QSV input. Encoded bitstream lands in pinned system memory. No pixel data ever touches CPU memory.

Savings: ~4ms per 4K frame (eliminates GPU download of raw pixels).

**H3: Capture-on-Damage, Not Polling (R31)**

DDA's `AcquireNextFrame()` blocks until new content. WGC's `FrameArrived` fires only on change. Static desktop (user reading) = 0 captures, 0 encodes, 0 bandwidth. Active typing = 5-10 events/sec, not 60.

**H11: Speculative IDR on Scene Change (R44)**

Hook `EVENT_SYSTEM_FOREGROUND` (Win32). On Alt+Tab, trigger immediate low-quality IDR before capture-on-damage fires. Pixels on screen in ~13ms instead of ~31ms. Next regular frame replaces with full quality.

### 5.2 Encode Stage

**H2: Encoder Lookahead Elimination**

Configure hardware encoders for zero lookahead, no B-frames, constant bitrate:

```
NVENC: tuning = ULTRA_LOW_LATENCY, lookahead = 0, gopLength = INFINITE
AMF:   usage = ULTRA_LOW_LATENCY, bFrames = 0
QSV:   targetUsage = 7 (speed), lookAhead = 0
```

Savings: 33-66ms eliminated. Single biggest latency win.

**H9: GPU-Side Frame Differencing (R42)**

Compute shader diffs current vs previous frame on GPU. Output: damage mask (~4KB for 4K, vs 33MB raw pixels). CPU reads only the mask, makes encoding decisions. Diff time: ~0.1ms vs 3-4ms for CPU.

**H10: Parallel Encoder Pool (R43)**

Modern GPUs support 2-3 concurrent NVENC sessions. Encode multiple regions in parallel:

```
Serial:   Region1 (1.5ms) + Region2 (2.0ms) + Region3 (0.3ms) = 3.8ms
Parallel: max(1.5ms, 2.0ms, 0.3ms) = 2.0ms
Savings: 1.8ms (47%)
```

**H12: Adaptive Keyframe Interval (R45)**

User reading (static): keyframe every 30s. Active coding: every 5s. Video playback: every 2s. Lossy network (>2%): every 1s. Interval adjusts continuously based on content activity and loss rate.

### 5.3 Network Stage

**H5: Slice-Level Streaming (R33)**

H.264/H.265 slices are independently decodable. Send each slice as it's encoded, don't wait for full frame:

```
Full-frame: 3ms encode + 10ms network + 2ms decode = 15ms
Slice-stream: 0.75ms + 10ms + 0.5ms = 11.25ms (pipelined)
Savings: ~4ms
```

NVENC supports this via `sliceMode` configuration.

**H6: QUIC Send Buffer Tuning (R34)**

Pin QUIC event loop to dedicated CPU core. Use GSO (Generic Segmentation Offload) for batch socket writes. Use IOCP (Windows) or io_uring (Linux) for zero-copy sends. Treat display datagrams as burst-eligible (bypass pacing).

### 5.4 Client Stage

**H7: Scroll Prediction (R35)**

Client detects scroll gesture, shifts current frame by predicted pixels, fills edges with background color. Server frame arrives ~20ms later, replaces prediction. User perceives instant scroll.

**H13: Static Region Atlas (R46)**

Client caches static regions (taskbar, sidebars) as GPU textures. Server marks unchanged regions in region map (zero bytes). Client composites cached + fresh in single GPU pass. 40-60% bandwidth savings for typical desktops.

### 5.5 System-Level

**H4: Input-Triggered Capture (R32)**

Server receives click/keystroke → immediately triggers capture cycle instead of waiting for next frame interval. Response frame ships up to 16ms earlier.

**H8: Memory-Mapped Ring Buffer (R36)**

Pre-allocated ring buffer (64MB, huge pages) for frame handoff between capture, encode, and network threads. Lock-free producer/consumer. Zero heap allocation on hot path.

**H14: Predictive Bandwidth Reservation (R47)**

Arbiter pre-allocates bandwidth when FileShare/Camera/Audio announces intent. Pre-adjusts display bitrate before congestion occurs. Eliminates reactive adaptation lag (~500ms → ~0ms).

**H15: NUMA-Aware Pipeline Pinning (R48)**

Pin capture/encode threads to same NUMA node as GPU. Pin QUIC event loop and input processing to high-performance cores. FileShare/Clipboard/Notifications to efficiency cores.

---

## 6. Requirements Index

### From Scenarios

| Req | Scenario | Description |
|-----|----------|-------------|
| R1 | S1: First Connection | Extensible capability negotiation (channel tuples, not flat struct) |
| R2 | S1: First Connection | Server-authoritative channel assignments |
| R3 | S1: First Connection | Silent drop indistinguishable from "host doesn't exist" |
| R4 | S1: First Connection | Direct LAN uses same sequence as Tailscale |
| R5 | S2: Reconnection | Per-channel recovery state definitions |
| R6 | S2: Reconnection | Server-side frame ring buffer (500ms, drop older) |
| R7 | S2: Reconnection | Heartbeat timeout produces session tombstone (60s + 5min) |
| R8 | S2: Reconnection | Connection migration distinct from reconnection |
| R9 | S3: Multi-Client | Multi-client first-class, per-client sessions |
| R10 | S3: Multi-Client | Channel ownership: exclusive, shared, transferable |
| R11 | S3: Multi-Client | Clipboard fan-out with source tagging (replaces hash dedup) |
| R12 | S3: Multi-Client | Per-client notification routing |
| R13 | S4: File + Display | Bandwidth arbiter across channels and clients |
| R14 | S4: File + Display | Channel priority enforcement |
| R15 | S4: File + Display | FileShare adapts to available bandwidth |
| R16 | S5: Degradation | Explicit degradation ladder with thresholds and hysteresis |
| R17 | S5: Degradation | Client-side quality feedback via Control channel |
| R18 | S5: Degradation | Encoder reconfiguration speed (cheap vs expensive changes) |
| R19 | S5: Degradation | User override (pin quality, set minimums) |
| R20 | S6: Regions | Region map is first-class protocol concept |
| R21 | S6: Regions | Fixed decoder pool on client (negotiated in handshake) |
| R22 | S6: Regions | Lossless-by-default for uncertain regions |
| R23 | S6: Regions | Damage rects for static regions |
| R24 | S7: Headless | Virtual display creation on demand |
| R25 | S7: Headless | Multi-client virtual displays |
| R26 | S8: Hostile Network | TCP fallback transport |
| R27 | S8: Hostile Network | Automatic transport probing cascade |
| R28 | S8: Hostile Network | FEC as degradation step, not separate feature |
| R29 | S9: Underpowered | Client performance profile in handshake |
| R30 | S9: Underpowered | Simple mode for low-capability clients |

### From Architecture

| Req | Source | Description |
|-----|--------|-------------|
| R31 | H3: Capture-on-damage | Variable framerate from damage-driven capture |
| R32 | H4: Input-triggered | Input events trigger immediate capture cycle |
| R33 | H5: Slice streaming | Slice-level packetization for progressive decode |
| R34 | H6: Send tuning | Dedicated pinned I/O threads for critical channels |
| R35 | H7: Scroll prediction | Server sends scroll velocity hints to client |
| R36 | H8: Ring buffer | Zero-allocation frame pipeline (pre-allocated ring) |
| R37 | G1: Data/control split | Session Manager routes metadata, not frame bytes |
| R38 | G2: Observability | Per-frame latency breakdown, metrics ring buffers |
| R39 | G3: Profiles | Connection profiles (Gaming, Coding, Media, Mobile, Companion) |
| R40 | G4: Multi-monitor | Per-monitor independent capture-encode pipelines |
| R41 | G5: Extensions | Extensions use same ChannelHandler trait as built-in |
| R42 | H9: GPU diff | GPU compute shader for frame differencing |
| R43 | H10: Parallel encode | Encoder pool with concurrent region encoding |
| R44 | H11: Speculative IDR | Win32 event hooks trigger speculative captures |
| R45 | H12: Adaptive keyframe | Keyframe interval scales with activity and loss |
| R46 | H13: Static atlas | Client-side texture atlas for static region caching |
| R47 | H14: Predictive BW | Predictive bandwidth reservation in arbiter |
| R48 | H15: NUMA pinning | NUMA-aware and core-type-aware thread pinning |

---

## 7. Subsystem Spec Roadmap

Each subsystem gets its own design spec after this architecture is approved:

| Subsystem | Spec File | Scope | Phase |
|-----------|-----------|-------|-------|
| Transport | `transport-design.md` | QUIC, probing cascade, TCP fallback, migration, DERP integration | 1 |
| Security | `security-design.md` | Noise NK, key management, allowlist, content filters, silent drop | 1 |
| Display Engine | `display-engine-design.md` | Capture traits, region detection (GPU), encoder pool, degradation ladder, virtual display, all H-optimizations | 1-2 |
| Session Manager | `session-manager-design.md` | Multi-client, channel ownership, bandwidth arbiter, capability negotiation, reconnection, profiles, routing table | 1 (single), 3 (multi) |
| Clipboard | `clipboard-design.md` | Continuous sync, fan-out, source tagging, content-type thresholds, security filters | 3 |
| FileShare | `fileshare-design.md` | Quick Send, DnD, browse, gallery, chunked transfer, pause/resume | 3 |
| Audio | `audio-design.md` | Bidirectional Opus, low-latency path, AV sync | 3 |
| Notifications | `notifications-design.md` | Mirroring, actions, filtering, desktop rendering | 4 |
| Camera | `camera-design.md` | Phone camera capture, encoding, virtual webcam drivers, controls | 4 |
| Sensor | `sensor-design.md` | GPS, accelerometer, gyroscope, consumer APIs | 4 |
| Touch | `touch-design.md` | 4 interaction modes, gesture mapping, mid-session switching | 4 |
| Browser Client | `browser-client-design.md` | WebTransport, WebCodecs, decoder pool, compositor, Safari fallback | 2 |
| Observability | `observability-design.md` | Metrics collection, latency breakdown, client overlay, external export | 1 |

---

*PRISM Architecture Design v1.0 — CC0 Public Domain*
