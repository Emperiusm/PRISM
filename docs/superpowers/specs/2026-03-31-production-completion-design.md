# PRISM Production Completion — Design Spec

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-31                     |
| Authors     | Ehsan + Claude                 |
| Parent spec | 2026-03-31-phase1-completion-design.md |

This document covers everything needed to take PRISM from a working demo to a production-quality remote desktop: fixing all wiring gaps, implementing missing features, pipelining the display engine, and making the binaries distributable. Organized into three phases: Make It Work (A), Make It Fast (B), Make It Ship (C).

---

## Phase A: Make It Work

### A1. ServerApp / ClientApp Refactor

Both `main.rs` files are monoliths that will become unmaintainable as features are added. Refactor into structs that own state:

```rust
pub struct ServerApp {
    config: ServerConfig,
    session_manager: Arc<Mutex<SessionManager>>,
    conn_store: Arc<ClientConnectionStore>,
    dispatcher: Arc<ChannelDispatcher>,
    tracker: Arc<ChannelBandwidthTracker>,
    acceptor: ConnectionAcceptor,
    quality_monitors: HashMap<ClientId, QualityMonitor>,
    shutdown: ShutdownCoordinator,
}

impl ServerApp {
    pub async fn run(&mut self) -> Result<()>;
    fn handle_connection(&self, conn: quinn::Connection) -> JoinHandle<()>;
    fn spawn_client_tasks(&self, client_id: ClientId, conn: quinn::Connection);
}
```

```rust
pub struct ClientApp {
    config: ClientConfig,
    connection: Option<quinn::Connection>,
    input_sender: InputSender,
    frame_stats: FrameStats,
    clipboard_guard: ClipboardEchoGuard,
    decoder: Decoder,
}
```

Server `main.rs` becomes ~20 lines: parse args → build ServerApp → `app.run().await`. Same for client.

### A2. Structured Logging (tracing)

Replace all `println!`/`eprintln!` with `tracing` crate. This is foundational — every subsequent feature benefits from structured, leveled logging.

```rust
// Before:
println!("[{}] Connected", remote);

// After:
tracing::info!(remote = %remote, "client connected");
```

Dependencies: `tracing = "0.1"`, `tracing-subscriber = "0.3"` with `fmt` feature. Initialize in main with `tracing_subscriber::fmt::init()`.

Log levels:
- ERROR: connection failures, encoder crashes, FFI errors
- WARN: TOFU auto-pair, device-lost recovery, frame drops
- INFO: connect/disconnect, session lifecycle, quality level changes
- DEBUG: heartbeat, probe, individual frame stats
- TRACE: per-packet decode, individual input events

### A3. Win32 SendInput FFI

Wire the actual Win32 `SendInput` API into `InputChannelHandler`. The `input_handler.rs` already has the structure — the `#[cfg(windows)]` block needs real FFI.

Add `"Win32_UI_Input_KeyboardAndMouse"` feature to the `windows` crate dependency in prism-server. The FFI uses:
- `SendInput()` with `INPUT` array
- `KEYBDINPUT` for keyboard events (scancode + vk)
- `MOUSEINPUT` for mouse events (absolute with `MOUSEEVENTF_ABSOLUTE`, relative with `MOUSEEVENTF_MOVE`)
- `KEYEVENTF_UNICODE` for TextInput (codepoint as wScan)

Mouse absolute coordinates: the normalized 0–65535 range maps directly to `MOUSEEVENTF_ABSOLUTE` which also uses 0–65535.

Scroll: `MOUSEEVENTF_WHEEL` with `mouseData` = delta_y (positive = up).

### A4. Client Identity Persistence

Client generates `LocalIdentity` once, saves to `~/.prism/client_identity.json`. On subsequent launches, loads from disk. This makes TOFU pairing work across restarts.

```rust
let identity_path = dirs::home_dir().unwrap().join(".prism/client_identity.json");
let identity = if identity_path.exists() {
    LocalIdentity::load(&identity_path)?
} else {
    let id = LocalIdentity::generate("PRISM Client");
    std::fs::create_dir_all(identity_path.parent().unwrap())?;
    id.save(&identity_path)?;
    id
};
```

