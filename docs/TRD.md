# PRISM — Technical Reference Document

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-19                     |
| Authors     | Ehsan + Claude                 |
| PRD ref     | PRD.md                         |

This document contains all implementation-level technical detail for PRISM. It consolidates the core protocol spec (v0.2) and the PRISM-Mobile companion spec (v0.1) into a single reference. The PRD defines *what* and *why*; this TRD defines *how*.

---

## 1. Protocol Comparison

| Feature | RDP | Parsec | SPICE | PRISM |
|---|---|---|---|---|
| Transport | TCP | UDP + custom | TCP | QUIC (RFC 9000) |
| Handshake | 3-5 RTT (NLA) | 2 RTT | 3+ RTT | 1 RTT (Noise NK) |
| Display codec | H.264/AVC | H.265 HW | MJPEG/LZ4 | AV1/H.265 + lossless regions |
| Multi-channel | Virtual channels | Monolithic | Full multi-ch | QUIC streams (independent) |
| Per-app capture | No | Yes | No | Yes (WGC) |
| Connection migration | No | No | No | Native (QUIC) |
| Cursor rendering | Server-side | Client-side | Agent | Predictive client-side |
| Browser client | No* | No | No | WebTransport + WebCodecs |
| Open source | No | No | Yes | Yes |

\* Apache Guacamole provides browser-based RDP access but wraps the legacy protocol.

---

## 2. Transport Layer

### 2.1 Protocol Stack

```
┌─────────────────────────────────────────────────────────┐
│  Application   Display │ Input │ Audio │ Clip │ Ctrl    │
├─────────────────────────────────────────────────────────┤
│  Framing       PRISM Frame Protocol                     │
│                Channel MUX via QUIC stream IDs          │
├─────────────────────────────────────────────────────────┤
│  Transport     QUIC streams (reliable)                  │
│                QUIC datagrams RFC 9221 (unreliable)     │
├─────────────────────────────────────────────────────────┤
│  Security      TLS 1.3 (QUIC-integrated)                │
│                Noise NK pre-authentication              │
├─────────────────────────────────────────────────────────┤
│  Network       UDP / IPv4 / IPv6                        │
│                Tailscale WireGuard / Direct              │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Hybrid Reliable/Unreliable Display Transport

RFC 9221 QUIC DATAGRAM frames are unreliable but ACK-eliciting — the sender knows when they were lost via QUIC's ACK mechanism. This enables the optimal video strategy without custom FEC:

```rust
// Display channel frame routing
match frame.frame_type {
    FrameType::IDR | FrameType::SPS | FrameType::PPS => {
        // Keyframes & codec config: MUST arrive → reliable QUIC stream
        quic_conn.send_stream(DISPLAY_STREAM_ID, frame.data).await;
    }
    FrameType::P | FrameType::B => {
        // Delta frames: loss-tolerant → QUIC datagrams (lowest latency)
        quic_conn.send_datagram(frame.data).await;
    }
}

