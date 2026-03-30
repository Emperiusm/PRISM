# PRISM — Product Requirements Document

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-19                     |
| Authors     | Ehsan + Claude                 |
| Spec refs   | protocol-spec.html (v0.2), prism-mobile-spec.html (v0.1) |

---

## 1. Problem Statement

Remote desktop protocols haven't kept up with modern expectations. RDP was designed in the late '90s around GDI drawing commands and TCP. It shows: high latency, clunky file transfer, constant security exposure on port 3389, and no mobile story beyond "touch emulates mouse." Alternatives like Parsec and Moonlight are better for latency but are proprietary, gaming-focused, and lack a cohesive phone-to-desktop experience.

Meanwhile, users increasingly need their phone and desktop to work as one continuous workspace — sharing files, syncing clipboards, mirroring notifications — but are forced to use 3-4 separate apps (KDE Connect, AirDrop, Scrcpy, etc.) to achieve what should be a unified experience.

There is no open-source protocol that combines low-latency display remoting with a full mobile-to-desktop bridge.

## 2. Vision

PRISM replaces RDP with a modern, open protocol built on QUIC that makes remoting a desktop feel like using a local machine, and connecting a phone feel like extending both devices into one workspace.

The core insight: desktop remoting and mobile integration are two faces of the same problem — multiplexing channels of display, input, files, clipboard, notifications, and sensor data over a single encrypted connection.

## 3. Target Users

**Primary — Phase 1-2:**
- Developers and power users who remote into Windows workstations from laptops or browsers
- Users already running Tailscale who want a better alternative to RDP/VNC

**Secondary — Phase 3-4:**
- Mobile users who want seamless phone-to-desktop integration (file sharing, clipboard sync, notification mirroring)
- Anyone who needs their phone camera as a webcam for video calls
- Mobile app developers who need real device sensors on desktop (GPS, accelerometer)

**Tertiary — Phase 4+:**
- Teams needing a self-hosted, open-source remote access solution
- Organizations replacing commercial RDP/VDI with something modern

## 4. Design Principles

1. **< 16ms latency budget** — Capture, encode, transmit, decode, and display within a single 60Hz frame. Every architectural decision optimizes for perceived latency.

2. **QUIC-native** — No TCP anywhere. Independent streams per channel eliminate head-of-line blocking. Built-in connection migration means sessions survive network changes.

3. **Region-aware encoding** — The server understands what's on screen. Text regions get lossless encoding, video regions get hardware-accelerated H.265/AV1, static UI gets damage rects. Never encode a terminal the same way you encode a YouTube video.

4. **Zero-trust security** — Identity is a cryptographic key (Curve25519). Noise NK handshake authenticates in 1 RTT. No exposed ports, no passwords at the protocol level. Server silently drops packets from unknown keys.

5. **Always-on + on-demand** — Background features (clipboard sync, notifications) stay alive on a lightweight heartbeat connection. Active features (display streaming, camera, file transfers) spin up dedicated channels when needed.

6. **Browser-first client** — The primary client is a web app via WebTransport + WebCodecs. No install, just a URL. Native clients exist for maximum performance, but the browser path eliminates adoption friction entirely.

## 5. Architecture Overview

### 5.1 Transport

PRISM runs over QUIC (RFC 9000) with the RFC 9221 datagram extension. Display P/B-frames travel as unreliable datagrams for lowest latency; keyframes and control data travel on reliable streams. The `quinn` Rust crate provides the implementation.

Network connectivity is handled by a pluggable transport trait. Phase 1 uses Tailscale (WireGuard tunnel, >90% direct P2P, DERP relay fallback). Phase 4 adds a custom relay for non-Tailscale users.

### 5.2 Channel Architecture

Each feature maps to an independent QUIC stream or datagram flow:

| Channel      | ID     | Purpose                                         | Transport  |
|-------------|--------|--------------------------------------------------|------------|
| Display      | 0x01   | Frame data, damage rects, codec bitstream        | Hybrid     |
| Input        | 0x02   | Keyboard, mouse, touch, pen, gamepad             | Datagram   |
| Audio        | 0x03   | Opus-encoded bidirectional audio                 | Datagram   |
| Clipboard    | 0x04   | Continuous bidirectional sync, content-type aware | Stream     |
| Device       | 0x05   | USB/IP forwarding, printer redirect              | Stream     |
| Control      | 0x06   | Session management, capabilities, heartbeat      | Stream     |
| FileShare    | 0x07   | File transfer, quick send, filesystem browse     | Stream     |
| Notify       | 0xE1   | Notification mirroring with actions              | Stream     |
| Camera       | 0xE2   | Phone camera as virtual webcam                   | Datagram   |
| Sensor       | 0xE3   | GPS, accelerometer, gyroscope, barometer         | Datagram   |
| Touch        | 0xE4   | Touch interaction mode negotiation               | Stream     |
| Extension    | 0xF0+  | User-defined channels                            | Varies     |

