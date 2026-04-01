<p align="center">
  <h1 align="center">PRISM</h1>
  <p align="center"><strong>Protocol for Remote Interactive Streaming & Multiplexing</strong></p>
  <p align="center">The modern alternative to RDP. Zero-copy GPU capture, H.264 hardware encoding, QUIC transport, Noise IK encryption — built from scratch in Rust.</p>
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> &bull;
  <a href="#features">Features</a> &bull;
  <a href="#architecture">Architecture</a> &bull;
  <a href="#performance">Performance</a> &bull;
  <a href="#security">Security</a> &bull;
  <a href="#configuration">Configuration</a> &bull;
  <a href="#roadmap">Roadmap</a>
</p>

<p align="center">
  <img alt="Tests" src="https://img.shields.io/badge/tests-754%20passing-brightgreen">
  <img alt="Crates" src="https://img.shields.io/badge/crates-11-blue">
  <img alt="Lines" src="https://img.shields.io/badge/lines-33K%20Rust-orange">
  <img alt="License" src="https://img.shields.io/badge/license-AGPL--3.0-blue">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2010%2B-0078D4">
</p>

---

## Why PRISM?

RDP was designed in 1998. It uses TCP, GDI rendering, and a protocol that predates H.264. Modern remote desktop demands better:

| | RDP | PRISM |
|---|---|---|
| **Transport** | TCP (head-of-line blocking) | QUIC (multiplexed, 0-RTT) |
| **Video codec** | RemoteFX / H.264 (software) | H.264/H.265/AV1 (GPU-accelerated) |
| **Encryption** | TLS 1.2 | Noise IK + QUIC TLS 1.3 |
| **Latency** | 30-60ms minimum | <16ms (input-triggered capture) |
| **Capture** | GDI/mirror driver | DXGI Desktop Duplication (zero-copy GPU) |
| **Quality adaptation** | Server-side only | Bidirectional: probes + client feedback + degradation ladder |
| **Audio** | Virtual audio device | WASAPI loopback (system audio, no driver) |
| **Multi-client** | One session per user | Multiple viewers, channel ownership model |
| **Extensibility** | Fixed protocol | Channel-based: add new features without breaking existing |

---

## Quick Start

### How It Works

PRISM has two parts:

| | **Server** (`prism-server.exe`) | **Client** (`prism-client.exe`) |
|---|---|---|
| **What it does** | Captures the screen, encodes video, sends it | Receives video, decodes it, shows it in a window |
| **Where to run** | On the computer you want to **control remotely** | On the computer you're **sitting at** |
| **Required?** | Yes — one per remote machine | Yes — one per viewer |

Both computers need Windows 10+. Both run the same download — just launch different `.exe` files.

### Option A: Download (Recommended)