// Server-side: on detected reference-frame loss, force recovery
if lost_frame.is_reference && !recovery_pending {
    encoder.force_idr();
    recovery_pending = true;
}
```

**Design rationale:** Keyframes on reliable streams get automatic QUIC retransmission. P/B-frames as datagrams have no retransmission overhead. Lost reference frames are detected via ACK gaps, triggering an immediate recovery IDR keyframe. This avoids custom FEC complexity in Phase 1 while maintaining visual quality. The `quinn` Rust crate supports both streams and RFC 9221 datagrams.

### 2.3 Why QUIC Over Raw UDP

Parsec and Moonlight use custom UDP protocols with their own reliability layers. QUIC provides congestion control, encryption, connection management, 0-RTT session resumption, and connection migration (sessions survive Wi-Fi → cellular transitions) without maintaining a custom transport stack. The engineering velocity gain outweighs the slight per-packet overhead versus raw UDP.

---

## 3. Channel Architecture

Each feature maps to an independent QUIC stream or datagram flow. Inspired by SPICE's multi-channel model.

### 3.1 Core Channels

| Channel | ID | Description | QoS | Transport |
|---|---|---|---|---|
| Display | 0x01 | Frame data, damage rects, codec bitstream | Highest | Hybrid (stream + datagram) |
| Input | 0x02 | Keyboard, mouse, touch, pen, gamepad events | Highest | Datagram |
| Audio | 0x03 | Opus-encoded audio stream, bidirectional | High | Datagram |
| Clipboard | 0x04 | Text, images, small data — continuous bidirectional sync | Normal | Stream |
| Device | 0x05 | USB/IP forwarding, printer redirect, serial | Normal | Stream |
| Control | 0x06 | Session management, resolution, capabilities, heartbeat | Normal | Stream |
| FileShare | 0x07 | Bidirectional file transfer, quick send, filesystem browse | Normal | Stream |

### 3.2 Mobile Extension Channels

| Channel | ID | Description | Mode | Transport |
|---|---|---|---|---|
| Notify | 0xE1 | Notification mirroring, actions, dismissal sync | Always-on | Stream |
| Camera | 0xE2 | Phone camera as virtual webcam on desktop | On-demand | Datagram |
| Sensor | 0xE3 | GPS, accelerometer, gyroscope, compass, barometer | On-demand | Datagram |
| Touch | 0xE4 | Touch interaction mode control, gesture negotiation | On-demand | Stream |

### 3.3 Extension Channels

| Channel | ID | Description |
|---|---|---|
| Extension | 0xF0+ | User-defined channels (custom agents, integrations) |

Mobile extension channels (0xE0+) are optional — clients that don't support them simply don't advertise these capabilities during handshake.

---

## 4. Packet Format

Minimal framing on top of QUIC — encryption, ordering, and reliability are handled by the transport.

```
┌──────────┬──────────┬──────────┬──────────────┬────────────────┐
│ 8 bits   │ 8 bits   │ 16 bits  │ 32 bits      │ 32 bits        │
│ Channel  │ Msg Type │ Flags    │ Timestamp µs │ Payload Length │
│ ID       │          │          │              │                │
├──────────┴──────────┴──────────┴──────────────┴────────────────┤
│ Variable: Payload                                              │
└────────────────────────────────────────────────────────────────┘
```

**Total header: 12 bytes** vs RDP's typical 20-30 bytes (TPKT + MCS + virtual channel headers).

The microsecond Timestamp field enables RTT calculation without separate ping messages. Both client and server include send timestamps, and the difference between a sent timestamp and its acknowledgment gives precise latency measurements for frame pacing and A/V sync.

---

## 5. Connection Handshake

Noise NK pattern — the client knows the server's static public key via Tailscale identity, QR code, or manual exchange.

```
 Client                                              Server
   │                                                       │
   │  ──── QUIC Initial + Noise NK Handshake ──────────►   │
   │       (ephemeral pub, encrypted static pub,           │
   │        capability flags, display config)               │
   │                                                       │
   │  ◄─── Handshake Response ─────────────────────────    │
   │       (server caps, codec negotiation,                │
   │        channel assignments, first keyframe)            │
   │                                                       │
   │  ════ Streaming begins (1 RTT total) ════════════     │
   │                                                       │
   │  Subsequent connections: 0-RTT via QUIC resumption    │
   │  + cached Noise session                                │