Channels 0xE1–0xE4 are defined in the PRISM-Mobile companion spec and are optional — clients that don't support mobile extensions simply don't advertise them.

### 5.3 Packet Format

12-byte header: Channel ID (8b) + Message Type (8b) + Flags (16b) + Timestamp µs (32b) + Payload Length (32b). Microsecond timestamps enable RTT measurement without separate pings. Compare to RDP's 20-30 byte overhead per PDU.

### 5.4 Security Model

- Curve25519 static keypairs for identity
- Noise NK handshake pattern — 1 RTT to first pixels
- First keyframe included in handshake response (~40ms to pixels on a 20ms network)
- 0-RTT reconnection via QUIC session resumption
- Tailscale-first: NAT traversal, key distribution, and device identity handled by the mesh
- No exposed ports — server drops unknown packets silently

## 6. Feature Requirements

### Phase 1 — Wire Protocol (MVP)

**Goal:** Connect from a Rust client to a Windows host over Tailscale, see the desktop, interact with keyboard and mouse.

| Requirement | Detail |
|---|---|
| **P1-TRANSPORT** | QUIC connection via `quinn` crate with RFC 9221 datagram support |
| **P1-AUTH** | Noise NK handshake with Curve25519 keys. Tailscale identity for key distribution |
| **P1-CAPTURE-WGC** | Per-window capture via Windows.Graphics.Capture API (HWND targeting) |
| **P1-CAPTURE-DDA** | Full-desktop capture via DXGI Desktop Duplication as fallback |
| **P1-ENCODE** | H.264 hardware encoding via NVENC (NVIDIA), AMF (AMD), or QSV (Intel) |
| **P1-DISPLAY** | Hybrid frame delivery: keyframes on reliable stream, P-frames on datagrams |
| **P1-INPUT** | Keyboard and mouse events sent as QUIC datagrams. Subpixel mouse coordinates |
| **P1-CURSOR** | Client-side cursor rendering. Server sends shape, client draws at local position |
| **P1-CONTROL** | Session setup, resolution negotiation, capability exchange, heartbeat |
| **P1-TRAIT** | Define platform capture trait in Rust — Windows impl only, but boundary set for macOS/Linux |

**Success criteria:** End-to-end frame latency under 30ms on a LAN. Usable for daily work (coding, browsing, document editing) without visible artifacts or input lag.

### Phase 2 — Smart Display + Browser Client

**Goal:** Region-aware encoding for sharper text. Browser client that works in Chrome — no install, just a URL.

| Requirement | Detail |
|---|---|
| **P2-REGIONS** | Frame analysis to classify screen regions as text, video, or static UI |
| **P2-LOSSLESS** | Lossless PNG/QOI encoding for text-heavy regions |
| **P2-CODECS** | H.265 and AV1 codec negotiation. Runtime capability detection on both ends |
| **P2-BROWSER** | WebTransport client with QUIC streams + datagrams |
| **P2-WEBCODECS** | Hardware-accelerated decode via `VideoDecoder` API. Codec negotiation via `isConfigSupported()` |
| **P2-ALPN** | Server exposes native QUIC + WebTransport/HTTP3 on same port (ALPN detection) |
| **P2-SAFARI** | WebSocket + MSE fallback for Safari (functional, higher latency) |
| **P2-CERTS** | Self-signed cert support via `serverCertificateHashes` — no CA required |
| **P2-CURSOR-PRED** | Predictive cursor with server correction. Input prediction for text fields |

**Success criteria:** Text is pixel-perfect in the browser client. Browser client within 5ms latency of native client on same network. Works in Chrome and Edge without any install.

### Phase 3 — Full Platform + FileShare

**Goal:** Audio, clipboard sync, file sharing, multi-monitor, and macOS/Linux server support.

