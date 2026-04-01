# PRISM Interactive Client — Design Specification

**Date:** 2026-04-01
**Status:** Approved
**Scope:** Launcher window + in-session overlay with glassmorphism UI, powered by a custom wgpu renderer

---

## 1. Overview

Replace the current CLI-only, minifb-based PRISM client with an interactive client that provides:

- **Launcher window** — connection manager with saved servers, quick-connect, and pairing UX
- **In-session overlay** — runtime settings, performance stats, and session management over the live stream
- **Custom GPU renderer** — wgpu-based pipeline with compute shader YUV→RGB, multi-pass Gaussian blur, and frosted glass compositing
- **Glassmorphism visual identity** — deep purple/indigo palette, frosted glass panels with noise texture, spring animations

CLI backward compatibility is preserved: `prism-client HOST:PORT` skips the launcher and connects directly, with the overlay still available.

**Audience:** Both power users (CLI bypass, hotkeys) and general desktop users (click-to-connect, visual settings).

---

## 2. Visual Identity

### Color Palette

| Role | Value | Usage |
|------|-------|-------|
| Background dark | `#0d0b1a` | Gradient start, deep corners |
| Background light | `#1a1035` | Gradient end, radial center |
| Glass tint | Purple at 10-15% opacity | Panel fill over blurred content |
| Glass border | White at 20% opacity | 1px panel edges, gradient stroke (bright top, fade bottom) |
| Glass noise | White at 3% opacity | Noise texture overlay on all glass surfaces |
| Accent primary | `#8b5cf6` | Interactive elements, active states, glows |
| Accent secondary | `#06b6d4` | Optional alternate (future: user-configurable) |
| Text primary | `#ffffff` at 90% | Headings, values |
| Text secondary | `#ffffff` at 60% | Labels, timestamps |
| Warning | Warm yellow | Degraded metrics (FPS <30) |
| Critical | Red | Problem metrics (latency >20ms) |

### Glassmorphism Properties

Each frosted glass panel has three visual layers:

1. **Outer glow** — accent-tinted, 2px spread, 5% opacity
2. **Glass surface** — blurred stream/background sampled at screen-space position + purple tint + noise texture
3. **Inner highlight** — 1px top border at 15% white (simulates light catching glass edge)

Drop shadows under panels are tinted purple (not black) for palette cohesion.

Panels at different Z-depths receive different blur intensities — front panels blur more, creating real depth perception.

The purple tint shifts subtly based on underlying content brightness — a natural result of correct alpha blending over the blurred stream.

### Typography

- **Sans-serif:** Inter (bundled) — medium weight for labels, semibold for values
- **Monospace:** JetBrains Mono (bundled) — for numeric values (FPS, latency, bandwidth) to prevent jitter on digit width changes
- Subpixel positioning disabled — whole-pixel rounding for crisper rendering on glass surfaces

---

## 3. Renderer Architecture

### Current Pipeline (replaced)

```
Server → H.264 NAL → openh264 decode → YUV420 (CPU) → RGB (CPU, per-pixel) → minifb pixel buffer
```

### New Pipeline

```
Server → H.264 NAL → openh264 decode → YUV420 planes → GPU texture upload → YUV→RGB compute shader → stream_texture

stream_texture → blur downsample shader → blurred_texture (for glass panels)

Render pass:
  1. Full-screen quad: stream_texture (remote desktop)
  2. Screen-dim quad: semi-transparent dark overlay (only when overlay active)
  3. Per-panel: sample blurred_texture region + tint/noise/border composite (frosted glass)
  4. Text/icons: glyphon glyph atlas on top of panels
  5. Cursor: predicted cursor sprite (replaces 8x8 crosshair)
```

### Core Components