```

The first keyframe is embedded in the handshake response. On a 20ms RTT network, pixels appear on screen in ~40ms from connection initiation. RDP typically requires 3-5 round trips before first pixel.

---

## 6. Display Channel — Capture & Encoding

### 6.1 Windows Capture APIs

Two sanctioned APIs — no DWM hooking required:

**Windows.Graphics.Capture (WGC)** — Primary
- Per-window capture via HWND targeting
- D3D11 texture output (GPU-resident)
- Win32 interop since Windows 10 May 2019 Update
- Cursor rendering can be toggled on/off
- Yellow capture border: mandatory on Win10, optional on Win11
- Rust crate: `windows-capture`

**DXGI Desktop Duplication (DDA)** — Fallback
- Full-monitor capture only (no per-window)
- D3D11 textures with dirty rects + move rects metadata
- Max 4 concurrent duplication instances per session
- Used for full-desktop mode and the wallpaper/background layer

**Research Resolution (v0.1 Question #1):** DWM hooking is unnecessary. WGC provides per-window capture natively. The PRISM agent enumerates the window tree via Win32 APIs (EnumWindows, GetWindowInfo), creates a WGC capture item per window, classifies each window's content by update frequency + window class name, and routes each captured surface to the appropriate encoder. Pure userspace, no EULA risk.

### 6.2 Region Encoding Strategy

| Region Type | Encoding | Detection Method | Bandwidth |
|---|---|---|---|
| Text | Lossless PNG/QOI | Frame analysis: high contrast, antialiased edges, regular spacing | ~80-90% savings vs video codec |
| Video | H.265/AV1 hardware encode (NVENC/AMF/QSV) | High update frequency, motion vectors | Adaptive bitrate |
| Static UI | Damage rects (lossless, cached) | No pixel changes between frames | Near-zero for unchanged regions |
| Cursor | Client-predicted | Shape sent once; position at input rate | Minimal (shape + deltas) |

**Research Resolution (v0.1 Question #2):** True glyph-run extraction via DirectWrite's `IDWriteTextRenderer::DrawGlyphRun` requires per-process DLL injection — invasive and fragile. Deferred to Phase 3+ as an opt-in SDK. Phase 1-2 uses smart region classification + lossless text encoding, which achieves ~80-90% of the glyph-run bandwidth benefit with zero injection complexity. DirectWrite covers modern apps (Chromium, VS Code, Windows Terminal, Edge); GDI apps fall back to standard video encoding — never worse than RDP.

### 6.3 Codec Negotiation

Priority order: AV1 (if both endpoints have HW support) → H.265 → H.264 → VP9 → software fallback.

Codecs can change per-region and adapt mid-session based on network conditions. Browser clients use `VideoDecoder.isConfigSupported()` for runtime capability detection. The codec is negotiated during the handshake via capability exchange in the Control channel.

---

## 7. Input Channel

### 7.1 Wire Format

```rust
// Input event (client → server) — sent as QUIC datagram
struct InputEvent {
    timestamp_us: u64,        // Microsecond precision
    event_type:   InputType,  // Key, Mouse, Touch, Pen, Gamepad
    device_id:    u8,         // Multi-device support
    payload:      InputPayload,
}

struct MousePayload {
    x: f32, y: f32,           // Subpixel position (0.0 - 1.0 normalized)
    buttons: u16,              // Button state bitmap
    scroll_x: f32,             // High-resolution horizontal scroll
    scroll_y: f32,             // High-resolution vertical scroll
    pressure: f16,             // Pen pressure (0 for mouse)
    tilt_x: f16, tilt_y: f16,  // Pen tilt angles
}
```

### 7.2 Design Decisions

- Sent as QUIC datagrams — lost events are superseded by the next, eliminating TCP head-of-line blocking that causes RDP's "mouse lag"
- Subpixel coordinates (f32, normalized 0.0–1.0) enable resolution-independent input
- Pen pressure and tilt are first-class fields, not extensions
- Browser clients use PointerEvent + Pointer Lock API for relative mouse movement
- System keys (Ctrl+Alt+Del, Alt+Tab, Windows key) cannot be intercepted from the browser sandbox — known and acceptable limitation

### 7.3 Predictive Cursor

The cursor is always rendered client-side. Server sends cursor shape changes (on hover over different UI elements); client draws the cursor at the local pointer position with zero perceived latency. Server sends position corrections only when the client's predicted position diverges.

For text input, the client optionally renders typed characters locally before server confirmation (optimistic update), similar to collaborative editors. The server sends corrections if prediction was wrong (rollback).

---

## 8. Security & NAT Traversal

### 8.1 Identity Model

Identity = Curve25519 static public key. No usernames or passwords at the protocol level (can be layered on top via the Control channel). The server maintains an allowlist of authorized client public keys. Unauthenticated packets are silently dropped — no port scanning surface.

### 8.2 Tailscale-First Transport

Phase 1-3: PRISM's QUIC connection runs directly over Tailscale's WireGuard tunnel. This eliminates three hard problems simultaneously:

- **NAT traversal:** Tailscale handles hole-punching, achieving direct P2P connections >90% of the time. DERP relays handle the rest.
- **Port exposure:** No fixed listening ports. The QUIC connection targets the server's Tailscale IP.
- **Key distribution:** Device identity is handled by Tailscale's coordination server. No manual key exchange needed.

Tailscale's DERP (Designated Encrypted Relay for Packets) relays forward already-encrypted WireGuard packets using Curve25519 keys as addresses — they cannot decrypt any traffic. DERP runs over HTTPS on port 443 for maximum firewall compatibility. Peer Relays (GA October 2025) use native WireGuard UDP for higher throughput on your own infrastructure.

### 8.3 Pluggable Transport Trait

```rust
trait PrismTransport {
    async fn connect(target: &PrismTarget) -> Result<QuicConnection>;
}