| Requirement | Detail |
|---|---|
| **P3-AUDIO** | Bidirectional Opus audio. Low-latency encode/decode path |
| **P3-CLIPBOARD** | Continuous bidirectional clipboard sync (Universal Clipboard model). Content-type aware: PlainText, RichText, Image, URL, FileRef. Dedup hashing to prevent echo loops. Large payloads handed off to FileShare |
| **P3-FILESHARE-SEND** | Quick Send: tap-to-push files to the other device. Configurable landing folder. Progress tracking |
| **P3-FILESHARE-DND** | Cross-device drag-and-drop when display streaming is active |
| **P3-FILESHARE-BROWSE** | Bidirectional filesystem browsing. Lazy-loaded directory listings. Thumbnails for images |
| **P3-FILESHARE-GALLERY** | Photo gallery: thumbnail grid of recent phone photos/videos. One-tap pull to desktop |
| **P3-FILESHARE-RESUME** | Chunked transfer with ACKs. Pause/resume on connection drop. Adaptive chunk sizing |
| **P3-USBIP** | USB/IP device forwarding for peripherals |
| **P3-MULTIMON** | Multi-monitor support with per-monitor resolution and codec settings |
| **P3-MACOS** | macOS server: ScreenCaptureKit capture → IOSurface → VideoToolbox encode |
| **P3-LINUX** | Linux server: PipeWire + xdg-desktop-portal capture → DMA-BUF → VAAPI/NVENC encode |
| **P3-GLYPH-SDK** | Optional opt-in SDK for apps to export DirectWrite glyph runs to PRISM (terminals, editors) |

**Success criteria:** File transfer between phone (Chrome browser) and desktop at >80% of link bandwidth. Clipboard sync under 500ms end-to-end. macOS and Linux servers usable for basic display remoting.

### Phase 4 — Production + Mobile Clients

**Goal:** Dedicated mobile apps, notification mirroring, camera forwarding, sensor passthrough, and standalone NAT traversal.

| Requirement | Detail |
|---|---|
| **P4-ANDROID** | Android client: Rust core + Kotlin UI. Foreground service for always-on features |
| **P4-IOS** | iOS client: Rust core + Swift UI. Background execution via permitted modes |
| **P4-NOTIFY** | Notification mirroring: phone → desktop (Android NotificationListenerService). Per-app filtering, priority threshold, category filter, quiet hours, DND sync. Reply actions with inline text field |
| **P4-NOTIFY-DESKTOP** | Render mirrored notifications via native OS notification system (Toast, Notification Center, libnotify) |
| **P4-CAMERA** | Phone camera as virtual webcam on desktop. H.264/H.265 via hardware encoder on phone. Virtual webcam driver: v4l2loopback (Linux), DAL plugin (macOS), DirectShow filter (Windows) |
| **P4-CAMERA-CTRL** | Front/back camera switching, torch toggle, resolution/FPS selection, auto-orientation, zoom |
| **P4-SENSOR** | Sensor passthrough: GPS, accelerometer, gyroscope, magnetometer, barometer, ambient light, proximity. Configurable update rates per sensor type. QUIC datagrams (loss-tolerant) |
| **P4-TOUCH** | Touch interaction modes: Trackpad (relative deltas), Direct Touch (absolute 0.0–1.0), Gesture (swipe → keyboard shortcut mapping), Companion (no display stream, controls only). Switchable mid-session |
| **P4-RELAY** | Custom STUN/TURN-style relay for non-Tailscale users. Pluggable via PrismTransport trait |
| **P4-FEC** | Optional Forward Error Correction for high-loss network environments |
| **P4-INPUT-PRED** | Input prediction for text: client renders characters before server confirmation with rollback |

**Success criteria:** Android client provides notification mirroring and file sharing that feels comparable to Samsung Link to Windows. Camera forwarding recognized as a webcam by Zoom/Teams/OBS. Sensor data consumable by Android Emulator for location testing.

## 7. Platform Capture Matrix

| Platform | Capture API | Texture Format | HW Encoder | Phase |
|---|---|---|---|---|
| Windows | WGC (per-window) + DDA (full-desktop) | D3D11 | NVENC / AMF / QSV | 1 |
| macOS | ScreenCaptureKit | IOSurface | VideoToolbox | 3 |
| Linux | PipeWire + xdg-desktop-portal | DMA-BUF fd | VAAPI / NVENC | 3 |
| Android (camera) | Camera2 API | MediaCodec surface | MediaCodec | 4 |
| iOS (camera) | AVCaptureSession | CVPixelBuffer | VideoToolbox | 4 |

## 8. Browser Codec Support Matrix

