# PRISM Phase 1 Completion — Design Spec (Revised)

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.1                            |
| Status      | DRAFT                          |
| Date        | 2026-03-31                     |
| Authors     | Ehsan + Claude                 |
| Parent spec | 2026-03-30-prism-architecture-design.md |

This document covers the remaining Phase 1 features needed to make PRISM a **usable remote desktop**: input forwarding with local cursor prediction, control channel handler with zero-allocation heartbeats, event-driven quality feedback loop, clipboard sync with hash-based echo suppression, audio streaming with silence detection, and SSH-style trust-on-first-use pairing. These features wire into the existing 10-crate architecture without creating new crates.

---

## 1. Input Forwarding (Client → Server)

### 1.1 Input Event Types

```rust
/// Input event types sent from client to server.
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    // Keyboard
    KeyDown { scancode: u16, vk: u16 },
    KeyUp { scancode: u16, vk: u16 },
    TextInput { codepoint: u32 },                       // IME/Unicode/emoji

    // Mouse — absolute mode (desktop use)
    MouseMove { x: u16, y: u16 },                       // normalized 0-65535
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    MouseScroll { delta_x: i16, delta_y: i16 },

    // Mouse — relative mode (games/FPS)
    MouseMoveRelative { dx: i16, dy: i16 },             // raw delta

    // Mode switching
    SetMouseMode { relative: bool },                    // client requests mode toggle
}

#[derive(Debug, Clone, Copy)]
pub enum MouseButton { Left, Right, Middle, X1, X2 }
```

Wire format: PRISM header (16B) + InputEvent binary (12B fixed, padded). Total: 28 bytes per event. Sent as datagrams on `CHANNEL_INPUT` (0x002).

**Pre-built header template (P1 optimization):** The PRISM header for input events only changes in sequence and timestamp. Pre-build the 16-byte header at session start, patch 8 bytes per packet. Eliminates ~5 buffer operations per event.

### 1.2 Client-Side Local Cursor Rendering (R32)

The client renders the cursor locally at zero latency. Server sends cursor position corrections only when prediction diverges.

```
Client event loop:
  1. User moves mouse → update local cursor position immediately (0ms)
  2. Send MouseMove datagram to server
  3. Server processes input → DDA captures new frame → sends back
  4. Frame arrives with cursor_x/cursor_y in SlicePayloadHeader
  5. Client compares server cursor position to local prediction
  6. If divergent by >5px: snap to server position (correction)
  7. If consistent: local prediction was right (no visible correction)
```

The minifb window hides the OS cursor (if supported) and renders a custom cursor sprite at the local predicted position. The existing `CursorShape` and `CursorPosition` types in prism-display handle this.

### 1.3 Server-Side Input Injection (Windows)

```rust
/// #[cfg(windows)] — Uses Win32 SendInput API.
pub struct InputInjector {
    screen_width: u32,
    screen_height: u32,
    relative_mode: bool,
}

impl InputInjector {
    pub fn inject(&self, event: InputEvent) -> Result<(), InputError>;
}
```

Mouse coordinates: absolute mode uses `MOUSEEVENTF_ABSOLUTE` (0–65535 matches the normalized range directly). Relative mode uses `MOUSEEVENTF_MOVE` with raw deltas.

`TextInput`: uses `SendInput` with `KEYEVENTF_UNICODE` flag and the codepoint as `wScan`. Handles CJK, emoji, and composed characters.

### 1.4 Input-Triggered Capture (R32)

When the server receives ANY input event, it immediately triggers a DDA capture (bypasses the frame pacer interval). This cuts perceived input latency by up to 16ms (one frame interval).

```rust
// In InputChannelHandler::handle_datagram():
self.injector.inject(event)?;
self.capture_trigger.send(()).ok();  // signal DDA to capture NOW
```

The existing `InputTriggerCoalescer` (8ms debounce) prevents excessive captures from rapid input.