// Phase 1: Direct connection to known IP (Tailscale, LAN)
struct DirectTransport { endpoint: SocketAddr }

// Phase 4: Relay with STUN/TURN-style NAT traversal
struct RelayTransport { coord_url: Url, server_pubkey: [u8; 32] }
```

**Research Resolution (v0.1 Question #5):** Tailscale-first for Phase 1-3. The pluggable trait allows a custom relay to be added in Phase 4 for non-Tailscale users without architectural changes.

### 8.4 Key Distribution by Phase

- **Phase 1:** Tailscale mesh identity (automatic, zero-config)
- **Phase 2+:** QR code pairing (server displays code, client scans)
- **Phase 4:** PRISM coordination service with key exchange
- **Always available:** Manual public key copy-paste

---

## 9. Cross-Platform Capture

OS-agnostic channel architecture with platform-specific code isolated behind a capture trait.

| Platform | Capture API | Texture Format | HW Encoder | Zero-Copy Path | Phase |
|---|---|---|---|---|---|
| Windows | WGC (per-window) + DDA (full-desktop) | D3D11 | NVENC / AMF / QSV | D3D11 texture → encoder | 1 |
| macOS | ScreenCaptureKit | IOSurface | VideoToolbox | IOSurface → VTCompressionSession | 3 |
| Linux | PipeWire + xdg-desktop-portal | DMA-BUF fd | VAAPI / NVENC | compositor → DMA-BUF → encoder | 3 |
| Android (camera) | Camera2 API | MediaCodec surface | MediaCodec | Camera → Surface → MediaCodec | 4 |
| iOS (camera) | AVCaptureSession | CVPixelBuffer | VideoToolbox | Camera → CVPixelBuffer → VT | 4 |

**Linux Zero-Copy Pipeline:** On Wayland, PipeWire is the sanctioned screen capture path. The xdg-desktop-portal D-Bus API handles user permission. Frames stay on the GPU as DMA-BUF file descriptors, passed from the compositor to PipeWire to the hardware encoder without ever touching CPU memory. Format/modifier negotiation between producers and consumers is handled by PipeWire's DMA-BUF sharing protocol, with shared-memory fallback.

**Research Resolution (v0.1 Question #4):** All three desktop platforms have GPU-native capture paths. The Rust capture trait is defined in Phase 1 (Windows-only implementation) so macOS and Linux can be added in Phase 3 without architectural changes.

---

## 10. Browser Client

### 10.1 WebTransport

QUIC in the browser: reliable streams + unreliable datagrams via the WebTransport API. Chrome and Firefox support WebTransport over HTTP/3 with datagram support. The `serverCertificateHashes` option allows self-signed certificates identified by their SHA-256 hash — no Certificate Authority needed on the PRISM server.

The PRISM server exposes both native QUIC (for Rust clients) and WebTransport/HTTP3 (for browsers) on the same port. The server detects the client type from ALPN (Application-Layer Protocol Negotiation) during the TLS handshake.

**Safari:** No WebTransport support. Fallback path is WebSocket + MSE (Media Source Extensions) — functional but higher latency. Monitor W3C WebTransport WG for Safari signals.

### 10.2 WebCodecs

Hardware-accelerated video decoding in the browser via the `VideoDecoder` API. Runtime codec detection via `VideoDecoder.isConfigSupported()`.

| Codec | Chrome/Edge | Firefox | Safari | HW Decode Coverage |
|---|---|---|---|---|
| H.264 | Full support | Shipped 2025 | VideoDecoder | ~99% all platforms |
| H.265 | Since M130 (HW only) | Not in WebCodecs yet | Playback only | 75% Win / 99% Mac |
| AV1 | Full support | Full support | Limited | Growing (~8% Win HW encode) |

Codec negotiation priority: H.265 first (best quality per bit) → H.264 fallback (universal HW support) → AV1 where both endpoints have HW acceleration.

### 10.3 Browser Rendering Pipeline

```
WebTransport datagrams
    → EncodedVideoChunk
    → VideoDecoder (hardware-accelerated)
    → VideoFrame
    → Canvas rendering (or VideoTrackGenerator → <video> element)