| Codec | Chrome/Edge | Firefox | Safari | HW Decode Coverage |
|---|---|---|---|---|
| H.264 | Full | Shipped 2025 | VideoDecoder | ~99% all platforms |
| H.265 | Since M130 (HW) | Not in WebCodecs yet | Playback only | 75% Win / 99% Mac |
| AV1 | Full | Full | Limited | Growing (8% Win HW encode) |

Negotiation priority: H.265 → H.264 → AV1. Runtime detection via `VideoDecoder.isConfigSupported()`.

## 9. Security Requirements

| Requirement | Detail |
|---|---|
| **SEC-IDENTITY** | Curve25519 static keypairs. Identity = public key |
| **SEC-HANDSHAKE** | Noise NK pattern. 1 RTT to encrypted channel |
| **SEC-TRANSPORT** | All data encrypted via QUIC/TLS 1.3 |
| **SEC-SILENT-DROP** | Server silently drops packets without valid Noise NK initial |
| **SEC-NO-PORTS** | No fixed listening port. No port scanning surface |
| **SEC-CLIPBOARD** | Configurable content filters for sensitive data (passwords, tokens). Optional confirmation for high-entropy clipboard entries |
| **SEC-NOTIFY** | Per-app allowlist for notification mirroring. Banking apps require explicit opt-in. Notifications encrypted end-to-end — relay cannot read them |
| **SEC-CAMERA** | Explicit per-session activation. Never auto-starts. Persistent indicator on both devices. Stop button on phone notification |
| **SEC-FILESYSTEM** | Configurable scope (Downloads + Photos on phone, home dir on desktop). Optional PIN/biometric gate for filesystem browse |
| **SEC-SENSOR** | Location sharing requires explicit per-session opt-in. Sensor data never cached on receiver |
| **SEC-KEYS** | Phase 1: Tailscale mesh identity. Phase 2+: QR code pairing. Phase 4: Coordination service. Always: manual key exchange |

## 10. Non-Functional Requirements

| Requirement | Target |
|---|---|
| End-to-end frame latency (LAN) | < 16ms |
| End-to-end frame latency (WAN, 50ms RTT) | < 80ms |
| Connection establishment (first pixel) | 1 RTT (~40ms on 20ms network) |
| Reconnection (session resume) | 0-RTT |
| Clipboard sync latency | < 500ms |
| File transfer throughput | > 80% of link bandwidth |
| Packet header overhead | 12 bytes |
| Browser client latency delta vs native | < 5ms additional |
| Connection migration (Wi-Fi → cellular) | Seamless, no session drop |
| Concurrent transfers (FileShare) | Independent QUIC streams, no blocking |

## 11. Technology Stack

| Component | Technology |
|---|---|
| Language (server + native client) | Rust |
| QUIC implementation | `quinn` crate (streams + RFC 9221 datagrams) |
| Windows capture | `windows-capture` crate (WGC + DDA) |
| Video encoding | NVENC / AMF / QSV via platform APIs |
| Audio codec | Opus |
| Authentication | Noise protocol framework (NK pattern) |
| Key exchange | Curve25519 |
| Network transport (Phase 1) | Tailscale WireGuard tunnel |
| Browser transport | WebTransport over HTTP/3 |
| Browser video decode | WebCodecs API (`VideoDecoder`) |
| Browser rendering | Canvas API / VideoFrame |
| Android client | Rust core + Kotlin UI |
| iOS client | Rust core + Swift UI |
| Virtual webcam (Linux) | v4l2loopback |
| Virtual webcam (macOS) | CoreMediaIO DAL plugin |
| Virtual webcam (Windows) | DirectShow virtual camera filter |

## 12. Prior Art & Influences

| Source | What we take |
|---|---|
| Moonlight | GPU pipeline hooking for near-zero capture latency. Frame pacing |
| Parsec | Client-side cursor. Per-app window capture. Virtual displays |
| SPICE | Multi-channel architecture. Agent-based host integration |
| Waypipe | Semantic surface remoting. Per-surface encoding decisions |
| WireGuard | Noise handshake. Cryptokey routing. Silent packet drop |
| QUIC / HTTP3 | Stream multiplexing. Connection migration. 0-RTT. RFC 9221 datagrams |
| Tailscale | DERP relay architecture. >90% direct P2P. Peer Relays |
| Tango ADB | Proof that WebTransport + WebCodecs works for real-time video decode |
| KDE Connect | Notification mirroring. Clipboard sync. File sharing. Plugin model |
| Scrcpy | Low-latency display/control. Camera forwarding. Codec negotiation |
| Apple Continuity | Universal Clipboard UX. Continuity Camera quality. Zero-config pairing |
| Samsung DeX/Link | Notification action forwarding. App-aware file routing |
| LocalSend/Snapdrop | "Tap to send" UX. Zero-setup file sharing |
| Intel Unison | Photo gallery quick-access without full filesystem mount |