- **`PrismRenderer`** — owns `wgpu::Device`, `Surface`, render pipelines. Orchestrates multi-pass frame.
- **`StreamTexture`** — ring-buffered `wgpu::Texture` updated each frame from decoded YUV planes. Runs YUV→RGB compute shader.
- **`BlurPipeline`** — two-pass Gaussian blur (horizontal + vertical) on downsampled stream texture copy. Runs at quarter resolution (480x270 for 1080p).
- **`GlassPanel`** — quad that samples blur texture at screen-space position, applies tint, border, corner radius, noise.
- **`TextRenderer`** — wraps `glyphon` for GPU text layout and rasterization.

### Windowing

`winit` replaces `minifb` for window creation and event handling. Provides proper input events, window resize, DPI awareness, and multi-monitor support.

### GPU Optimizations

- **Compute shader YUV→RGB** — replaces CPU per-pixel conversion (~2-3ms → <0.1ms per frame at 1080p). Upload raw Y, U, V planes as separate textures, sample in shader.
- **Ring buffer texture upload** — double-buffer the stream texture. GPU reads texture A while CPU uploads next frame to texture B. No stalls.
- **Lazy blur** — only run blur pipeline when overlay visible AND stream texture changed. Zero cost when overlay hidden.
- **Progressive blur resolution** — during fade-in animation, start blur at 1/8th resolution, step to 1/4th as animation completes.
- **Subgroup/wave ops for blur** — use subgroup shuffle operations to share samples across threads within a wavefront. Cuts texture reads ~40%.
- **Zero-copy YUV upload** — map GPU staging buffer directly, have openh264 decode into it. Eliminates one full-frame memcpy (3.1 MB at 1080p).
- **Async frame upload** — `wgpu::Queue::write_texture` on separate thread. Overlap upload of frame N+1 with render of frame N.
- **Pipeline derivatives** — glass, glow, and text pipelines share vertex format, differ only in fragment shader. GPU driver optimizes state transitions.
- **Depth buffer panel occlusion** — render panels front-to-back with depth testing. Fragments behind opaque panel regions killed before fragment shader runs.
- **Bindless texture array** — glyph atlas, noise texture, and baked panel textures in single texture array. One bind group for all UI rendering.

### Render Loop

```
Frame tick (vsync or 60fps target):
  ├─ IF new decoded frame ready:
  │    └─ Upload YUV planes to ring buffer texture (async, non-blocking)
  ├─ Render pass 1: stream quad (always, ~0.05ms)
  ├─ IF overlay visible AND stream texture dirty:
  │    └─ Render pass 2: blur downsample (~0.2ms at 1/4 res)
  ├─ IF overlay visible:
  │    ├─ Render pass 3: dim + glass panels (batched single draw, ~0.1ms)
  │    └─ Render pass 4: text atlas (single draw, ~0.05ms)
  └─ Present

Overhead when overlay hidden: ~0ms (stream quad cheaper than minifb blit)
Overhead when overlay visible: ~0.4ms on integrated graphics
```

---

## 4. Launcher Window

### Window Properties

- Default size: 960x640, resizable, minimum 720x480
- Title: "PRISM"
- DPI-aware via winit (logical pixels, scaled to physical)
- Same wgpu renderer as stream client

### Layout

```
┌─────────────────────────────────────────────────┐
│  Background: radial gradient #0d0b1a → #1a1035  │
│                                                  │
│  ┌─────────────────────────────────────────┐     │
│  │  PRISM                    [⚙]  [─][□][×]│    │
│  │         glass title bar                  │     │
│  └─────────────────────────────────────────┘     │
│                                                  │
│  ┌─────────────────────────────────────────┐     │
│  │  ┌───────────────────────────┐ [Connect]│     │
│  │  │ 192.168.1.100:7000        │          │     │
│  │  └───────────────────────────┘          │     │
│  │  Hero: Quick Connect bar (frosted glass) │     │
│  └─────────────────────────────────────────┘     │
│                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │ ● Online │  │ ○ Offline│  │  + Add   │       │
│  │ Work PC  │  │ Home Lab │  │  Server  │       │
│  │ 10.0.0.5 │  │ 192.168  │  │  dashed  │       │
│  │ h265 1080p│ │ Last: 2d │  │  border  │       │
│  │ Gaming   │  │ Coding   │  │          │       │
│  └──────────┘  └──────────┘  └──────────┘       │
│       Server card grid (responsive wrap)         │
└─────────────────────────────────────────────────┘
```