```

**Reference implementation:** The Tango ADB project proves this full pipeline end-to-end, decoding H.264/H.265/AV1 streams from Android devices in a browser using WebCodecs.

### 10.4 Browser Input Capture

- Keyboard: KeyboardEvent API (standard)
- Mouse: PointerEvent + Pointer Lock API for relative mouse movement
- Touch: TouchEvent / PointerEvent
- Gamepad: Gamepad API (limited)
- **Limitation:** System-level keys (Ctrl+Alt+Del, Alt+Tab, Windows key) cannot be intercepted from the browser sandbox. This is inherent and not solvable.

**Research Resolution (v0.1 Question #6):** Browser client promoted from Phase 4 to Phase 2. Not just feasible — compelling as PRISM's primary differentiator (zero install, just a URL).

---

## 11. FileShare Channel (0x07)

FileShare is separate from Clipboard. Clipboard handles small, instant data (text snippets, single images <10MB). FileShare handles everything from a 50KB PDF to a 4GB video, with progress tracking, pause/resume, and bidirectional filesystem browsing.

### 11.1 Interaction Modes

| Mode | Description | Use Case |
|---|---|---|
| Quick Send | AirDrop/LocalSend model. Select file(s) → they appear on the other device. Configurable landing folder. No browsing, lowest friction. | Getting photos off phone, sharing a PDF |
| Drag & Drop | Cross-device DnD during active display streaming. Requires coordination between Input channel (drag gesture position) and FileShare (payload delivery). | Dragging a desktop file into a mobile app |
| Filesystem Browse | Remote file manager with tree view. Lazy-loaded directory listings. Download/upload/create folders. | Power users who want full access |
| Photo Gallery | Thumbnail grid of recent photos/videos from phone. One-tap pull to desktop. | Most common use case: getting photos off phone |

### 11.2 Wire Protocol

```rust
enum FileShareMsg {
    // Quick Send flow
    Offer       { files: Vec<FileMeta>, transfer_id: u64 },
    Accept      { transfer_id: u64, destination: Option<String> },
    Reject      { transfer_id: u64, reason: RejectReason },
    Chunk       { transfer_id: u64, file_index: u32, offset: u64, data: Bytes },
    ChunkAck    { transfer_id: u64, file_index: u32, bytes_received: u64 },
    Complete    { transfer_id: u64 },

    // Filesystem browse
    ListDir     { path: String, request_id: u32 },
    DirListing  { request_id: u32, entries: Vec<DirEntry> },
    Thumbnail   { path: String, request_id: u32 },
    ThumbnailData { request_id: u32, jpeg_data: Bytes },

    // Drag and drop
    DragBegin   { files: Vec<FileMeta>, drag_id: u32 },
    DragDrop    { drag_id: u32, target_window: Option<u64> },
    DragCancel  { drag_id: u32 },
}

struct FileMeta {
    name: String,
    size: u64,
    mime_type: String,
    modified: u64,            // Unix timestamp
    thumbnail: Option<Bytes>,  // JPEG thumbnail for images/video
}
```

### 11.3 Transfer Mechanics

- **Chunked with ACKs:** Enables pause/resume — if the connection drops mid-transfer, the receiver knows exactly which bytes it has and can request the remainder
- **Adaptive chunk sizing:** Start at 64KB, grow to 1MB on fast links based on measured throughput
- **Reliable transport:** FileShare runs on reliable QUIC streams — every byte must arrive
- **Concurrent transfers:** Multiple transfers use separate QUIC streams within the channel, so one slow transfer doesn't block others

---

## 12. Continuous Clipboard (0x04 Upgrade)

The core Clipboard channel (0x04) is upgraded from basic copy-paste to always-on continuous sync, modeled on Apple's Universal Clipboard.

### 12.1 How It Works

When either device's clipboard changes, the new content is immediately pushed to the peer. The receiving device updates its local clipboard silently. Copy on phone → paste on desktop. Copy on desktop → paste on phone. No user action required beyond initial pairing.

### 12.2 Wire Format

```rust
struct ClipboardSync {
    sequence: u64,                  // Monotonic counter for conflict resolution
    content_type: ClipContentType,  // What kind of data
    payload: Bytes,                 // The data (inline if small)
    payload_ref: Option<u64>,       // FileShare transfer_id if large
}