## 13. Implementation Roadmap

```
Phase 1 — Wire Protocol                              ← CURRENT
├── Rust server (Windows) + Rust client
├── QUIC via quinn + RFC 9221 datagrams
├── WGC + DDA capture → H.264 HW encode
├── Noise NK auth over Tailscale
├── Keyboard/mouse input via datagrams
└── Platform capture trait definition

Phase 2 — Smart Display + Browser Client
├── Region classification (text / video / static)
├── Lossless text-region encoding
├── H.265 / AV1 codec negotiation
├── Browser client (WebTransport + WebCodecs)
├── Client-side predictive cursor
└── Self-signed cert support (serverCertificateHashes)

Phase 3 — Full Platform + FileShare
├── Audio (Opus), continuous clipboard, USB/IP
├── FileShare: quick send, drag-drop, browse, gallery
├── Multi-monitor support
├── macOS server (ScreenCaptureKit)
├── Linux server (PipeWire + DMA-BUF)
└── Optional glyph-run SDK

Phase 4 — Production + Mobile
├── Android client (Rust + Kotlin)
├── iOS client (Rust + Swift)
├── Notification mirroring with actions
├── Camera forwarding (virtual webcam)
├── Sensor passthrough (GPS, accel, gyro)
├── Touch interaction modes (4 modes)
├── Custom relay for non-Tailscale users
├── FEC for high-loss networks
└── Extension channel SDK
```

## 14. Open Questions

### Core Protocol (from spec v0.2)

1. **WGC yellow border on Windows 10** — Mandatory on Win10, optional on Win11. Use DDA as default on Win10?
2. **WebTransport Safari timeline** — No Safari support. WebSocket + MSE fallback is functional but adds latency.
3. **Text region detection accuracy** — Heuristic-based classification needs benchmarking in Phase 2. What's the acceptable false-positive/negative rate?
4. **QUIC congestion control tuning** — Default CC may favor throughput over latency. Evaluate BBRv2 low-latency mode.
5. **Virtual display for headless servers** — Windows Indirect Display Driver (IDD) API. Research needed for Phase 2.

### Mobile Extensions (from companion spec v0.1)

6. **iOS notification access** — No equivalent to Android's NotificationListenerService. Shortcuts/Automation is a possible workaround, or accept iOS limitation.
7. **Virtual webcam installation** — Requires driver install (v4l2loopback, DAL plugin, DirectShow filter). Bundle with PRISM or separate install step?
8. **Background execution on mobile** — iOS aggressively kills background apps. Android foreground service with notification is the path. iOS: explore permitted background modes.
9. **Clipboard OS banners** — Android 13+ and iOS 14+ show clipboard access banners. Continuous sync would trigger these constantly. Sync on explicit trigger only?
10. **Cross-device drag-and-drop latency** — DnD requires tight coordination between Input channel (drag position) and FileShare (payload). Deferred or streamed transfer?
11. **Sensor consumer APIs** — How do desktop apps consume forwarded sensor data? Options: virtual HID, local WebSocket API, named pipe, or framework integration (Unity, Android Emulator).

## 15. Success Metrics

| Metric | Phase 1 | Phase 2 | Phase 3 | Phase 4 |
|---|---|---|---|---|
| Frame latency (LAN) | < 30ms | < 20ms | < 16ms | < 16ms |
| Connection time | < 100ms | < 100ms | < 100ms | < 100ms |
| Daily-drivable for coding | Yes | Yes | Yes | Yes |
| Browser client works | — | Chrome/Edge | + Firefox | + Safari fallback |
| File transfer | — | — | > 80% bandwidth | > 80% bandwidth |
| Clipboard sync | — | — | < 500ms | < 500ms |
| Platform servers | Windows | Windows | + macOS, Linux | + macOS, Linux |
| Mobile clients | — | — | Browser only | Android, iOS |
| Notification mirroring | — | — | — | Android → Desktop |
| Camera forwarding | — | — | — | Phone → Desktop |

---

*PRISM Protocol Specification v0.2.0 + PRISM-Mobile Extensions v0.1.0 — CC0 Public Domain*