1. Go to [Releases](https://github.com/Emperiusm/PRISM/releases/latest)
2. Download `prism-vX.X.X-windows-x64.zip`
3. Extract it — you get `prism-server.exe` and `prism-client.exe`

> **SmartScreen warning:** Windows may show "Windows protected your PC" because the exe is unsigned. Click **More info** → **Run anyway**. This is safe — you can verify the source code yourself.

### Option B: Build from Source

```bash
# Requires Rust 1.85+ — https://rustup.rs
git clone https://github.com/Emperiusm/PRISM.git
cd PRISM
cargo build --release -p prism-server -p prism-client
# Binaries are in target/release/
```

### Step 1: Start the Server (on the remote computer)

```bash
# Share your real desktop
prism-server.exe --dda

# Or test locally with a synthetic test pattern (no desktop capture)
prism-server.exe
```

The server starts listening on port **7000**. Note the computer's IP address (run `ipconfig` to find it, e.g. `192.168.1.100`).

### Step 2: Connect the Client (on your computer)

```bash
# Direct connect — replace with the server's IP
prism-client.exe 192.168.1.100:7000
```

A window opens showing the remote desktop. Your mouse and keyboard are forwarded — you're controlling the remote machine.

**To test on a single machine** (both server and client on the same PC):

```bash
# Terminal 1: start server
prism-server.exe --dda

# Terminal 2: connect client to localhost
prism-client.exe 127.0.0.1:7000
```

### Step 3: Enable Encryption (Recommended)

Without encryption, video and input travel unencrypted over the network. To enable end-to-end encryption:

```bash
# Server — add --noise flag. It prints a 64-character hex public key on startup.
prism-server.exe --dda --noise
# Output: Server public key: a3f1...beef

# Client — paste the server's public key after --noise
prism-client.exe 192.168.1.100:7000 --noise a3f1...beef
```

First connection is auto-paired (SSH-style TOFU). Subsequent connections are recognized instantly.

### Firewall / Network Notes

- The server listens on **UDP port 7000** (QUIC protocol)
- If connecting over a local network (same Wi-Fi/Ethernet), it should work immediately
- If connecting over the internet, you need to **port-forward UDP 7000** on the server's router, or use a VPN
- Windows Firewall may prompt to allow `prism-server.exe` through — click **Allow**

### Multi-Monitor

```bash
# Capture the second monitor (0-indexed)
prism-server.exe --dda --monitor 1
```

### Controls

| Action | How |
|--------|-----|
| Move mouse / type | Just use your mouse and keyboard — input is forwarded |
| Copy/paste | Clipboard syncs automatically between machines |
| Disconnect | Close the client window or press Escape |

---

## Features

### Interactive Client (Glassmorphism UI)

- **GPU-accelerated renderer** — custom wgpu pipeline replacing minifb. Compute shader YUV→RGB conversion (<0.1ms vs ~3ms CPU), ring-buffered texture upload, two-pass Gaussian blur for frosted glass.
- **Launcher window** — quick-connect hero bar with autocomplete, saved server card grid with hover animations, accent color stripes, add/edit/delete. Servers persisted via crash-safe append-only log with compaction (`~/.prism/servers.json`).
- **In-session overlay** — double-tap Left Ctrl to toggle. Stats bar with color-coded metrics (FPS, latency, codec, bandwidth), profile switcher. Four floating sub-panels: Performance (sparkline graphs), Quality (encoder/FPS/bandwidth controls), Connection (info + disconnect), Display (monitor selector map).
- **Glassmorphism design** — deep purple/indigo palette (#0d0b1a → #1a1035), frosted glass panels with noise texture, gradient borders, spring animations with elastic overshoot. 3 GPU draw calls for the entire UI via instanced batching.
- **10 purpose-built widgets** — Label, Button, Separator, Checkbox, Slider, Sparkline, TextInput, Dropdown, MonitorMap. Spatial hash hit testing, arena-friendly draw commands, dirty rect tracking.
- **Zero-latency overlay toggle** — 3-state double-tap detector (300ms window). Input routed to overlay when visible, forwarded to remote when hidden. Event coalescing reduces per-frame work.

### Display Streaming

- **DXGI Desktop Duplication (DDA)** — captures the composited desktop directly from the GPU. Zero CPU pixel copies. Works at native resolution (tested at 4K/2560x1440).
- **H.264 hardware encoding** — automatic GPU detection: NVENC (NVIDIA) > QSV (Intel) > AMF (AMD) > software fallback. Ultra-low-latency preset: zero lookahead, zero B-frames, CBR.
- **Adaptive frame rate** — 2fps when desktop is idle, scales to 15fps+ on active content. DDA damage detection drives capture rate.
- **Backpressure** — when the network or client is slow, frames are skipped instead of queued. Prevents latency growth.
- **Persistent QUIC streams** — one stream per client with length-prefixed framing. No per-frame stream setup overhead.
- **Input-triggered capture** — keyboard/mouse events trigger an immediate frame capture, bypassing the frame pacer. Cuts up to 16ms of perceived latency.
- **Speculative IDR** — scene changes (idle→active transitions) automatically trigger keyframes for instant visual refresh.
- **Static region caching** — tracks frame hashes to detect unchanging regions. Reports potential bandwidth savings of 40-60% for typical desktop sessions.

### Input Forwarding

- **Full keyboard** — scancodes + virtual key codes via Win32 `SendInput`. Handles all keys including modifiers, function keys, numpad.
- **Unicode text input** — `KEYEVENTF_UNICODE` for CJK, emoji, composed characters.
- **Mouse** — absolute positioning (0-65535 normalized), relative mode for games/FPS, scroll wheel.
- **Zero-latency cursor** — client renders cursor at local mouse position instantly. Server sends corrections only when prediction diverges >5px.
- **Pre-built packet templates** — input datagram headers are built once and patched per-event. Eliminates repeated serialization.

### Audio

- **WASAPI loopback capture** — captures all system audio without a virtual audio device. No driver installation required.
- **Opus encoding** — 48kHz stereo at 128kbps. 20ms frames = 50 packets/sec = 11KB/sec bandwidth.
- **Silence detection** — RMS-based at -60dB. Stops sending during silence, saves bandwidth during coding sessions.
- **Adaptive jitter buffer** — 20ms on LAN, grows to 80ms on WAN. Automatically tunes to network conditions.

### Clipboard

- **Bidirectional text sync** — copy on server, paste on client (and vice versa).
- **Hash-based echo suppression** — FNV-1a hash prevents clipboard loops. More robust than sequence-number approaches.
- **Cross-platform** — server uses Win32 clipboard APIs, client uses `arboard` (works on Windows, macOS, Linux).

### Quality Adaptation

- **Proactive probing** — server sends probe datagrams every 2 seconds. Client echoes them back for RTT measurement.
- **ConnectionQuality scoring** — composite score (0.0-1.0) from RTT, loss rate, jitter, bandwidth, and one-way delay asymmetry.
- **Profile-specific degradation** — Gaming profile drops resolution before FPS. Coding profile drops FPS before resolution (preserves text sharpness).
- **Hysteresis** — 2-second downgrade hold (fast reaction), 10-second upgrade hold (prevents flapping).
- **Runtime encoder reconfigure** — bitrate adjusts based on quality score. >20% change triggers encoder recreation with new parameters.
- **ArcSwap quality cache** — quality score computed at ~2Hz, read at ~60Hz. Lock-free pointer swap = ~1ns per read.

### Session Management

- **Channel-based architecture** — Display, Input, Audio, Control, Clipboard, FileShare, Device, and more. Each channel has its own ownership model and priority.
- **Exclusive/Shared/Transferable ownership** — Display is exclusive (one viewer controls). Clipboard is shared (all viewers see copies). Touch is transferable (one at a time, can be passed).
- **ArcSwap routing table** — lock-free reads at ~5ns. Updated atomically on connect/disconnect.
- **Tombstone reconnection** — disconnected clients get a 5-minute tombstone. Reconnect with the same identity = session restored with channel recovery.
- **Heartbeat monitoring** — 5-second interval, zero-allocation pre-built packets. 10s silence → suspend, 60s → tombstone.
- **Client auto-reconnect** — 3-second retry interval, up to 100 attempts (5 minutes). Transparent to the user.
- **Graceful shutdown** — Ctrl+C sends ShutdownNotice to all clients, waits grace period, persists tombstones.

### Security

- **Noise IK protocol** — `Noise_IK_25519_ChaChaPoly_SHA256`. Mutual authentication in 1 round-trip. Client's static key transmitted encrypted.
- **Trust-on-first-use (TOFU)** — SSH-style pairing. First connection auto-pairs. Key change = connection refused with security warning.
- **Client identity persistence** — keypair saved to `~/.prism/client_identity.json`. Survives restarts.
- **Connection rate limiting** — token bucket per IP (10 connections/minute). Prevents connection flooding.
- **Audit trail** — ring buffer logs connect/disconnect events with device identity.
- **Channel-level permissions** — SecurityContext defines per-channel Allow/Deny/NeedsConfirmation decisions.

### Observability

- **Structured logging** — `tracing` crate with info/warn/error/debug levels. No more println.
- **Overlay HUD** — 128-byte binary packet sent every 100ms with FPS, bitrate, RTT, loss, resolution, per-channel bandwidth.
- **Frame tracing** — adaptive sampling captures slow frames (>p95) and uniform 1-in-60 baseline. Full pipeline latency breakdown: capture → encode → send → network → decode → render.
- **Per-client metrics** — atomic counters isolated per client. No cross-client interference.
- **Time-series history** — 5-minute ring buffer (300 samples at 1/sec) for sparkline graphs.
- **Bandwidth arbiter** — priority-weighted proportional allocation. Critical (input) > High (display) > Normal (control) > Low (file transfer).

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     PRISM Server                         │
│                                                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐ │
│  │ DDA      │  │ H.264    │  │ Session  │  │ Control │ │
│  │ Capture  │→ │ Encoder  │→ │ Manager  │← │ Channel │ │
│  └──────────┘  └──────────┘  └──────────┘  └─────────┘ │
│        ↑                          ↕              ↕       │
│  ┌──────────┐              ┌──────────┐  ┌──────────┐   │
│  │ Input    │              │ Routing  │  │ Quality  │   │
│  │ Injector │              │ Table    │  │ Monitor  │   │
│  └──────────┘              └──────────┘  └──────────┘   │
│        ↑                        ↕                        │
│  ┌─────────────────────────────────────────────────┐     │
│  │              QUIC Transport (quinn)              │     │
│  │  Latency socket (BBR, DSCP EF, datagrams)       │     │
│  │  Throughput socket (Cubic, DSCP AF11, streams)   │     │
│  └─────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────┘
                           ↕ QUIC/TLS 1.3
┌─────────────────────────────────────────────────────────┐
│                     PRISM Client                         │
│                                                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐ │
│  │ H.264    │  │ wgpu     │  │ Input    │  │ Cursor  │ │
│  │ Decoder  │→ │ Renderer │  │ Router   │  │ Predict │ │
│  └──────────┘  └──────────┘  └──────────┘  └─────────┘ │
│        ↓            ↑ ↓            ↑                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
│  │ Stream   │  │ Glass UI │  │ Session  │               │
│  │ Texture  │  │ Overlay  │  │ Bridge   │               │
│  └──────────┘  └──────────┘  └──────────┘               │
└─────────────────────────────────────────────────────────┘
```

### Crate Map

| Crate | Purpose | Dependencies |
|---|---|---|
| `prism-protocol` | Wire format, headers, channels, capabilities, input events | `bytes`, `serde` |
| `prism-metrics` | Lock-free AtomicHistogram, MetricsRecorder, RateCounter | none |
| `prism-security` | Identity, pairing, Noise IK, SecurityGate, audit | `snow`, `x25519-dalek` |
| `prism-transport` | PrismConnection trait, QUIC, quality measurement | `quinn`, `tokio` |
| `prism-observability` | Frame tracing, client feedback, overlay, time-series | `prism-metrics` |
| `prism-session` | Channels, routing, tombstones, arbiter, dispatch | `arc-swap` |
| `prism-display` | Pipeline types, FrameRing, classification, degradation | `prism-protocol` |
| `prism-platform-windows` | DDA capture, NVENC config, texture pool, D3D11 | `windows` |
| `prism-server` | ServerApp, handlers, quality, frame sender, overlay | all above |
| `prism-client` | Interactive client, wgpu renderer, glassmorphism UI, launcher, overlay | `wgpu`, `winit`, `glyphon`, `openh264` |

---

## Performance

### Latency Budget (LAN, 1080p)

| Stage | Time |
|---|---|
| DDA capture | <1ms |
| BGRA→YUV conversion | ~5ms (CPU) / <0.1ms (NV12 GPU) |
| H.264 encode (software) | ~15ms |
| H.264 encode (NVENC) | <1ms |
| QUIC send | <1ms |
| Network (LAN) | <1ms |
| H.264 decode | ~3ms |
| Render to window | <1ms |
| **Total (software)** | **~25ms** |
| **Total (NVENC)** | **~7ms** |

### Bandwidth

| Content | Bitrate | Notes |
|---|---|---|
| Static desktop | ~0 Kbps | DDA reports no damage, no frames sent |
| Active coding | 1-3 Mbps | Adaptive FPS, text regions |
| Video playback | 5-15 Mbps | Full motion, H.264 CBR |
| 4K desktop | 10-30 Mbps | Software encoder |
| Audio | 11 KB/sec | Opus 128kbps stereo |
| Input events | 3 KB/sec | 24-byte datagrams at 125Hz |
| Heartbeat | 160 B/sec | 16 bytes every 5 seconds |

### Optimizations

| ID | Optimization | Impact |
|---|---|---|
| S11 | Zero-allocation heartbeat (pre-built `Bytes`) | 0 heap alloc per heartbeat |
| S12 | ArcSwap quality cache | ~1ns reads vs ~5us recompute |
| P1 | Pre-built packet header templates | ~5 fewer buffer ops per packet |
| R32 | Input-triggered capture | Up to 16ms faster response |
| R36 | FrameRing SPSC (cache-line padded) | Zero-alloc capture→encode handoff |
| T16 | Datagram coalescing (1ms window) | 50% fewer syscalls |
| T17 | Pre-opened stream pool | Zero stream-open latency |

---

## Security

PRISM uses layered security:

1. **QUIC TLS 1.3** — transport encryption on every connection
2. **Noise IK** — application-layer mutual authentication with forward secrecy
3. **Trust-on-first-use** — SSH-style device pairing without a CA
4. **Channel permissions** — per-device, per-channel Allow/Deny decisions
5. **Rate limiting** — token bucket prevents connection floods
6. **Silent drop** — unknown/blocked devices get no response (indistinguishable from "host doesn't exist")
7. **Audit logging** — all authentication events recorded in a ring buffer

```bash
# Generate server identity and enable Noise IK
cargo run -p prism-server -- --noise --dda
# Server public key: a3f1...beef (64 hex chars)

# Client connects with server's key
cargo run -p prism-client -- 192.168.1.100:7000 --noise a3f1...beef
# First connection: auto-paired via TOFU
# Subsequent connections: recognized instantly
```

---

## Configuration

### Generate a Config File

```bash
# Generate a fully commented prism-server.toml with all defaults
prism-server --init

# Or when running from source:
cargo run --release -p prism-server -- --init
```

This creates `prism-server.toml` in the working directory. All keys are optional — missing keys use sensible defaults.

### Config File Reference

```toml
# Network
listen_addr_str = "0.0.0.0:7000"       # Main QUIC endpoint (video, audio, input)
throughput_addr_str = "0.0.0.0:7001"    # Bulk transfer endpoint

# Limits
max_clients = 4                          # Max simultaneous clients
total_bandwidth_bps = 100000000          # 100 Mbps aggregate cap

# Display
display_name = "PRISM Server"            # Name shown to clients

# Session management
heartbeat_suspend_secs = 10              # Silence before session suspend
heartbeat_tombstone_secs = 60            # Suspend before tombstone
tombstone_max_age_secs = 300             # Tombstone before permanent removal

# Security & Identity
identity_path = "identity.key"           # Noise IK key file (auto-generated)
pairing_path = "pairing.json"            # Approved devices registry
tombstone_path = "tombstones.json"       # Session resurrection store
```

### Server CLI Flags

| Flag | Description |
|---|---|
| `--dda` | Use DXGI Desktop Duplication (real desktop capture) |
| `--noise` | Enable Noise IK end-to-end encryption |
| `--monitor <N>` | Select monitor to capture (0-indexed, default: 0) |
| `--port <PORT>` | Override listen port (default: 7000) |
| `--bind <ADDR>` | Override bind address (e.g., `192.168.1.5:7000`) |
| `--config <PATH>` | Path to TOML config file (default: `prism-server.toml`) |
| `--init` | Generate default `prism-server.toml` and exit |
| `--help` | Print help and exit |
| `--version` | Print version and exit |

CLI flags override config file values.

### Client CLI Flags

| Flag | Description |
|---|---|
| *(no args)* | Open launcher (connection manager) |
| `HOST:PORT` | Direct connect (skips launcher, exits on disconnect) |
| `--noise <KEY>` | Server's Noise IK public key (64-char hex) |
| `--config <PATH>` | Custom config directory (default: `~/.prism`) |
| `--init` | Generate default `servers.json` and exit |
| `--help` | Print help and exit |
| `--version` | Print version and exit |

### Hardware Encoding

GPU-accelerated encoding requires FFmpeg libraries:

```bash
# Install FFmpeg (Windows — via vcpkg or download from gyan.dev)
# Then build with hardware encoding:
cargo run -p prism-server --features hwenc -- --dda
```

The encoder auto-detects: NVENC > QSV > AMF > software fallback.

### File Locations

| File | Location | Created |
|------|----------|---------|
| Server config | `./prism-server.toml` | `--init` or manual |
| Server identity | `./identity.key` | Automatic on first run |
| Paired devices | `./pairing.json` | Automatic on first pairing |
| Session tombstones | `./tombstones.json` | Automatic |
| Client identity | `~/.prism/client_identity.json` | Automatic on first run |
| Saved servers | `~/.prism/servers.json` | `--init` or automatic |
| Shader cache | `~/.prism/shader_cache/` | Automatic on first run |

---

## Development

### Building from Source

```bash
git clone https://github.com/Emperiusm/PRISM.git
cd PRISM
cargo build                    # Debug build
cargo build --release          # Optimized (LTO, strip)
cargo test --workspace         # Run all 754 tests
cargo clippy --workspace       # Zero warnings
```

### Project Stats

```
11 crates | 754 tests | 33K lines of Rust | 184 source files | 0 clippy warnings
```

### Release Build Profile

The release profile is configured for maximum performance:

```toml
[profile.release]
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
opt-level = 3
```

---

## Roadmap

### Phase 1 (Current)

- [x] DXGI Desktop Duplication capture
- [x] H.264 software + hardware encoding (NVENC/QSV/AMF)
- [x] QUIC transport with dual-connection architecture
- [x] Keyboard/mouse input forwarding with Win32 SendInput
- [x] Noise IK mutual authentication
- [x] Adaptive quality with degradation ladder
- [x] Clipboard sync (bidirectional text)
- [x] Audio types + synthetic source
- [x] Session management with tombstone reconnection
- [x] TOML configuration
- [x] Structured logging (tracing)
- [x] Performance overlay HUD

### Phase 1.5 (Current)

- [x] Interactive client with glassmorphism UI (wgpu renderer)
- [x] Launcher window with saved server management
- [x] In-session overlay (stats, quality, connection, display panels)
- [x] Custom widget system (10 widgets, GPU-batched rendering)
- [x] SessionBridge channel architecture (UI↔Network)
- [x] Server parallel stream acceptance (Noise + capability concurrent)
- [ ] Wire launcher → connection → stream full lifecycle
- [ ] Live server thumbnail previews on launcher cards
- [ ] mDNS/DNS-SD server auto-discovery

### Phase 2

- [ ] GPU compute shader region classification (sub-window detection)
- [ ] Multi-client active sessions
- [ ] HW lossless encoding for text regions
- [ ] Scroll prediction metadata
- [ ] Client-side static region atlas caching
- [ ] File transfer channel
- [ ] SPAKE2 short-code pairing

### Phase 3

- [ ] macOS capture (ScreenCaptureKit)
- [ ] Linux capture (PipeWire + DMA-BUF)
- [ ] Touch/pen/gamepad input
- [ ] Spatial audio
- [ ] Browser client (WebTransport)

### Phase 4

- [ ] WebSocket/TCP fallback transport
- [ ] Transport probing cascade
- [ ] Hot-switching between transports
- [ ] PRISM relay for NAT traversal

---

## Acknowledgments

PRISM was built with these excellent Rust crates:

- [quinn](https://github.com/quinn-rs/quinn) — QUIC protocol implementation
- [snow](https://github.com/mcginty/snow) — Noise protocol framework
- [openh264](https://github.com/ralfbiedert/openh264-rs) — H.264 codec
- [wgpu](https://github.com/gfx-rs/wgpu) — GPU-accelerated rendering
- [winit](https://github.com/rust-windowing/winit) — Cross-platform windowing
- [glyphon](https://github.com/grovesNL/glyphon) — GPU text rendering
- [windows-rs](https://github.com/microsoft/windows-rs) — Win32 API bindings

---

## License

PRISM is dual-licensed:

- **Open Source:** [GNU Affero General Public License v3.0](LICENSE) — free for open-source use. Any network-accessible deployment of modified PRISM code must release the source.
- **Commercial:** A proprietary license is available for organizations that cannot comply with the AGPL. See [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) for details.

Copyright 2025-2026 Ehsan Khalid. All rights reserved.

---

<p align="center">
  <strong>PRISM</strong> — Remote desktop, reimagined.
</p>