enum ClipContentType {
    PlainText,                      // Always synced immediately
    RichText { format: String },    // HTML or RTF
    Image { mime: String, w: u32, h: u32 },  // Synced inline if < 10MB
    Url,                            // Extracted and synced as text + metadata
    FileRef { files: Vec<FileMeta> }, // "Copy file" → handed off to FileShare
}
```

### 12.3 Design Decisions

- **Content-type thresholds:** PlainText and URLs sync immediately (tiny payload). Images under 10MB sync inline. Larger payloads (file copies, huge images) are offered as a FileShare transfer — the receiving clipboard shows "File available from [device]" and initiates the transfer on paste.
- **Conflict resolution:** Last-writer-wins with monotonic sequence numbers. Higher sequence number wins. In practice, humans don't copy on two devices at the exact same millisecond.
- **Deduplication hash:** Before sending, the sender hashes the clipboard content and sends the hash first. If the receiver already has that hash (common in echo loops: copy on A → paste on B → B's clipboard updates → would sync back to A), the transfer is skipped.

---

## 13. Notification Mirroring (0xE1)

Phone notifications appear on the desktop. Desktop notifications appear on the phone (when away). Dismissing on either device dismisses on both. Actions (Reply, Mark as Read) can be forwarded to the source device.

### 13.1 Data Model

```rust
struct Notification {
    id: String,                      // Unique: source_device:app:notification_id
    app_id: String,                  // Package name (Android) or bundle ID (iOS)
    app_name: String,                // Human-readable app name
    title: String,
    body: String,
    icon: Option<Bytes>,             // App icon (PNG, max 128x128)
    timestamp: u64,
    priority: NotifPriority,         // Low, Default, High, Urgent
    category: Option<String>,        // "message", "email", "social", "transport"
    actions: Vec<NotifAction>,       // Buttons the user can tap
    is_ongoing: bool,                // Music playback, navigation, etc.
}

struct NotifAction {
    action_id: String,
    label: String,
    is_reply: bool,                  // True = shows text input field on desktop
}

enum NotifEvent {
    Post(Notification),              // New notification arrived
    Update(Notification),            // Content changed (e.g. new message in group)
    Dismiss { id: String },          // User dismissed on source device
    Action { id: String, action_id: String, reply_text: Option<String> },
}
```

### 13.2 Filtering

Not all notifications should cross devices. Configurable filters:

- **Per-app allowlist/blocklist** — user chooses which apps mirror
- **Priority threshold** — only forward High/Urgent, suppress Low
- **Category filter** — forward "message" and "email," suppress "social"
- **Quiet hours** — no mirroring during configured time ranges
- **DND sync** — if phone is in Do Not Disturb, desktop mirrors stay silent

### 13.3 Desktop Rendering

Mirrored notifications use the native OS notification system: Windows Toast, macOS Notification Center, libnotify on Linux. Reply actions open an inline text field. The notification source is tagged with the phone's device name and the originating app icon. On Android, notification access is via `NotificationListenerService`. iOS has no equivalent (see Open Questions).

---

## 14. Camera Forwarding (0xE2)

The phone's camera appears as a virtual webcam on the desktop — cross-platform Continuity Camera.

### 14.1 Sequence

```
 Phone                    PRISM                    Desktop
   │                          │                          │
   │  ── Camera Offer ────►   │  ── Camera Available ──► │
   │     (caps, resolutions,  │                          │
   │      front/back, codec)  │                          │
   │                          │  ◄── Camera Request ───  │
   │  ◄── Start Stream ───   │     (resolution, fps,    │
   │                          │      preferred codec)    │
   │                          │                          │
   │  ══ H.264/H.265 frames via QUIC datagrams ══════►  │
   │                          │                   Virtual webcam
   │                          │                   device (v4l2/
   │                          │                   AVFoundation/
   │  ◄── Switch Camera ──   │                   DirectShow)
   │     (front ↔ back)       │                          │
