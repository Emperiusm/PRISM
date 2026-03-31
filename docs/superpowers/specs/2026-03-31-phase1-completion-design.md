# PRISM Phase 1 Completion — Design Spec

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-31                     |
| Authors     | Ehsan + Claude                 |
| Parent spec | 2026-03-30-prism-architecture-design.md |

This document covers the remaining Phase 1 features needed to make PRISM a **usable remote desktop**: input forwarding, control channel handler, quality feedback loop, clipboard sync, audio streaming, and manual pairing. These features wire into the existing 10-crate architecture without creating new crates.

---

## 1. Input Forwarding (Client → Server)

### 1.1 Client-Side Input Capture

The client captures keyboard and mouse events from the minifb window and sends them as datagrams to the server on `CHANNEL_INPUT` (0x002).

```rust
/// Input event types sent from client to server.
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    KeyDown { scancode: u16, vk: u16 },
    KeyUp { scancode: u16, vk: u16 },
    MouseMove { x: u16, y: u16 },         // normalized 0-65535
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    MouseScroll { delta_x: i16, delta_y: i16 },
}

#[derive(Debug, Clone, Copy)]
pub enum MouseButton { Left, Right, Middle, X1, X2 }
```

Wire format: PRISM header (16B) + InputEvent binary (8B fixed). Total: 24 bytes per event. Sent as datagrams (latency-critical, loss-tolerant).

Mouse coordinates are normalized to 0–65535 range (resolution-independent). Server maps to actual screen coordinates using the captured display's resolution.

### 1.2 Server-Side Input Injection (Windows)

Server receives input datagrams, decodes `InputEvent`, and calls Win32 `SendInput()` to inject keyboard/mouse events into the desktop.

```rust
/// InputInjector is #[cfg(windows)] — uses Win32 SendInput API.
pub struct InputInjector {
    screen_width: u32,
    screen_height: u32,
}

impl InputInjector {
    pub fn inject(&self, event: InputEvent) -> Result<(), InputError>;
}
```

Win32 `SendInput` accepts `INPUT` structs with `KEYBDINPUT` or `MOUSEINPUT`. Mouse coordinates use absolute positioning (0–65535 range matches `MOUSEEVENTF_ABSOLUTE`).

### 1.3 Input Channel Handler

Implements `ChannelHandler` for `CHANNEL_INPUT`. Registered with `ChannelDispatcher` at server startup. Receives datagrams from the recv loop, parses `InputEvent`, calls `InputInjector::inject()`.

### 1.4 Debouncing

Input events are sent immediately (no coalescing) — latency is critical. The existing `InputTriggerCoalescer` (8ms debounce) applies only to input-triggered capture, not to input forwarding itself.

Mouse move events may be rate-limited to 125Hz (one per 8ms) to avoid flooding the network. At 24 bytes per event, 125Hz = 3 KB/sec — negligible.

---

## 2. Control Channel Handler

### 2.1 Heartbeat Exchange

Both sides send `HEARTBEAT` datagrams every 5 seconds. Receiver resets the heartbeat timer on any packet (not just heartbeat messages). If no packet arrives for 10 seconds → suspend. 60 seconds → tombstone.

Wire format: PRISM header with `channel_id = CHANNEL_CONTROL`, `msg_type = HEARTBEAT` (0x01). Empty payload (16 bytes total — just the header).

The server's `HeartbeatMonitor` is already implemented. What's missing: a background task that sends heartbeat datagrams and a ControlChannelHandler that processes incoming heartbeats.

### 2.2 Control Channel Handler

Implements `ChannelHandler` for `CHANNEL_CONTROL` (0x006). Routes control messages by `msg_type`:

| msg_type | Action |
|---|---|
| HEARTBEAT (0x01) | Reset heartbeat timer (already happens via activity signal) |
| HEARTBEAT_ACK (0x02) | Log RTT |
| PROBE_REQUEST (0x05) | Echo back as PROBE_RESPONSE with timestamp |
| PROBE_RESPONSE (0x06) | Forward to QualityMonitor prober |
| CLIENT_FEEDBACK (0x07) | Parse JSON, feed to QualityMonitor |
| CLIENT_ALERT (0x08) | Log alert, adjust quality immediately |

### 2.3 Heartbeat Background Task

Server spawns a task per client that sends HEARTBEAT datagrams every 5 seconds via `connection.try_send_datagram()`. Client does the same. Both reset their timers on any received packet.

---

## 3. Quality Feedback Loop

### 3.1 Data Flow