`LocalIdentity` already has `load_or_generate()` and `save()` methods from Plan 2.

### A5. ClientConnectionStore Cleanup

Fix the memory leak: call `conn_store.remove(&client_id)` when a client disconnects. In the connection handler task, add cleanup when the recv loop ends:

```rust
// When connection task finishes (recv loop ended):
conn_store.remove(&client_id);
session_manager.lock().await.disconnect(client_id, "connection lost".into());
tracing::info!(client_id = %client_id, "client cleanup complete");
```

### A6. Heartbeat Timeout Task

Spawn a single background task that calls `session_manager.check_heartbeats()` every second:

```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let mut mgr = session_manager.lock().await;
        let actions = mgr.check_heartbeats();
        for (client_id, tombstoned) in actions {
            if tombstoned {
                conn_store.remove(&client_id);
                tracing::warn!(client_id = %client_id, "client tombstoned (heartbeat timeout)");
            }
        }
    }
});
```

### A7. Client Disconnect Detection + Reconnection

Client detects server death via heartbeat timeout: if no data received for 10 seconds, close connection and attempt reconnect every 3 seconds for up to 5 minutes (tombstone window).

```rust
pub struct ReconnectPolicy {
    max_attempts: u32,          // 100 (5min / 3s)
    retry_interval: Duration,   // 3 seconds
    attempt: u32,
}
```

On reconnect, the same client identity is used (A4), so the server recognizes it via TOFU pairing.

### A8. Graceful Shutdown

Server registers a Ctrl+C handler via `tokio::signal::ctrl_c()`. On signal:
1. `ShutdownCoordinator::initiate("User requested shutdown", true)`
2. Send `ShutdownNotice` to all clients via control streams
3. Wait grace period (30s default)
4. Close all connections
5. Persist tombstones to disk for restart recovery
6. Exit

```rust
tokio::select! {
    _ = app.run() => {}
    _ = tokio::signal::ctrl_c() => {
        tracing::info!("Ctrl+C received, shutting down...");
        app.shutdown().await;
    }
}
```

### A9. Quality Probe Task

Per-client background task:
1. `ConnectionProber::generate_probe()` → get probe payload
2. Wrap in PRISM header (CHANNEL_CONTROL, PROBE_REQUEST)
3. `connection.try_send_datagram(probe_datagram)`
4. When `ControlChannelHandler` receives PROBE_RESPONSE, forward to `QualityMonitor::prober_mut().process_echo()`

Probe frequency adapts via `ActivityState`: 2s during streaming, 60s when idle.

### A10. Quality → Encoder Bitrate

Event-driven: when probe echo arrives OR client feedback arrives OR 500ms fallback fires:
1. Read `TransportMetrics` from `QuicConnection::metrics()`
2. `QualityMonitor::update(&metrics)` → `QualityUpdate`
3. Store quality in `ArcSwap<ConnectionQuality>` (S12 optimization)
4. If `level_changed`: call `HwEncoder` to reconfigure bitrate
5. Bitrate-only changes are cheap (no encoder reinit). Resolution changes trigger reinit + IDR.

### A11. Client Feedback

Client measures:
- Decode time per frame (Instant::now around openh264 decode)
- Frames dropped (sequence gaps)
- Decoder queue depth (frames waiting to render)

Every 1s (normal) or 200ms (stressed — queue ≥ 3 or drop rate ≥ 5%):
- Serialize `ClientFeedback` as JSON
- Send on control bidirectional stream via `FramedWriter`

Server `ControlChannelHandler` deserializes, feeds to `QualityMonitor`.

### A12. WASAPI Audio Capture + Opus

**Server:** WASAPI loopback capture (`IAudioClient` with `AUDCLNT_STREAMFLAGS_LOOPBACK`). Dedicated OS thread (COM requires STA). Captures 48kHz stereo float32 in 20ms chunks (960 samples × 2 channels).

`SilenceDetector` (already built) suppresses sending during silence.

Opus encoding via `audiopus` crate (bundles libopus — no system dependency). 128kbps stereo. Each encoded frame ~100-300 bytes. Wrapped in PRISM header with `timestamp_us` for A/V sync.