### 1.5 Input Batching

Mouse move events within a 1ms window are coalesced using the existing `DatagramCoalescer`. At 125Hz mouse polling, this batches 1-2 moves per datagram, halving syscall overhead. Keyboard events are never coalesced (each keypress matters).

---

## 2. Control Channel Handler

### 2.1 Zero-Allocation Heartbeat (S11)

The heartbeat packet is always the same 16 bytes. Pre-build once at session start:

```rust
pub struct HeartbeatGenerator {
    packet: Bytes,  // pre-built, immutable, clone is Arc increment
}

impl HeartbeatGenerator {
    pub fn new() -> Self {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id: CHANNEL_CONTROL,
            msg_type: control_msg::HEARTBEAT,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        Self { packet: buf.freeze() }
    }

    pub fn next(&self) -> Bytes { self.packet.clone() }  // Arc clone, zero alloc
}
```

`try_send_datagram(heartbeat_gen.next())` every 5 seconds. Zero heap allocation on the hot path.

### 2.2 Control Channel Handler

Implements `ChannelHandler` for `CHANNEL_CONTROL` (0x006). Routes by `msg_type`:

| msg_type | Action | Transport |
|---|---|---|
| HEARTBEAT (0x01) | Reset heartbeat timer (activity signal) | Datagram |
| HEARTBEAT_ACK (0x02) | Compute RTT from timestamp | Datagram |
| PROBE_REQUEST (0x05) | Echo back as PROBE_RESPONSE | Datagram |
| PROBE_RESPONSE (0x06) | Forward to QualityMonitor prober | Datagram |
| CLIENT_FEEDBACK (0x07) | Deserialize, feed to QualityMonitor | Stream (framed) |
| CLIENT_ALERT (0x08) | Log + adjust quality immediately | Datagram |
| SHUTDOWN_NOTICE (0x20) | Client shows message, prepares reconnect | Stream (framed) |

### 2.3 ChannelHandler Trait Refactor

The current `ChannelHandler` trait only has `handle_datagram`. Clipboard and client feedback need bidirectional streams. Add `handle_stream`:

```rust
#[async_trait]
pub trait ChannelHandler: Send + Sync {
    fn channel_id(&self) -> u16;
    async fn handle_datagram(&self, from: ClientId, data: Bytes) -> Result<(), ChannelError>;

    /// Handle a stream-delivered message. Default: no-op.
    async fn handle_stream(
        &self,
        _from: ClientId,
        _send: OwnedSendStream,
        _recv: OwnedRecvStream,
    ) -> Result<(), ChannelError> {
        Ok(()) // channels that don't use streams ignore this
    }
}
```

The recv loop's `accept_bi` path dispatches to `handler.handle_stream()`.

---

## 3. Quality Feedback Loop

### 3.1 Event-Driven Architecture

Quality evaluation is event-driven, NOT periodic polling. Triggers:
1. **Probe echo received** → recompute quality (primary trigger)
2. **Client feedback received** → recompute quality
3. **500ms fallback timer** → recompute if no events (catch stale state)

This saves CPU when quality is stable (typical case) while reacting within one probe interval (~2s) when quality changes.

### 3.2 Data Flow

```
ConnectionProber (2s/5s/30s/60s adaptive)
    │ sends PROBE_REQUEST datagram
    ▼
Peer echoes PROBE_RESPONSE
    │
    ▼
prober.process_echo() → ProbeResult { rtt }
    │
    ▼
BandwidthEstimator::record_send/recv()  ← also fed by transport metrics
TrendDetector::record(rtt)
OneWayDelayEstimator::record()
    │
    ▼
ConnectionQuality::compute(rtt, jitter, loss, bw_send, bw_recv, asymmetry)
    │ score + QualityRecommendation
    ▼
ArcSwap<ConnectionQuality> cache (S12)
    │ ~1ns reads by degradation ladder, overlay, frame sender
    ▼
DegradationLadder::target_level(&recommendation)
    │
    ▼
Hysteresis::should_change(current, target)
    │ 2s downgrade hold, 10s upgrade hold
    ▼
If level changed:
    ├─ HwEncoder: reconfigure bitrate (cheap, no reinit)
    ├─ HwEncoder: reconfigure resolution (expensive, reinit + IDR)
    └─ Send QUALITY_UPDATE to client via Control stream
```