```

### 14.2 Encoding

The phone encodes camera frames using its hardware encoder (MediaCodec on Android, VideoToolbox on iOS) and sends them as QUIC datagrams — same transport mechanism as the Display channel but on a separate QUIC stream.

### 14.3 Virtual Webcam Drivers

The desktop creates a virtual webcam device that any application (Zoom, Teams, OBS) can use as a camera source:

- **Windows:** DirectShow virtual camera filter (or OBS VirtualCam approach)
- **macOS:** CoreMediaIO Device Abstraction Layer (DAL) plugin (same mechanism Continuity Camera uses)
- **Linux:** v4l2loopback kernel module — write decoded frames to `/dev/videoN`

### 14.4 Controls

- Front/back camera switching from desktop UI
- Torch/flashlight toggle
- Resolution and FPS selection (up to 4K30 or 1080p60)
- Auto-orientation (phone rotation → stream rotation metadata)
- Zoom control via desktop (digital zoom mapped to phone's Camera API)

---

## 15. Sensor Passthrough (0xE3)

Expose the phone's sensors to the desktop for mobile app development, location-aware desktop apps, and motion-based input.

### 15.1 Wire Format

```rust
struct SensorFrame {
    sensor_type: SensorType,
    timestamp_us: u64,
    data: SensorData,
}