**Client:** `cpal` crate for audio output. `audiopus` decoder. `AdaptiveJitterBuffer` (already built, tested) absorbs network jitter. Frames fed to cpal callback.

Wire format: PRISM header (16B, CHANNEL_AUDIO, msg_type=0x01, timestamp_us set) + AudioFrameHeader (8B, already defined) + Opus data (~200B). Total ~224 bytes per 20ms = 50 packets/sec = 11.2 KB/sec.

### A13. Clipboard Stream-Based Sending

Currently: client polls clipboard, prints changes. Change to:

1. After session established, open a bidirectional QUIC stream for clipboard (CHANNEL_CLIPBOARD)
2. Wrap in `FramedWriter`/`FramedReader` (already built)
3. On clipboard change: serialize `ClipboardMessage` as JSON, send via `FramedWriter`
4. On receive: deserialize, check echo guard, set local clipboard
5. Bidirectional: both server and client run this logic

Server clipboard: Win32 `AddClipboardFormatListener` on a dedicated window message thread. Client clipboard: `arboard` polling every 250ms (already implemented).

### A14. Input-Triggered Capture

When `InputChannelHandler` processes any input event, signal the DDA capture loop to run an immediate frame:

```rust
// In InputChannelHandler:
self.capture_trigger.try_send(()).ok(); // tokio::sync::mpsc or std::sync::mpsc
```

The capture loop `select!`s on: DDA `AcquireNextFrame` timeout OR trigger signal. On trigger, skip the frame pacer interval.

The existing `InputTriggerCoalescer` (8ms debounce, already built) prevents excessive captures from rapid mouse moves.

### A15. Audit Log Recording

Wire actual events into the existing `AuditLog`:
- `ClientAuthenticated` on successful Noise handshake
- `ClientRejected` on SilentDrop/Blocked
- `ClientDisconnected` on disconnect

`DefaultSecurityGate::authenticate()` already calls `self.audit_log.record()` — verify this is wired, and add disconnect events in SessionManager.

### A16. DDA Device-Lost Recovery

`DdaCapture` state machine already handles `DeviceLost` → `RecoveringDevice` and `AccessLost` → `RecoveringAccess`. Wire into the actual capture loop:

```rust
loop {
    match dda.capture_frame() {
        Ok(Some(pixels)) => { /* encode + send */ }
        Ok(None) => { /* no new frame, sleep */ }
        Err(e) if e.is_recoverable() => {
            tracing::warn!("DDA access lost, recreating duplication...");
            dda.recreate_duplication()?;
        }
        Err(e) if e.is_device_lost() => {
            tracing::error!("DDA device lost, recreating D3D device...");
            dda = DdaDesktopCapture::new()?; // full reinit
        }
        Err(e) => return Err(e.into()),
    }
}
```

### A17. Channel Bandwidth Tracking

In `recv_loop.rs`, after parsing the PrismHeader in the `ChannelDispatch` branch, call:

```rust
record_datagram_bandwidth(&tracker, &header);
```

This already exists as a function — it just needs to be called in the live recv loop (currently only called in tests).

Also record send bandwidth: in the frame sender, after sending each frame:

```rust
tracker.record_send(CHANNEL_DISPLAY, h264_data.len() as u32);
```

### A18. Tombstone Reconnection

On disconnect, `SessionManager::disconnect()` already creates a tombstone. On reconnect (same device_id via TOFU), `SessionManager::new_session()` already calls `tombstones.claim_by_device()`. Verify:
1. Claimed tombstone's `subscribed_channels` are re-requested
2. `ChannelRecoveryState::SendIdr` triggers an immediate keyframe for display
3. Other channels restore appropriately

### A19. Config File (TOML)

```toml
# prism-server.toml
[server]
listen_addr = "0.0.0.0:9876"
throughput_addr = "0.0.0.0:9877"
max_clients = 4
display_name = "My PC"

[security]
mode = "tofu"  # "tofu" | "strict" | "dev"
identity_path = "~/.prism/server_identity.json"
pairing_path = "~/.prism/pairings.json"

[capture]
mode = "dda"  # "dda" | "test-pattern"
resolution = "native"  # "native" | "1920x1080" | "1280x720"
max_fps = 60

[encoder]
backend = "auto"  # "auto" | "nvenc" | "qsv" | "amf" | "software"
bitrate = "5mbps"
preset = "ultra-low-latency"

[audio]
enabled = true
bitrate = "128kbps"
```