```
Transport metrics (RTT, loss, bandwidth)
        ↓
ConnectionProber sends probe datagrams (2s interval during streaming)
        ↓
Probe echoes arrive via Control channel → ProbeResult (measured RTT)
        ↓
BandwidthEstimator + TrendDetector update
        ↓
ConnectionQuality::compute() → score + QualityRecommendation
        ↓
DegradationLadder::target_level() → new level
        ↓
Hysteresis check (2s downgrade, 10s upgrade)
        ↓
If level changed: adjust encoder bitrate + resolution
```

### 3.2 Probe Task

Server spawns a per-client task that:
1. Calls `ConnectionProber::generate_probe()` to get probe payloads
2. Wraps in PRISM header (CHANNEL_CONTROL, PROBE_REQUEST)
3. Sends as datagram
4. When PROBE_RESPONSE arrives (via ControlChannelHandler), calls `prober.process_echo()` → gets RTT

### 3.3 Quality Evaluation Task

Server spawns a periodic task (every 500ms or on quality change):
1. Read transport metrics from QuicConnection
2. Feed to `QualityMonitor::update(metrics)`
3. If `level_changed`: apply new encoder bitrate via `HwEncoder` reconfig
4. Send QUALITY_UPDATE to client via Control stream

### 3.4 Client Feedback

Client tracks its own performance (decode time, frame drops, render time) and sends `ClientFeedback` every 1 second (normal) or 200ms (stressed) via the Control stream using `FramedWriter`.

Server's ControlChannelHandler deserializes and feeds to QualityMonitor for decision-making.

---

## 4. Clipboard Sync

### 4.1 Architecture

Bidirectional clipboard sync on `CHANNEL_CLIPBOARD` (0x004). When the user copies on either side, the clipboard content is sent to the other side.

```rust
pub struct ClipboardMessage {
    pub format: ClipboardFormat,
    pub data: Vec<u8>,
    pub source_device_id: Uuid,
    pub sequence: u32,
}

pub enum ClipboardFormat {
    Text,
    Html,
    Image,   // PNG encoded
    Files,   // JSON list of filenames (metadata only, not content)
}
```

### 4.2 Platform Clipboard Access

- **Windows:** `OpenClipboard` / `GetClipboardData` / `SetClipboardData` via the `windows` crate. Use `AddClipboardFormatListener` to detect changes.
- **Client (cross-platform):** `arboard` crate (cross-platform clipboard access).

### 4.3 Echo Suppression

When we set the clipboard ourselves (from a remote copy), we must not echo it back. Use a `last_set_sequence` counter — if the clipboard change matches what we just set, suppress it.

### 4.4 Clipboard Channel Handler

Implements `ChannelHandler` for `CHANNEL_CLIPBOARD`. Messages sent via FramedWriter/FramedReader on a bidirectional stream (clipboard content can be large — images may be hundreds of KB).

### 4.5 Size Limits

Text: 1MB max. Images: 10MB max. Files: metadata only (actual file transfer uses FileShare channel). Content exceeding limits is silently truncated with a warning.

---

## 5. Audio Streaming

### 5.1 Architecture

Server captures system audio → encodes with Opus → sends as datagrams on `CHANNEL_AUDIO` (0x003). Client decodes Opus → plays via audio output.

```
Server: WASAPI loopback capture → Opus encode → PRISM datagram
Client: PRISM datagram → Opus decode → cpal audio output
```

### 5.2 Audio Capture (Windows)

WASAPI loopback capture (`IAudioClient` in shared mode with `AUDCLNT_STREAMFLAGS_LOOPBACK`). Captures all system audio without affecting playback.

```rust
pub struct WasapiCapture {
    // #[cfg(windows)] — WASAPI COM interfaces
}

pub trait AudioCapture: Send + 'static {
    fn start(&mut self) -> Result<(), AudioError>;
    fn stop(&mut self);
    fn read_samples(&mut self, buf: &mut [f32]) -> Result<usize, AudioError>;
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
}
```

### 5.3 Opus Encoding/Decoding

Use the `opus` crate (or `audiopus`). Encode at 48kHz stereo, 128kbps. Each Opus frame = 20ms of audio = 960 samples × 2 channels = ~200 bytes encoded.

Wire format: PRISM header (16B) + Opus frame (~200B). Sent as datagrams (latency-critical). At 50 packets/sec × 216B = 10.8 KB/sec — negligible bandwidth.

### 5.4 Client Playback

Use the `cpal` crate for cross-platform audio output. Decode Opus frames, write to audio output buffer. Buffer 3 frames (60ms) for jitter absorption.

### 5.5 Audio Channel Handler

Server-side: implements `ChannelHandler` for `CHANNEL_AUDIO`. But audio flows server→client (not client→server), so the handler only processes client-originated messages (volume control, mute toggle). The main audio flow is a server-side task that captures and sends.

---