### 3.3 ConnectionQuality ArcSwap Cache (S12)

Quality is computed infrequently (~every 2s from probes) but read frequently (~60x/sec by frame sender, overlay, degradation ladder). Cache in `ArcSwap`:

```rust
pub struct QualityCache {
    inner: ArcSwap<ConnectionQuality>,
}

impl QualityCache {
    /// Write: called by quality evaluation task (~0.5-2Hz)
    pub fn update(&self, quality: ConnectionQuality) {
        self.inner.store(Arc::new(quality));
    }

    /// Read: called by frame sender, overlay, etc. (~60Hz). Cost: ~1ns.
    pub fn load(&self) -> Arc<ConnectionQuality> {
        self.inner.load_full()
    }
}
```

### 3.4 Keyframe Hint Integration

Before encoding an IDR, the frame sender notifies the QualityMonitor. The arbiter temporarily boosts display allocation for 100ms by reducing lower-priority channels. This prevents congestion spikes from keyframe bursts.

### 3.5 Asymmetry Response

When `OneWayDelayEstimator` reports asymmetry:
- `DownstreamSlow`: reduce outgoing (server→client) bandwidth allocation proportionally
- `UpstreamSlow`: send `REDUCE_SEND_RATE` control message to client

### 3.6 Client Feedback

Client tracks performance locally and reports periodically:

```rust
pub struct ClientPerformanceTracker {
    decode_times: VecDeque<u64>,     // last 60 decode times (µs)
    render_times: VecDeque<u64>,     // last 60 render times (µs)
    frames_decoded: u64,
    frames_dropped: u64,
    decoder_queue_depth: u8,
    feedback_config: ClientFeedbackConfig,
}
```

**Tiered frequency:** 1s normal, 200ms when stressed (queue depth ≥ 3 or drop rate ≥ 5%). Sent as JSON on the Control bidirectional stream via `FramedWriter`.

---

## 4. Clipboard Sync

### 4.1 Architecture

Bidirectional clipboard sync on `CHANNEL_CLIPBOARD` (0x004). Uses a bidirectional QUIC stream (clipboard content can be large).

```rust
pub struct ClipboardMessage {
    pub format: ClipboardFormat,
    pub data: Vec<u8>,
    pub content_hash: u64,           // for echo suppression
    pub source_device_id: Uuid,
}

pub enum ClipboardFormat {
    Text,
    Html,
    Image,   // PNG encoded
    Files,   // JSON metadata only
}
```

### 4.2 Hash-Based Echo Suppression

When we set the clipboard from a remote copy, we hash the content and store it. When the OS notifies us of a clipboard change, we hash the new content — if it matches our last-set hash, suppress the echo.

```rust
pub struct ClipboardEchoGuard {
    last_set_hash: AtomicU64,
}

impl ClipboardEchoGuard {
    pub fn set_and_remember(&self, data: &[u8]) {
        self.last_set_hash.store(fast_hash(data), Ordering::Relaxed);
        // ... actually set clipboard
    }

    pub fn should_send(&self, data: &[u8]) -> bool {
        fast_hash(data) != self.last_set_hash.load(Ordering::Relaxed)
    }
}
```

This is more robust than sequence-number-based suppression (handles network reordering, race conditions).

### 4.3 Platform Clipboard