Use `toml` crate + `serde::Deserialize` for `ServerConfig`. CLI flags override config file values.

---

## Phase B: Make It Fast

### B1. Pipelined Display Engine

The single-threaded capture→encode→send loop becomes a multi-stage pipeline:

```
Thread 1 (OS thread, pinned): DDA capture → FrameRing<CapturedFrame>
Thread 2 (OS thread, pinned): FrameRing → classify → encode → FrameRing<EncodedFrame>
Async task:                    FrameRing → FramedWriter → QUIC send
```

`FrameRing` (already built, tested, cache-line padded) connects stages. If downstream is slow, upstream drops — stale frames are worse than skipped frames.

DDA capture must run on a dedicated OS thread (COM requires single-threaded apartment). Encoding is CPU-heavy and should not run on the tokio executor.

```rust
// Capture thread
std::thread::Builder::new().name("prism-capture".into()).spawn(move || {
    // COM init, DDA loop, push to frame_ring
});

// Encode thread
std::thread::Builder::new().name("prism-encode".into()).spawn(move || {
    // Pop from capture_ring, encode, push to send_ring
});

// Send task (async)
tokio::spawn(async move {
    // Pop from send_ring, FramedWriter::send() to QUIC stream
});
```

### B2. Stream Reuse for Frame Delivery

Replace `open_uni` per frame with a persistent `FramedWriter` per client:

```rust
// On client connect:
let (send, _recv) = connection.open_bi(StreamPriority::High).await?;
let mut writer = FramedWriter::new(OwnedSendStream::from_quic(send));

// Per frame:
let mut frame_buf = Vec::with_capacity(16 + h264_data.len());
frame_buf.extend_from_slice(&width.to_le_bytes());
frame_buf.extend_from_slice(&height.to_le_bytes());
frame_buf.extend_from_slice(&seq.to_le_bytes());
frame_buf.extend_from_slice(&(h264_data.len() as u32).to_le_bytes());
frame_buf.extend_from_slice(&h264_data);
writer.send(&frame_buf).await?;
```

Client uses `FramedReader` on a persistent stream instead of `accept_uni` per frame.

### B3. Adaptive FPS

Three signals determine target FPS:
1. **Content rate:** DDA damage frequency (static desktop → 0fps, active coding → 10fps, video → 60fps). Use `FramePacer` (already built).
2. **Network quality:** `DegradationLadder` current level's `max_fps`.
3. **Client capacity:** `ClientFeedback.decoder_queue_depth` — if growing, reduce FPS.

Target FPS = min(content_rate × 1.2, degradation_max_fps, client_max_fps).

### B4. Backpressure (Frame Skipping)

Before sending a frame, check QUIC send buffer depth:

```rust
let stats = connection.stats();
if stats.path.cwnd > 0 && stats.path.in_flight > stats.path.cwnd * 80 / 100 {
    // >80% of congestion window used — skip this frame
    frames_dropped += 1;
    continue;
}
```

Alternative: use `FrameRing` between encode and send. If the ring is full, the oldest frame is overwritten (producer-wins). The send task always gets the freshest frame.

### B5. Double-Buffered DDA Textures

Wire `TexturePool` (already built, 8 tests) into the DDA capture loop:

```rust
let slot = texture_pool.acquire_write()?;
// DDA copies to texture[slot]
texture_pool.commit_write(slot);
// Encoder reads from texture[slot] via acquire_read()
// On encode complete: texture_pool.release_read(slot)
```

Overlaps capture of frame N with encode of frame N-1. Eliminates fence wait (~1ms savings per frame).

### B6. NV12 DDA Output Format

Try requesting NV12 from DDA's output duplication. If the GPU supports it, skip the BGRA→YUV CPU conversion entirely:

```rust
// Try NV12 first
let result = output1.DuplicateOutput2(
    &device,
    0,
    &[DXGI_FORMAT_NV12, DXGI_FORMAT_B8G8R8A8_UNORM], // preferred formats
);
let duplication = match result {
    Ok(dup) => {
        let mut desc = DXGI_OUTDUPL_DESC::default();
        dup.GetDesc(&mut desc);
        if desc.ModeDesc.Format == DXGI_FORMAT_NV12 {
            tracing::info!("DDA using NV12 output (zero CPU conversion)");
        }
        dup
    }
    Err(_) => {
        // Fallback: standard DuplicateOutput (BGRA)
        output1.DuplicateOutput(&device)?
    }
};
```

When NV12 is available: pass directly to encoder (openh264 accepts NV12 via `YUVBuffer`). When BGRA: use existing `bgra_to_yuv420_raw()` (can be SIMD-optimized later with `std::arch`).

### B7. Pre-Allocated Encode Output Buffer

Reuse a single `Vec<u8>` for encoder output instead of allocating per frame:

```rust
let mut encode_buf = Vec::with_capacity(1024 * 1024); // 1MB pre-alloc
// Per frame:
encode_buf.clear();
encoder.encode_into(&yuv, &mut encode_buf)?;
```

If `openh264` doesn't support encode-into, use `encoder.encode()` and swap the returned vec into our buffer.

### B8. Capability Negotiation Over Wire

After Noise handshake (or QUIC connect in dev mode), client sends `ClientCapabilities` on the control bidirectional stream:

```
Client → Server: ClientCapabilities (JSON via FramedWriter)
Server → Client: ServerCapabilities + NegotiationResult (JSON via FramedWriter)
```

Server calls `CapabilityNegotiator::negotiate()` (already implemented), returns granted channels + codec + resolution.

### B9. Multi-Monitor Selection

Server enumerates monitors via DDA (or `DxgiOutputInfo` from prism-platform-windows), sends list in `ServerCapabilities`. Client selects which monitor during negotiation. Server creates DDA duplication for the selected output.

### B10. Cursor Shape Transmission

DDA provides cursor shape via `IDXGIOutputDuplication::GetFramePointerShape()`. Server:
1. Hash the cursor shape data
2. If hash changed from last sent: send on reliable stream (CHANNEL_DISPLAY, CURSOR_SHAPE msg_type)
3. `CursorManager` (already built) tracks this

Client stores received cursor shapes, renders the correct one.

### B11. Client-Side Cursor Prediction

Client renders cursor at its local position with zero latency:
1. Hide OS cursor when over PRISM window (`minifb` may support this)
2. Track local mouse position
3. Render cursor sprite from last received `CursorShape` at local position
4. When display frame arrives with `cursor_x/cursor_y` in `SlicePayloadHeader`: compare to local prediction
5. If divergent by >5px: snap to server position

### B12. IDR Request on Frame Gap

Client's `FrameGapDetector` (already built, tested) detects sequence gaps. When gap detected:
1. Build PRISM header (CHANNEL_CONTROL, IDR_REQUEST)
2. Send as datagram to server
3. Server's `ControlChannelHandler` triggers immediate IDR on next encode

1-second cooldown between IDR requests (already in FrameGapDetector).

### B13. Bandwidth Arbiter Wiring

Feed the arbiter with actual metrics:
1. `ChannelBandwidthTracker` provides per-channel byte counts (A17)
2. Periodically (100ms): snapshot byte counts → compute BPS per channel
3. Feed to `BandwidthArbiter::rebalance()`
4. Read `AllocationHandle::allocated_bps()` in the encoder → set target bitrate

Display Engine implements `BandwidthNeeds`:
- Static desktop: min=100Kbps, ideal=500Kbps, max=2Mbps
- Active coding: min=500Kbps, ideal=2Mbps, max=5Mbps
- Video playing: min=2Mbps, ideal=8Mbps, max=20Mbps

### B14. Speculative IDR on Window Events

Register Win32 `SetWinEventHook()` for `EVENT_SYSTEM_FOREGROUND` and `EVENT_OBJECT_SHOW`:

```rust
#[cfg(windows)]
fn register_window_hooks(tx: mpsc::Sender<WindowEvent>) {
    // SetWinEventHook with callback that sends WindowEvent to tx
}
```