## 6. Manual Pairing Flow

### 6.1 Flow

1. Server generates identity at startup, prints public key as hex
2. User copies the 64-char hex string to the client machine
3. Client passes it as `--server-key <hex>` CLI argument
4. Client generates its own identity, starts Noise IK handshake with the server's key
5. Server receives handshake, extracts client's public key
6. If key is unknown: server prompts "New device 'Client-Name' wants to connect. Allow? [y/n]"
7. If approved: server adds to PairingStore, session proceeds
8. If denied: server drops connection

### 6.2 Implementation

The existing `--noise` flag already does steps 1-5. What's missing:
- Server-side prompt for unknown devices (interactive stdin)
- PairingStore persistence (save/load paired devices)
- Auto-approve option (`--auto-pair`) for development

### 6.3 Persistent Pairing

Paired devices are saved to `prism_pairing.json`. On next connection, the device is recognized by its public key and auto-authenticated without prompting.

```rust
// At startup:
let pairing_store = PairingStore::load_or_create(&config.pairing_path)?;
let gate = DefaultSecurityGate::new(pairing_store, identity, audit_log);

// On unknown device:
// Interactive: prompt user
// --auto-pair: auto-approve
```

---

## 7. Phase Mapping

| Component | This Plan | Phase 2+ |
|---|---|---|
| Input forwarding | Full: keyboard + mouse + scroll | Touch, pen, gamepad |
| Control channel | Heartbeat, probe, feedback, alerts | Profile switching, overlay toggle |
| Quality loop | Probes → metrics → degradation → encoder | Content-aware adaptation |
| Clipboard | Text + image sync | File drag-and-drop |
| Audio | WASAPI + Opus + cpal | Multi-channel, spatial |
| Pairing | Manual hex + interactive prompt | SPAKE2 short code, Tailscale auto |

---

## 8. File Layout

All new code goes into existing crates — no new crates needed.

```
crates/prism-server/src/
    input_handler.rs            # InputChannelHandler + InputInjector (#[cfg(windows)])
    control_handler.rs          # ControlChannelHandler (heartbeat, probe, feedback routing)
    heartbeat_task.rs           # Per-client heartbeat sender task
    probe_task.rs               # Per-client quality probe sender task
    quality_task.rs             # Periodic quality evaluation + encoder adjustment
    clipboard_handler.rs        # ClipboardChannelHandler + platform clipboard access
    audio_capture.rs            # WasapiCapture (#[cfg(windows)]) + AudioCapture trait
    audio_sender.rs             # Audio capture → Opus encode → datagram send task
    main.rs                     # Wire all handlers + tasks into accept loop

crates/prism-client/src/
    input_sender.rs             # Capture minifb keyboard/mouse → send input datagrams
    control_client.rs           # Client heartbeat + feedback sender
    clipboard_client.rs         # Client-side clipboard sync (arboard crate)
    audio_player.rs             # Opus decode → cpal playback
    main.rs                     # Wire input capture + audio + clipboard into client loop

crates/prism-protocol/src/
    input.rs                    # InputEvent, MouseButton wire types

crates/prism-display/src/
    (no changes — display types already complete)
```

### New Dependencies

| Crate | Where | Purpose |
|---|---|---|
| `opus` or `audiopus` | prism-server, prism-client | Opus encode/decode |
| `cpal` | prism-client | Audio output |
| `arboard` | prism-client | Cross-platform clipboard |

Windows-only (already have `windows` crate):
- WASAPI audio capture uses existing `Win32_Media_Audio` features
- `SendInput` uses existing `Win32_UI_Input_KeyboardAndMouse` features

---

## 9. Testing Strategy

| Category | What | How |
|---|---|---|
| Unit: InputEvent | Serialize/deserialize roundtrip | Known bytes |
| Unit: InputInjector | Coordinate mapping (normalized → screen) | Math tests (no actual injection) |
| Unit: ControlHandler | Message routing by msg_type | Mock dispatcher |
| Unit: ClipboardMessage | JSON roundtrip, size limits | Known payloads |
| Unit: FrameStats | FPS, gap detection (existing) | Already tested |
| Integration: Input roundtrip | Client sends KeyDown → server receives | Loopback QUIC |
| Integration: Heartbeat | Both sides exchange heartbeats for 5s | Loopback, verify no timeout |
| Integration: Probe → quality | Send probes, verify RTT computed | Loopback with artificial delay |
| Integration: Clipboard | Set clipboard on server → verify client receives | Loopback + arboard |
| Integration: Audio | Capture silence → encode → send → decode → verify samples | Loopback + synthetic audio |

---

*PRISM Phase 1 Completion Design v1.0 — CC0 Public Domain*