- **Server (Windows):** `windows` crate — `AddClipboardFormatListener` for change detection, `OpenClipboard`/`GetClipboardData`/`SetClipboardData` for access. Runs on a dedicated thread with a message pump (clipboard APIs require a window handle).
- **Client:** `arboard` crate — cross-platform clipboard. Poll every 250ms for changes (arboard doesn't support change notifications on all platforms).

### 4.4 Size Limits & Filtering

| Format | Max Size | Exceeds Limit |
|---|---|---|
| Text | 1 MB | Truncate with warning |
| HTML | 2 MB | Truncate with warning |
| Image | 10 MB (PNG) | Skip with warning |
| Files | Metadata only | File content via FileShare |

The `SecurityContext` has a `ContentFilter` trait slot for clipboard — Phase 1 uses no-op (AllowAll). Phase 3 adds URL sanitization, size limits per device, etc.

---

## 5. Audio Streaming

### 5.1 Architecture

```
Server: WASAPI loopback → silence detect → Opus encode → PRISM datagram
Client: PRISM datagram → adaptive jitter buffer → Opus decode → cpal output
```

### 5.2 Audio Capture (Windows)

WASAPI loopback capture (`IAudioClient` in shared mode with `AUDCLNT_STREAMFLAGS_LOOPBACK`). Captures all system audio.

```rust
pub trait AudioCapture: Send + 'static {
    fn start(&mut self) -> Result<(), AudioError>;
    fn stop(&mut self);
    fn read_samples(&mut self, buf: &mut [f32]) -> Result<usize, AudioError>;
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
}
```

### 5.3 Silence Detection

Before encoding, check if the audio buffer is silent (RMS below threshold). If silent for >100ms, stop sending packets. Resume on first non-silent frame. This saves ~10KB/sec during typical coding sessions.

```rust
pub struct SilenceDetector {
    threshold_rms: f32,         // 0.001 (-60dB)
    silent_frames: u32,
    silent_threshold: u32,      // 5 frames = 100ms at 20ms/frame
}

impl SilenceDetector {
    pub fn is_silent(&mut self, samples: &[f32]) -> bool {
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        if rms < self.threshold_rms {
            self.silent_frames += 1;
            self.silent_frames >= self.silent_threshold
        } else {
            self.silent_frames = 0;
            false
        }
    }
}
```

### 5.4 Opus Encoding

Use `audiopus` (bundles libopus, no system dependency). Encode at 48kHz stereo, 128kbps. 20ms frames = 960 samples × 2 channels.

Wire format:
```
PRISM header (16B):
    channel_id = CHANNEL_AUDIO (0x003)
    msg_type = 0x01 (audio frame)
    timestamp_us = capture timestamp    ← A/V sync anchor
    payload_length = opus_frame_len

Opus frame (~100-300B)
```

The `timestamp_us` field in the PRISM header provides A/V synchronization. The client correlates audio timestamps with display frame timestamps to maintain lip sync.

### 5.5 Client Playback — Adaptive Jitter Buffer

Fixed buffers add unnecessary latency on LAN and cause glitches on WAN. Use an adaptive jitter buffer:

```rust
pub struct AdaptiveJitterBuffer {
    buffer: VecDeque<AudioFrame>,
    target_depth_ms: u32,       // starts at 20ms
    min_depth_ms: u32,          // 20ms (LAN)
    max_depth_ms: u32,          // 80ms (WAN)
    jitter_estimator: f32,      // EMA of inter-arrival jitter
}
```

- **LAN (~1ms RTT):** buffer depth = 20ms (1 Opus frame). Near-zero latency.
- **WAN (~50ms RTT):** buffer depth grows to 40-60ms based on observed jitter.
- **Bad network:** caps at 80ms. Beyond that, quality loop should reduce audio bitrate or skip.

Audio output via `cpal` — write decoded PCM to the default output device.

### 5.6 Audio Channel Handler

Server-side handler for `CHANNEL_AUDIO` processes client→server control messages:
- Volume adjustment
- Mute toggle
- Audio device selection (Phase 2)

The main audio flow (capture→encode→send) is a server-side background task, not handler-driven.

---

## 6. Pairing — Trust-On-First-Use (TOFU)

### 6.1 Model

SSH-style trust-on-first-use instead of interactive prompting:

1. Server starts with `--noise` flag, generates identity, prints public key
2. Client connects with `--server-key <hex>`
3. Noise IK handshake completes → server extracts client's public key
4. **First connection from this key:** server auto-pairs, logs warning:
   ```
   [SECURITY] New device paired: "Client-PC" (key: a3f1...beef)
   ```
5. **Subsequent connections:** recognized instantly, no warning
6. **Key change detected:** server refuses connection, logs alert:
   ```
   [SECURITY] WARNING: Device "Client-PC" presented different key! Possible attack.
   ```

### 6.2 PairingStore Integration

```rust
// In connection handler, after Noise handshake:
let client_key = handshake_result.remote_static.unwrap();

match gate.authenticate(&client_key, &device_identity) {
    AuthResult::Authenticated(ctx) => {
        // Known device → proceed
    }
    AuthResult::SilentDrop => {
        // Unknown device → TOFU: auto-pair
        if auto_pair_enabled {
            pairing_store.pair(device_identity, PairingState::Paired);
            log::warn!("New device paired: {}", device_identity.display_name);
            // Retry authentication → now succeeds
        } else {
            // Strict mode: reject unknown devices
            connection.close();
        }
    }
    AuthResult::Blocked => {
        // Blocked device (key changed) → reject
        connection.close();
    }
}
```

### 6.3 Persistence

Paired devices saved to `prism_pairing.json` on each new pairing. Loaded at startup. The existing `PairingStore::persist()` and `PairingStore::restore()` methods handle this (defined in Plan 2).

### 6.4 CLI Flags

| Flag | Behavior |
|---|---|
| `--noise` | Enable Noise IK handshake (default: AllowAllGate) |
| `--auto-pair` | TOFU: auto-pair unknown devices (default with --noise) |
| `--strict` | Reject unknown devices (require manual pre-pairing) |

---

## 7. Phase Mapping

| Component | This Plan | Phase 2+ |
|---|---|---|
| Input forwarding | Keyboard + mouse (absolute + relative) + scroll + Unicode | Touch, pen, gamepad |
| Local cursor | Client-side prediction with server correction | Predictive cursor with RTT compensation |
| Control channel | Heartbeat (zero-alloc), probe, feedback, alerts, shutdown | Profile switching, overlay toggle |
| Quality loop | Event-driven probes → ArcSwap cache → degradation → encoder | Content-aware adaptation |
| Clipboard | Text + image with hash echo suppression | File drag-and-drop, rich format |
| Audio | WASAPI + silence detect + Opus + adaptive jitter + cpal | Multi-channel, spatial, device selection |
| Pairing | TOFU + persistent PairingStore | SPAKE2 short code, Tailscale auto |

---

## 8. File Layout

All new code goes into existing crates — no new crates needed.

```
crates/prism-server/src/
    input_handler.rs            # InputChannelHandler + InputInjector (#[cfg(windows)])
    control_handler.rs          # ControlChannelHandler (heartbeat, probe, feedback routing)
    heartbeat_task.rs           # HeartbeatGenerator (zero-alloc) + per-client sender task
    probe_task.rs               # Per-client quality probe sender task
    quality_task.rs             # Event-driven quality evaluation + ArcSwap cache + encoder adjust
    clipboard_handler.rs        # ClipboardChannelHandler + ClipboardEchoGuard
    clipboard_win32.rs          # #[cfg(windows)] Win32 clipboard access + change listener
    audio_capture.rs            # AudioCapture trait + WasapiCapture (#[cfg(windows)])
    audio_sender.rs             # Silence detect → Opus encode → datagram send task
    main.rs                     # Wire all handlers + tasks into accept loop

crates/prism-client/src/
    input_sender.rs             # Capture minifb keyboard/mouse → send input datagrams
    cursor_renderer.rs          # Local cursor rendering + server correction
    control_client.rs           # Client heartbeat + feedback sender
    performance_tracker.rs      # ClientPerformanceTracker (decode/render times)
    clipboard_client.rs         # Client-side clipboard sync (arboard + echo guard)
    audio_player.rs             # Adaptive jitter buffer → Opus decode → cpal playback
    silence.rs                  # SilenceDetector (shared with server)
    main.rs                     # Wire everything into client loop

crates/prism-protocol/src/
    input.rs                    # InputEvent, MouseButton, TextInput wire types

crates/prism-session/src/
    dispatch.rs                 # REFACTOR: add handle_stream() to ChannelHandler trait
```

### New Dependencies

| Crate | Where | Purpose |
|---|---|---|
| `audiopus` | prism-server, prism-client | Opus encode/decode (bundles libopus) |
| `cpal` | prism-client | Cross-platform audio output |
| `arboard` | prism-client | Cross-platform clipboard |

Windows-only (already have `windows` crate, add features):
- `Win32_Media_Audio` — WASAPI loopback capture
- `Win32_UI_Input_KeyboardAndMouse` — SendInput for input injection
- `Win32_System_DataExchange` — Clipboard APIs

---

## 9. Optimizations Index

| ID | Optimization | Impact | Section |
|---|---|---|---|
| P1 | Pre-built input packet header template | ~5 fewer buffer ops per event | §1.1 |
| S11 | Zero-allocation heartbeat (pre-built Bytes) | 0 heap alloc per heartbeat | §2.1 |
| S12 | ConnectionQuality ArcSwap cache | ~1ns reads vs ~5µs recompute | §3.3 |
| R32 | Input-triggered capture | Up to 16ms faster response | §1.4 |
| — | Hash-based echo suppression | More robust than sequence-based | §4.2 |
| — | Silence detection (-60dB threshold) | ~10KB/sec savings when silent | §5.3 |
| — | Adaptive jitter buffer (20-80ms) | Optimal latency on any network | §5.5 |
| — | Event-driven quality evaluation | CPU savings when stable | §3.1 |
| — | Input batching (DatagramCoalescer) | 50% fewer syscalls for mouse | §1.5 |

---

## 10. Testing Strategy

| Category | What | How |
|---|---|---|
| Unit: InputEvent | Serialize/deserialize roundtrip, all variants | Known bytes |
| Unit: InputInjector | Coordinate mapping (normalized → screen, relative deltas) | Math tests |
| Unit: HeartbeatGenerator | Packet is exactly 16 bytes, parseable header | Roundtrip |
| Unit: SilenceDetector | Detects silence, clears on sound, threshold | Synthetic audio |
| Unit: ClipboardEchoGuard | Hash match suppresses, different data passes | Known data |
| Unit: AdaptiveJitterBuffer | Grows on jitter, shrinks on stability | Simulated arrivals |
| Unit: QualityCache | ArcSwap read/write, concurrent access | Spawn readers+writer |
| Unit: ControlHandler | Routes msg_type correctly | Mock prober |
| Integration: Input roundtrip | Client KeyDown → server receives → inject called | Loopback QUIC |
| Integration: Heartbeat | Both sides exchange heartbeats, no timeout for 10s | Loopback |
| Integration: Probe → quality | Send probes, verify RTT and quality score computed | Loopback |
| Integration: Clipboard text | Set text on server → client receives → verify match | Loopback + arboard |
| Integration: Audio flow | Synthetic sine wave → encode → send → decode → verify | Loopback + cpal |
| Integration: TOFU pairing | Unknown client → auto-pair → reconnect → recognized | Loopback + PairingStore |

---

*PRISM Phase 1 Completion Design v1.1 — CC0 Public Domain*