On `ForegroundChanged` (Alt+Tab):
1. Immediate DDA capture (bypass frame pacer)
2. Mark `is_speculative = true`, `QualityTier::Preview`
3. Encode at 50% bitrate (faster)
4. Send with `is_preview = true`
5. Next regular frame has `replaces_seq` pointing to the preview

### B15. Parallel Encoder Workers

Spawn 2-3 dedicated OS threads for encoding. Use `EncodeQueue` (already built, tested) for job distribution:

```rust
// Classifier pushes jobs
encode_queue.push_high(EncodeJob { region_type: Video, ... });
encode_queue.push_normal(EncodeJob { region_type: Text, ... });

// Worker threads
for _ in 0..num_workers {
    std::thread::spawn(move || {
        loop {
            if let Some(job) = encode_queue.steal() {
                let encoded = encoder.encode(job)?;
                send_ring.try_push(encoded);
            }
        }
    });
}
```

Phase 1: 1 worker (current). Phase B: 2-3 workers with work-stealing.

### B16. Overlay Packet + Client Rendering

Server builds `OverlayPacket` (128B, already defined) every 100ms:
- FPS, bitrate, RTT, loss, degradation level
- Per-channel bandwidth
- Frame latency breakdown (capture, encode, network, decode, render)

Sent as datagram on CHANNEL_CONTROL (OVERLAY_DATA msg_type). Client renders as a semi-transparent HUD over the display.

### B17. Throughput Connection

Server binds a second QUIC endpoint with `throughput_transport_config()` (Cubic, AF11, large windows). On client connect:
1. Server sends throughput endpoint address in handshake response
2. Client opens second connection to throughput addr
3. `UnifiedConnection` wraps both: display/input/audio on latency, file transfers on throughput

### B18. Connection Rate Limiter

Token bucket per source IP. Check before accepting QUIC handshake:

```rust
pub struct ConnectionRateLimiter {
    buckets: HashMap<IpAddr, TokenBucket>,
    max_per_minute: u32,  // 10
}
```

Reject connections exceeding rate → silent timeout (not CONNECTION_CLOSE).

### B19. Static Region Caching

`StaticAtlasTracker` (already built with LRU eviction) identifies regions that haven't changed for 30 frames. Protocol:

1. Server detects static region (hash stable for 30 frames)
2. Server sends region data with `StaticDecision::SendAndCache` flag
3. Client caches the region texture in a local atlas
4. Subsequent frames: server sends `StaticDecision::Unchanged` → client composites from cache
5. Hash change → invalidate cache, send fresh data

40-60% bandwidth savings for typical desktop sessions.

---

## Phase C: Make It Ship

### C1. Frame Tracing Instrumentation

Add `tracing::Span` at each pipeline stage with `Instant::now()` timestamps:

```rust
let capture_start = Instant::now();
let frame = dda.capture_frame()?;
let capture_us = capture_start.elapsed().as_micros();

let encode_start = Instant::now();
let h264 = encoder.encode(&frame)?;
let encode_us = encode_start.elapsed().as_micros();
```

Feed to `FrameTracer` (already built) for adaptive sampling. Slow frames (>p95) always traced.

### C2. Per-Client Metrics Isolation

Each client gets its own `MetricsRecorder` instance:

```rust
struct ClientMetrics {
    display: MetricsRecorder<7, 6, 4>,  // DisplayRecorder type
    transport: MetricsRecorder<6, 3, 2>,
}
```

Collector aggregates across clients for Prometheus labels: `prism_display_frames_encoded{client="uuid"}`.

### C3. Time-Series History Recording

`MetricsTimeSeries` (already built, tested) records samples every 1 second. Collector task:

```rust
// Every second:
for (name, recorder) in recorders {
    let snapshot = recorder.snapshot();
    time_series.record("rtt_us", now_secs, snapshot.gauges[RTT_GAUGE] as f64);
    time_series.record("fps", now_secs, snapshot.gauges[FPS_GAUGE] as f64);
}
```

5-minute ring buffer (300 samples). Feeds overlay sparkline graphs.

### C4. Release Build Profile

```toml
[profile.release]
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
opt-level = 3

[profile.release.package.openh264-sys2]
opt-level = 3  # C code benefits from aggressive optimization
```

Expected: 2-3x faster encode, 50% smaller binary.