### Quick Connect Bar

- Frosted glass panel, slightly elevated from card grid
- Text input with placeholder "Enter server address..."
- Optional noise key field (hidden by default, revealed via lock icon)
- Connect button with accent color glow (#8b5cf6)
- Enter key triggers connect
- Auto-complete from saved servers (trie-based prefix lookup, <1μs)

### Server Cards

- Frosted glass surface with subtle purple tint
- Accent color stripe on left edge (auto-generated from server name hash, or user-picked)
- Content: display name (large), address (muted), last profile, resolution/codec from last session
- Status dot: green pulse animation (breathing, 2s sine cycle) = online, gray = unknown/offline
- Last connected: relative timestamp ("2 hours ago", "3 days ago")
- Hover: card lifts (Y translate + shadow increase), border brightens
- Click: connect immediately with last-used settings
- Right-click: context menu (Edit, Delete, Copy Address, Duplicate)
- "+ Add Server" card: dashed border, no glass fill

### Add/Edit Server Form

- Glass panel slides in overlaying the card grid
- Fields: Display Name, Address (host:port), Noise Public Key (optional), Default Profile (dropdown), Accent Color picker
- Save / Cancel buttons

### Settings Panel (gear icon)

- Floating glass panel over launcher
- Sections: default identity path, about/version info
- Minimal for v1

### Connection Flow

1. User clicks card or hits Enter in quick connect
2. Cards fade out, centered connection status panel ("Connecting to Work PC...")
3. Success: launcher transforms into stream window (same wgpu surface, swap pipeline)
4. Failure: status panel shows error with retry/back buttons

**Single window, two modes.** Launcher and stream share the same winit window and wgpu device. No new window on connect — transition the existing one. Window resizes to match server display resolution on connect.

### Launcher Optimizations

- **Lazy GPU init** — start device creation on background thread at launch. Parse CLI/config on main thread simultaneously.
- **Pre-compiled shader cache** — ship pipeline-compatible bytecode. First launch compiles to `~/.prism/shader_cache/`. Subsequent launches skip compilation.
- **Deferred card rendering** — render quick connect bar first frame, cards on second frame. Responsive window in <50ms.
- **Speculative DNS + QUIC pre-connect** — on card hover, start DNS resolution and QUIC handshake in background. Discard if no click within 5s.
- **Parallel Noise + capability negotiation** — open both streams after QUIC connects, run concurrently. Saves one full RTT. Server identifies streams by first message type.
- **Connection pooling** — keep warm QUIC endpoint for 30s after disconnect. Reconnect to same server reuses endpoint.
- **Frame skip when idle** — no input events and no animations = don't render. Drop to 0 FPS. Wake on mouse/keyboard/status change.
- **Glyph cache warming** — pre-rasterize ASCII + common UI glyphs at used font sizes on first launch.
- **Async server pings** — fire UDP probes to all saved servers concurrently on launcher open. Non-blocking, 2s timeout.
- **Pipeline pre-creation** — while showing "Connecting...", create stream render pipelines in parallel with QUIC handshake.
- **Zero-frame transition** — first decoded frame uploaded and rendered in same vsync as connection success. No black frame gap. Launcher panels fade out over first stream frame.
- **Window resize race avoidance** — issue resize to stream resolution during "Connecting..." before first frame render.
- **Card grid layout caching** — recompute only on window resize or card add/remove.
- **Static atlas baking** — idle card grid baked to single texture. One quad for the entire grid.

---

## 5. In-Session Overlay

### Trigger

- **Double-tap Left Ctrl** — two key-down events within 300ms, no other keys between
- Intercepted locally, never forwarded to remote
- Single Ctrl presses forward normally (no delay/buffering)
- Detector is a 3-state machine: `Idle` → `FirstTapSeen(Instant)` → `Consumed`. Zero allocations, one timestamp comparison per key event.

### Animation

**Open (200ms total):**
1. 0ms: capture stream texture for blur source
2. 0-150ms: dim overlay fades in (black 30% opacity, ease-out)
3. 0-150ms: blur ramps 0 → full (progressive: 1/8 → 1/4 resolution)
4. 50-200ms: stats bar slides down from top (Y translate + fade, ease-out)

**Close (150ms total — faster close feels snappy):**
1. All panels scale to 95% + fade out simultaneously (150ms, ease-in)
2. Blur ramps down, dim fades (150ms)

**Sub-panel open:** Panel emerges from its trigger point in the stats bar. Scale 0.9 → 1.02 → 1.0 over 200ms (spring curve with elastic overshoot).

**Sub-panel close:** Shrinks back toward stats bar and fades.

### Stats Bar

Top-docked, full width, ~36px tall frosted glass bar.

```
┌──────────────────────────────────────────────────────────────────┐
│  FPS: 60  │  Latency: 4.2ms  │  H.265  │  1920×1080  │  12.4 Mbps  │  Gaming ▾  │  📌  │  ×  │
└──────────────────────────────────────────────────────────────────┘
```

- Left: performance metrics (from probe RTT + frame counter)
- Right: active profile (clickable dropdown), pin toggle, close
- Values update at 1Hz (not every frame)
- Color-coded thresholds: green = good, yellow = degraded (FPS <30), red = problem (latency >20ms)
- Metric separators: embossed glass ridges (1px light + 1px dark side by side)
- Active profile chip has filled glass background with accent color
- Warning/critical state: smooth color transition + single warm/red glow pulse behind metric
- Numeric values use monospace font to prevent jitter

**Pinned mode:** When pinned and overlay closed, stats bar becomes ultra-minimal — 5% glass fill, 60% text opacity, fades to full on mouse hover near top edge. ~24px tall.

### Sub-Panels

Independent floating glass panels. Opened by clicking stats bar metrics. Multiple can be open simultaneously. Draggable within window bounds.

**Panel 1 — Performance** (click FPS or Latency)

```
┌─────────────────────────────┐
│  Performance          [pin] │
│─────────────────────────────│
│  Framerate     60 fps       │
│  ██████████████████████ ▁▃▅ │  ← sparkline (last 60 samples)
│                             │
│  Latency       4.2 ms       │
│  ▃▅▃▂▁▃▅▃▂▁▃▅▃▂▁▃▅▃▂▁ ▁▃▅ │
│                             │
│  Decode time   0.8 ms       │
│  Bandwidth     12.4 Mbps    │
│  Frame gaps    0             │
│  IDR requests  2             │
└─────────────────────────────┘
```

- Sparklines: ring buffer polyline with trailing glow, accent-colored current-value dot with subtle pulse
- Pin icon: keeps panel visible after overlay closes (semi-transparent, non-interactive passthrough)

**Panel 2 — Quality** (click codec or resolution)

```
┌─────────────────────────────┐
│  Quality                    │
│─────────────────────────────│
│  Profile    [Gaming ▾]      │
│  Encoder    [Ultra Low ▾]   │
│  Max FPS    [120 ▾]         │
│  Lossless   [ ] Text mode   │
│  Region det [ ] Damage only │
│                             │
│  Bandwidth limit            │
│  ──●──────────── 100 Mbps   │
└─────────────────────────────┘
```

- Profile dropdown → `PROFILE_SWITCH` control message
- Individual settings → `QUALITY_UPDATE` / `CAPABILITY_UPDATE`
- Slider → `REDUCE_SEND_RATE`
- Changes apply instantly (no save/apply button)

**Panel 3 — Connection** (click encryption or server name)

```
┌─────────────────────────────┐
│  Connection                 │
│─────────────────────────────│
│  Server     Work PC         │
│  Address    10.0.0.5:7000   │
│  Encrypted  Noise IK ✓      │
│  Session    1h 23m          │
│  Client ID  a3f1...beef     │
│                             │
│  [Disconnect]  [Switch ▾]   │
└─────────────────────────────┘
```

- Disconnect returns to launcher (zero-frame transition) or exits process (CLI bypass mode)
- Switch dropdown shows saved servers for quick-switching
- Session duration ticks live

**Panel 4 — Display** (click resolution)

```
┌─────────────────────────────┐
│  Display                    │
│─────────────────────────────│
│  Monitor    [0: Primary ▾]  │
│  Resolution 1920×1080       │
│  Refresh    60 Hz           │
│                             │
│  ┌─────┐ ┌───┐             │
│  │  0  │ │ 1 │  monitor map│
│  └─────┘ └───┘             │
└─────────────────────────────┘
```

- Monitor selector sends to server (uses `MONITOR_LAYOUT` data)
- Visual monitor arrangement diagram, clickable

### Micro-Animations

- Stats bar values interpolate (don't jump) — FPS 60→58 counts down over 200ms
- Dropdown menus: staggered item reveal, each option 30ms after previous (cascade)
- Slider thumb: soft accent glow intensifies on drag
- Checkbox: radial wipe fill from center on toggle
- Status dots: breathing pulse (opacity 0.6→1.0, 2s sine cycle)

### Input Handling

- When overlay visible: all input goes to overlay, NOT forwarded to remote
- Exception: double-tap Left Ctrl closes overlay and resumes forwarding
- Mouse cursor switches from hidden (stream) to visible (overlay)

---

## 6. Widget System

### Widget Set

| Widget | Behavior |
|--------|----------|
| `Label` | Static or live-updating text, color-coded, monospace numeric option |
| `Sparkline` | Ring buffer polyline with trailing glow, accent current-value dot |
| `Dropdown` | Staggered cascade reveal, glass-styled popup |
| `Slider` | Accent fill, glow thumb, drag handling, value label |
| `Checkbox` | Radial wipe toggle animation |
| `Button` | Glass surface, hover glow, click callback |
| `TextInput` | Cursor, selection, autocomplete support (quick connect) |
| `Separator` | Embossed glass ridge (1px light + 1px dark) |
| `MonitorMap` | Custom monitor arrangement diagram from MONITOR_LAYOUT |

### Widget Trait

```rust
pub trait Widget {
    fn layout(&mut self, available: Rect) -> Size;
    fn paint(&self, ctx: &mut PaintContext);  // emits quads + text runs to batched draw list
    fn handle_event(&mut self, event: &UiEvent) -> EventResponse;
    fn animate(&mut self, dt: f32);  // per-frame animation tick
}
```

- `PaintContext` collects draw commands into batched list
- One render pass processes entire draw list
- `EventResponse`: `Consumed`, `Ignored`, or `Action(UiAction)` — actions bubble to state machine

### Layout System

- `Rect`-based absolute positioning for panels (draggable)
- Inside panels: vertical stack with padding, widgets get full width, report height
- Card grid: flow layout, fixed card width, wrap to next row, centered
- No CSS, no constraint solver

### Draw Batching

```
Per frame, UI produces:
  1. Quad batch: Vec<GlassQuad>   — position, size, blur rect, tint, border, radius, noise
  2. Text batch: Vec<TextRun>     — position, string, size, color, weight, monospace flag
  3. Glow batch: Vec<GlowRect>    — position, size, color, spread, intensity

All three uploaded as instance buffers, drawn in 3 draw calls total.
```

### Widget Optimizations

- **Instanced rendering** — 50 panels with 200 widgets = still 3 draw calls
- **Dirty rect tracking** — only re-upload instance data for changed widgets
- **Static panel atlas baking** — when animations settled, rasterize entire panel to single texture
- **Arena allocator** — all per-frame draw commands from 64KB bump arena, resets each frame, zero heap allocations steady-state
- **Widget pools** — flat `Vec<Widget>` per type (all labels together, all sparklines together). Cache-friendly iteration.
- **SOA sparkline storage** — all sparkline values in single contiguous `Vec<f32>` with per-sparkline offset/length
- **Glyph atlas packing** — shelf-packing on 1024x1024 texture. Pre-warm ASCII at startup.
- **Text hash caching** — hash (string, size, weight) per TextRun. If unchanged, skip layout. 59/60 frames skip text layout for 1Hz-updating values.
- **Subpixel positioning disabled** — whole-pixel rounding, eliminates subpixel coverage computation, cuts glyph variants 3x
- **Batched animation tick** — all Animation structs in contiguous Vec, SIMD-friendly layout. Dormant animations skipped via bitmask.
- **Frame skip on no-change** — empty dirty rects + all animations dormant + no input = skip entire UI render pass
- **Event coalescing** — mouse moves within same frame collapse to last position, scroll events accumulate delta
- **Spatial hash hit testing** — 64x64 grid, O(1) lookup per mouse event. Rebuild only on panel drag/resize.
- **Autocomplete trie** — saved server names/addresses in compact trie, <1μs prefix lookup

### Animation System

```rust
pub struct Animation {
    pub value: f32,          // current 0.0→1.0
    pub target: f32,         // target
    pub curve: EaseCurve,    // EaseOut, EaseIn, Spring, Linear
    pub duration_ms: f32,
    pub elapsed_ms: f32,
}
```

- Spring curve for panel open (overshoot)
- Ease-out for fade/blur
- Linear for numeric value interpolation
- Dormant animations marked via bitmask, tick returns <1μs when all settled

---

## 7. Data Flow & Protocol Integration

### Architecture

```
┌─────────────────────────────────────┐
│  UI Thread (winit + wgpu)           │
│  ┌───────────┐  ┌────────────────┐  │
│  │ Launcher   │  │ Overlay        │  │
│  └─────┬─────┘  └───────┬────────┘  │
│        └────────┬────────┘           │
│           ┌─────▼─────┐             │
│           │SessionBridge│            │
│           └─────┬─────┘             │
└─────────────────┼───────────────────┘
                  │ channels
┌─────────────────┼───────────────────┐
│  Async Runtime (tokio)              │
│           ┌─────▼─────┐            │
│           │NetworkAgent│            │
│           │ Frame Recv │            │
│           │ Input Send │            │
│           │ Control    │            │
│           │ Heartbeat  │            │
│           └────────────┘            │
└─────────────────────────────────────┘
```

### SessionBridge Channels

| Channel | Direction | Type | Purpose |
|---------|-----------|------|---------|
| `frame_rx` | Network → UI | `mpsc::Receiver<Frame>` | Decoded video frames |
| `input_tx` | UI → Network | `mpsc::Sender<InputDatagram>` | Mouse/keyboard events |
| `stats_rx` | Network → UI | `watch::Receiver<SessionStats>` | Live stats (1Hz) |
| `control_tx` | UI → Network | `mpsc::Sender<ControlCommand>` | Profile switch, quality, etc. |
| `connection_tx` | UI → Network | `oneshot::Sender<ConnectRequest>` | Initiate connection |
| `connection_rx` | Network → UI | `oneshot::Receiver<ConnectResult>` | Connection result |
| `server_info_rx` | Network → UI | `watch::Receiver<ServerInfo>` | Server metadata |

### SessionStats

```rust
pub struct SessionStats {
    pub fps: f32,
    pub latency_ms: f32,
    pub decode_time_ms: f32,
    pub bandwidth_bps: u64,
    pub frame_gaps: u32,
    pub idr_requests: u32,
    pub codec: String,
    pub resolution: (u32, u32),
    pub active_profile: String,
    pub encryption: EncryptionStatus,
    pub session_duration: Duration,
    pub sparkline_fps: [f32; 60],
    pub sparkline_latency: [f32; 60],
}
```

Uses `watch` channel — UI reads latest snapshot only, no queue buildup.

### ControlCommand

```rust
pub enum ControlCommand {
    SwitchProfile(String),
    UpdateQuality {
        encoder_preset: Option<EncoderPreset>,
        max_fps: Option<u8>,
        lossless_text: Option<bool>,
        region_detection: Option<bool>,
    },
    SetBandwidthLimit(u64),
    SelectMonitor(u8),
    Disconnect,
    RequestServerInfo,
}
```

Each variant maps 1:1 to an existing protocol control message.

### ConnectRequest / ConnectResult

```rust
pub struct ConnectRequest {
    pub server_addr: SocketAddr,
    pub noise_key: Option<[u8; 32]>,
    pub identity_path: PathBuf,
    pub preferred_profile: String,
}

pub enum ConnectResult {
    Connected {
        server_info: ServerInfo,
        frame_rx: mpsc::Receiver<Frame>,
        stats_rx: watch::Receiver<SessionStats>,
        control_tx: mpsc::Sender<ControlCommand>,
        server_info_rx: watch::Receiver<ServerInfo>,
        input_tx: mpsc::Sender<InputDatagram>,
    },
    Failed {
        error: String,
        retryable: bool,
    },
}
```

### Server Pinger

```rust
pub struct ServerPinger {
    pub statuses: HashMap<SocketAddr, watch::Receiver<ServerStatus>>,
}

pub enum ServerStatus {
    Unknown,
    Online(Duration),  // RTT
    Offline,           // 2s timeout
}
```

Fires on launcher open, uses same QUIC endpoint as connections.

### Speculative Pre-connect

```rust
pub enum PreconnectState {
    Idle,
    DnsResolving(SocketAddr),
    QuicHandshaking(quinn::Connecting),
    Ready(quinn::Connection),  // held up to 5s
    Expired,
}
```

Triggered on card hover. If Ready on click, skip to Noise + capability negotiation.

### Parallel Handshake

Server identifies streams by first message type: Noise handshake header vs JSON length prefix. Both streams opened concurrently after QUIC connect. Saves one RTT.

**Server-side dependency:** This requires a change to the server's stream acceptance logic in `prism-server/src/negotiation_handler.rs` — the server must accept and route multiple concurrent bi-streams by inspecting the first bytes, rather than assuming a fixed stream order. This is a protocol-level change that must be implemented alongside the client work.

---

## 8. UI State Machine

```
            ┌──────────┐
    start──▶│ Launcher │◀──── disconnect (if LaunchMode::Launcher)
            └────┬─────┘
                 │ connect
                 ▼
           ┌────────────┐
           │ Connecting  │──── fail ──▶ Launcher
           └─────┬──────┘
                 │ first frame received
                 ▼
            ┌─────────┐  double-tap LCtrl  ┌─────────┐
            │ Stream   │◀────────────────▶│ Overlay  │
            └─────────┘                    └─────────┘
```

- Launcher and Stream are full render modes (swap pipelines)
- Connecting is transitional (status panel over dimmed cards)
- Overlay composites on top of Stream (additive layer)
- CLI bypass enters Connecting directly

---

## 9. Persistence & Config

### Directory Structure

```
~/.prism/
├── client_identity.json    # X25519 keypair (existing, unchanged)
├── servers.json            # Compacted saved servers
├── servers.log             # Append-only mutation log
└── shader_cache/           # Pre-compiled pipeline bytecode
    └── <adapter>-<hash>.bin
```

### SavedServer

```rust
pub struct SavedServer {
    pub id: Uuid,
    pub display_name: String,
    pub address: String,               // host:port string (allows hostnames)
    pub noise_public_key: Option<String>,
    pub default_profile: String,
    pub accent_color: [u8; 3],         // RGB
    pub last_connected: Option<u64>,   // Unix timestamp
    pub last_resolution: Option<(u32, u32)>,
    pub last_codec: Option<String>,
    pub created_at: u64,
}
```

### Persistence Strategy

- **Append-only log** (`servers.log`): each mutation is a JSON line (`{"op":"add","server":{...}}`, `{"op":"update",...}`, `{"op":"delete",...}`)
- **Compaction** to `servers.json` on launcher startup or when log exceeds 100 entries
- **Crash safety**: partial last log line skipped on read; `servers.json` is always valid fallback
- Log write is O(1) append

### Shader Cache

- Keyed by pipeline hash + GPU adapter name
- Invalidated when binary version changes (compile-time hash)

---

## 10. CLI Integration

### Usage

```
prism-client                                    → Launcher mode
prism-client 192.168.1.100:7000                 → Direct connect (CLI bypass)
prism-client 192.168.1.100:7000 --noise abc...  → Direct connect with encryption
prism-client --help                             → Help text
prism-client --version                          → Version
prism-client --init                             → Generate default servers.json
prism-client --config PATH                      → Custom config path
```

### Launch Modes

```rust
pub enum LaunchMode {
    Launcher,       // return to launcher on disconnect
    DirectConnect,  // exit process on disconnect
}
```

| Invocation | Window opens as | Overlay available | On disconnect |
|------------|----------------|-------------------|---------------|
| No args | Launcher | Yes (after connect) | Returns to Launcher |
| With address | Stream | Yes | Exit process |

### Backward Compatibility

Existing `prism-client HOST:PORT` invocations behave identically except: window uses wgpu/winit instead of minifb, and overlay is available via double-tap Left Ctrl. No behavioral regressions for scripts or automation.

---

## 11. Crate Structure

```
prism-client/
├── src/
│   ├── main.rs
│   ├── client_app.rs
│   ├── renderer/
│   │   ├── mod.rs               # PrismRenderer
│   │   ├── stream_texture.rs    # Ring-buffered YUV upload + compute shader
│   │   ├── blur_pipeline.rs     # Two-pass Gaussian blur
│   │   ├── glass_panel.rs       # Frosted glass compositing
│   │   ├── text_renderer.rs     # glyphon wrapper + glyph cache
│   │   ├── shader_cache.rs      # Pipeline cache persistence
│   │   └── animation.rs         # Spring, ease-out, interpolation
│   ├── ui/
│   │   ├── mod.rs               # UI state machine
│   │   ├── widgets/
│   │   │   ├── mod.rs           # Widget trait, layout primitives
│   │   │   ├── label.rs
│   │   │   ├── sparkline.rs
│   │   │   ├── dropdown.rs
│   │   │   ├── slider.rs
│   │   │   ├── checkbox.rs
│   │   │   ├── button.rs
│   │   │   ├── text_input.rs
│   │   │   ├── separator.rs
│   │   │   └── monitor_map.rs
│   │   ├── launcher/
│   │   │   ├── mod.rs
│   │   │   ├── quick_connect.rs
│   │   │   ├── server_card.rs
│   │   │   ├── card_grid.rs
│   │   │   ├── server_form.rs
│   │   │   └── settings.rs
│   │   └── overlay/
│   │       ├── mod.rs
│   │       ├── stats_bar.rs
│   │       ├── perf_panel.rs
│   │       ├── quality_panel.rs
│   │       ├── conn_panel.rs
│   │       └── display_panel.rs
│   ├── input/
│   │   ├── mod.rs               # Input router
│   │   ├── double_tap.rs        # Double-tap detector
│   │   └── drag.rs              # Panel drag handler
│   └── config/
│       ├── mod.rs               # Unified config
│       └── servers.rs           # SavedServer, append-log persistence
```

---

## 12. Future Enhancements (Out of Scope)

- **Live server thumbnails** on launcher cards (requires new protocol channel)
- **User-configurable accent color** per theme
- **Audio controls** in overlay (when audio channel is implemented)
- **Clipboard history panel** in overlay
- **Screenshot/recording** tools
- **Gamepad input mapping** UI
- **mDNS/DNS-SD server discovery** (auto-populate launcher)
- **Multi-session tabs** (connect to multiple servers)