enum SensorType {
    GPS,              // lat, lon, altitude, accuracy, speed, bearing
    Accelerometer,    // x, y, z in m/s²
    Gyroscope,        // x, y, z in rad/s
    Magnetometer,     // x, y, z in µT
    Barometer,        // pressure in hPa
    AmbientLight,     // lux
    Proximity,        // distance in cm (or boolean near/far)
}
```

### 15.2 Transport & Rates

Sensor data is sent as QUIC datagrams (loss-tolerant, latest-value-wins semantics). Update rates are configurable per sensor type: GPS at 1Hz, accelerometer/gyroscope at 100Hz for motion tracking, barometer at 0.1Hz for weather apps. The desktop exposes sensors through a virtual sensor interface or local API that development tools can connect to. GPS data can feed into the system's location services via a location provider shim.

---

## 16. Touch Interaction Modes (0xE4)

When a phone connects to PRISM for display remoting, "touch emulates mouse" (the RDP approach) is inadequate. PRISM-Mobile defines four interaction modes, switchable mid-session without reconnecting.

| Mode | Input Semantics | Display Stream | Best For |
|---|---|---|---|
| Trackpad | Relative deltas (finger movement = cursor movement). Tap = click. Two-finger scroll. Pinch = zoom. | Active | Precise desktop interaction |
| Direct Touch | Absolute coordinates normalized to 0.0–1.0. Touch position maps to screen position. | Active | Touch-optimized apps, drawing, tablets |
| Gesture | Recognized gesture IDs mapped to keyboard shortcuts/macros. Swipe up = Alt+Tab, swipe left = Back, three-finger swipe = workspace switch. Fully customizable. | Active | Quick navigation, power users |
| Companion | No display stream. Phone shows controls, media playback, file quick-send, notification center. | None | Lowest bandwidth secondary input |

The active mode is communicated via the Touch channel (0xE4) during session setup and can be changed mid-session.

---

## 17. Mobile Design Philosophy

The mobile-to-desktop relationship is fundamentally different from desktop-to-desktop remoting. PRISM-Mobile operates on two tiers:

**Always-on background sync** — Clipboard continuously synced, notifications forwarded automatically. No "connect to remote" ceremony. Devices stay paired on a lightweight QUIC heartbeat connection. Minimal bandwidth.

**On-demand active features** — Display streaming, camera forwarding, large file transfers, sensor passthrough. These spin up dedicated channels when the user explicitly activates them.

The common failure mode of existing solutions (KDE Connect, Scrcpy, Apple Continuity, Intel Unison) is that they're all separate apps covering different subsets. KDE Connect does notifications but not display streaming. Scrcpy does display but not notifications. Apple Continuity does everything but only on Apple. PRISM-Mobile unifies all of these into one protocol that works across platforms.

**Quick win:** FileShare Quick Send and Continuous Clipboard can ship with core PRISM Phase 3 using just the browser client — no dedicated mobile app needed. Any phone with Chrome becomes a PRISM file sender and clipboard partner.

---

## 18. Mobile-Specific Security

Mobile extensions introduce attack surfaces beyond core PRISM remoting. Each channel has specific mitigations:

| Channel | Risk | Mitigation |
|---|---|---|
| Clipboard sync | Passwords and tokens transit automatically | Configurable content filters (skip high-entropy, short-length entries). Optional confirmation for sensitive patterns |
| Notification mirroring | OTP codes, private messages, financial data | Per-app allowlist (user must explicitly enable banking apps). E2E encrypted — relay cannot read |
| Camera forwarding | Camera access is sensitive on both platforms | Explicit per-session activation (never auto-starts). Persistent indicator on desktop. Stop button on phone notification |
| Filesystem browse | Full filesystem access is powerful and dangerous | Configurable scope (Downloads + Photos on phone, home dir on desktop). Optional PIN/biometric gate |
| Sensor data | GPS is privacy-sensitive | Location sharing requires explicit per-session opt-in. Sensor data never cached on receiver |

All mobile extension channels inherit PRISM's core security model: end-to-end encryption via QUIC/TLS 1.3, Noise NK authentication, no exposed ports. The Tailscale-first transport adds device identity verification — you can only pair with devices in your own tailnet.

---

## 19. Research Resolutions

Six open questions were raised in the v0.1 spec. All have been resolved through research:

| # | Question | Resolution | Impact |
|---|---|---|---|
| 1 | DWM hooking for region classification | **Unnecessary.** WGC provides per-window capture natively. DDA is the full-desktop fallback. Pure userspace. | Simplified Phase 1; no EULA risk |
| 2 | Glyph run extraction for text | **Deferred to Phase 3+.** DirectWrite's DrawGlyphRun requires per-process injection. Smart region classification + lossless encoding gets 80-90% of the benefit. | Phase 1-2 use frame analysis instead |
| 3 | QUIC datagram reliability for video | **Hybrid confirmed.** Keyframes on reliable streams, P/B-frames on datagrams. ACK-eliciting datagrams detect reference frame loss → forced IDR recovery. | No custom FEC needed for Phase 1 |
| 4 | macOS / Linux server support | **Clean trait boundary.** ScreenCaptureKit (macOS) and PipeWire + DMA-BUF (Linux) map cleanly. Define Rust capture trait in Phase 1. | macOS/Linux added in Phase 3 without refactor |
| 5 | Coordination service / NAT traversal | **Tailscale-first.** >90% direct P2P. DERP relay fallback. Pluggable transport trait for Phase 4 custom relay. | Ships fast; NAT traversal is free |
| 6 | Browser client feasibility | **Promoted to Phase 2.** WebTransport + WebCodecs proven by Tango ADB. H.265/H.264/AV1 negotiation. serverCertificateHashes for self-signed certs. | PRISM's killer differentiator |

---

## 20. Mobile Extension Roadmap

PRISM-Mobile features map onto the core PRISM roadmap with their own sub-phases:

| Phase | Scope |
|---|---|
| **P3 (Core Phase 3)** | FileShare channel (Quick Send + filesystem browse). Continuous Clipboard sync. Works with browser client — no mobile app needed. |
| **P4a (Mobile MVP)** | Android client (Rust + Kotlin). Notification mirroring (Android → Desktop). Touch interaction: Trackpad + Direct Touch modes. |
| **P4b (Full Mobile)** | Camera forwarding with virtual webcam drivers. Sensor passthrough. iOS client (Rust + Swift). Gesture mode. Photo gallery quick-access. |
| **P5 (Ecosystem)** | Bidirectional notification mirroring (desktop → phone). Desktop app continuity. Third-party extension SDK for mobile channels. |

---

*PRISM Protocol Specification v0.2.0 + PRISM-Mobile Extensions v0.1.0 — CC0 Public Domain*