### C5. Windows Service Support

Use `windows-service` crate:

```rust
#[cfg(windows)]
fn main() {
    if std::env::args().any(|a| a == "--service") {
        windows_service::service_dispatcher::start("PRISMServer", service_main)?;
    } else {
        // Normal console mode
        run_server()?;
    }
}
```

Register with `sc create PRISMServer binPath= "C:\...\prism-server.exe --service"`.

### C6. Installer (WiX MSI)

Use `cargo-wix` or manual WiX XML:
- Installs prism-server.exe + prism-client.exe
- Creates Start Menu shortcuts
- Optionally registers Windows service
- Adds firewall rules for QUIC ports
- Config file template in `%AppData%\PRISM\`

### C7. Auto-Update

Check GitHub releases API on startup (daily or on-demand):

```rust
pub struct UpdateChecker {
    current_version: semver::Version,
    release_url: String,
    check_interval: Duration,  // 24 hours
}
```

Download new binary to temp, verify SHA256, replace-on-restart. Windows: use `MoveFileEx` with `MOVEFILE_DELAY_UNTIL_REBOOT` for in-use binary replacement.

---

## Dependencies Summary

### New Workspace Dependencies

| Crate | Phase | Purpose |
|---|---|---|
| `tracing` | A2 | Structured logging |
| `tracing-subscriber` | A2 | Log formatting |
| `audiopus` | A12 | Opus encode/decode |
| `cpal` | A12 | Audio output |
| `toml` | A19 | Config file parsing |
| `dirs` | A4 | Home directory |
| `ctrlc` or `tokio::signal` | A8 | Signal handling (already in tokio) |
| `windows-service` | C5 | Windows service |
| `semver` | C7 | Version comparison |
| `cargo-wix` | C6 | MSI installer (build tool) |

### Existing Dependencies Needing Feature Additions

| Crate | Feature to Add | Phase |
|---|---|---|
| `windows` | `Win32_UI_Input_KeyboardAndMouse` | A3 |
| `windows` | `Win32_Media_Audio` | A12 |
| `windows` | `Win32_System_DataExchange` | A13 |

---

## Testing Strategy

### Phase A Tests

| Test | Type | What it verifies |
|---|---|---|
| SendInput coordinate mapping | Unit | Normalized → screen pixel math |
| Heartbeat timeout detection | Unit | check_heartbeats returns stale clients |
| Quality probe → RTT computation | Unit | ProbeResult from echo |
| Quality → encoder bitrate change | Integration | Level change triggers reconfig |
| Silence detection accuracy | Unit | Already tested (5 tests) |
| Audio header roundtrip | Unit | Already tested |
| Clipboard echo suppression | Unit | Already tested (6 tests) |
| Config file parsing | Unit | TOML → ServerConfig |
| Client reconnection | Integration | Disconnect → reconnect → session restored |
| Graceful shutdown notice | Integration | Ctrl+C → client receives ShutdownNotice |
| DDA recovery after device lost | Integration | Simulate lost → verify recovery |
| E2E audio flow | Integration | Synthetic sine → encode → send → decode |

### Phase B Tests

| Test | Type | What it verifies |
|---|---|---|
| Pipeline throughput | Perf | >60fps at 1080p through FrameRing |
| FrameRing backpressure | Unit | Full ring drops oldest |
| Adaptive FPS convergence | Unit | Static → 0fps, active → target |
| Capability negotiation roundtrip | Integration | Client caps → server intersection |
| IDR request on gap | Integration | Gap detected → IDR within 1 RTT |
| Cursor prediction accuracy | Unit | Local vs server position <5px |
| Rate limiter enforcement | Unit | >10 connections/min rejected |
| Static region cache hit rate | Integration | Repeated frames → Unchanged |
| NV12 fallback to BGRA | Integration | Unsupported GPU → BGRA path |

### Phase C Tests

| Test | Type | What it verifies |
|---|---|---|
| Frame tracing timestamps | Unit | Monotonic, non-zero |
| Time-series ring capacity | Unit | Already tested |
| Config file round-trip | Unit | Write → read → identical |
| Release binary size | CI | <20MB |

---

*PRISM Production Completion Design v1.0 — CC0 Public Domain*
