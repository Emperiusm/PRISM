# Plan 13: Phase A Finish — Remaining Wiring + Config (Phase A, Part 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Phase A by wiring input-triggered capture, audit log recording, DDA error recovery, channel bandwidth tracking, tombstone reconnection verification, and adding TOML config file support.

**Architecture:** Input-triggered capture signals the DDA loop via `tokio::sync::mpsc` when InputChannelHandler processes events. Audit log records connect/disconnect events in the existing AuditLog ring buffer. DDA recovery is wired into the capture loop's error handling (already has state machine, needs runtime integration). Channel bandwidth tracking calls `record_recv()` in the live recv loop. Config file uses `toml` crate + `serde::Deserialize` for ServerConfig.

**Tech Stack:** `toml` (config parsing), existing prism crates

**Spec refs:**
- Production Completion: `docs/superpowers/specs/2026-03-31-production-completion-design.md` (Phase A: A14-A19)

---

## File Structure

```
crates/prism-server/src/
    server_app.rs               # Wire input trigger, audit, bandwidth tracking
    config.rs                   # Extend ServerConfig with TOML deserialization

crates/prism-protocol/src/
    (no changes)

Cargo.toml (workspace)          # Add toml
```

---

## Task 1: Input-Triggered Capture Signal

**Files:**
- Modify: `crates/prism-server/src/input_handler.rs`
- Modify: `crates/prism-server/src/server_app.rs`

When the server receives an input event, it should trigger an immediate DDA capture (bypass the frame pacer interval, cutting up to 16ms latency).

- [ ] **Step 1: Add capture trigger channel to InputChannelHandler**

READ `crates/prism-server/src/input_handler.rs`. Add an optional `mpsc::Sender<()>` that fires on every input event:

```rust
pub struct InputChannelHandler {
    screen_width: u32,
    screen_height: u32,
    stats: Arc<InputStats>,
    capture_trigger: Option<tokio::sync::mpsc::Sender<()>>,
}

impl InputChannelHandler {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        Self { screen_width, screen_height, stats: Arc::new(InputStats::default()), capture_trigger: None }
    }

    pub fn with_capture_trigger(mut self, tx: tokio::sync::mpsc::Sender<()>) -> Self {
        self.capture_trigger = Some(tx);
        self
    }
}
```

In `handle_datagram`, after `process_event`:
```rust
if let Some(ref tx) = self.capture_trigger {
    let _ = tx.try_send(()); // non-blocking, drop if full
}
```

The existing `InputTriggerCoalescer` (8ms debounce) from prism-display can be used in the capture loop to prevent excessive captures. For now, a bounded channel with capacity 1 acts as natural debounce (try_send drops if one trigger is already pending).

Update tests: existing tests pass (capture_trigger is None by default).

Add one new test:
```rust
#[tokio::test]
async fn capture_trigger_fires_on_input() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let handler = InputChannelHandler::new(1920, 1080).with_capture_trigger(tx);
    let data = make_input_datagram(InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 });
    handler.handle_datagram(Uuid::nil(), data).await.unwrap();
    assert!(rx.try_recv().is_ok());
}
```

- [ ] **Step 2: Wire trigger in server_app.rs**

READ server_app.rs, find where InputChannelHandler is created. Add the trigger channel:

```rust
let (capture_tx, capture_rx) = tokio::sync::mpsc::channel::<()>(1);
let input_handler = InputChannelHandler::new(width, height).with_capture_trigger(capture_tx);
dispatcher.register(Arc::new(input_handler));
```

In the frame sender task, `select!` on the capture trigger alongside the interval timer:
```rust
tokio::select! {
    _ = interval.tick() => { /* normal frame pacing */ }
    _ = capture_rx.recv() => { /* input triggered — capture immediately */ }
}
```

- [ ] **Step 3: Verify, commit**

```bash
cargo test -p prism-server
git commit -m "feat(server): input-triggered DDA capture signal (-16ms latency)"
```

---

## Task 2: Audit Log Recording

**Files:**
- Modify: `crates/prism-server/src/server_app.rs`

- [ ] **Step 1: Wire audit events**

READ `crates/prism-security/src/audit.rs` to understand AuditEvent variants and AuditLog::record().

In `handle_connection`, after successful session creation:
```rust
// Record audit event
if let Some(gate) = &self_gate {
    gate.audit(AuditEvent::ClientAuthenticated {
        device_id: device_id,
        device_name: format!("Client-{}", &client_id.to_string()[..8]),
    });
}
```

On disconnect/cleanup:
```rust
// AuditEvent for disconnect (if audit trait supports it — check available variants)
```

READ the actual AuditEvent enum to see what variants exist. Use what's available.

This may be as simple as adding 2-3 lines to the existing handle_connection function.

- [ ] **Step 2: Verify, commit**

```bash
cargo build -p prism-server
git commit -m "feat(server): record audit events on connect/disconnect"
```

---

## Task 3: Channel Bandwidth Tracking in Recv Loop

**Files:**
- Modify: `crates/prism-server/src/recv_loop.rs`

- [ ] **Step 1: Verify bandwidth tracking is called**

READ `crates/prism-server/src/recv_loop.rs`. The `spawn_recv_loop` function already has `classify_datagram` and `record_datagram_bandwidth`. Verify that `record_datagram_bandwidth` is actually called in the live `ChannelDispatch` branch.

If it's not called (just classified but not tracked), add the call:
```rust
DatagramAction::ChannelDispatch { channel_id } => {
    if let Ok(header) = PrismHeader::decode_from_slice(&data) {
        record_datagram_bandwidth(&tracker, &header); // THIS LINE
    }
    dispatcher.dispatch(client_id, channel_id, data).await;
}
```

Also add send tracking in the frame sender. READ server_app.rs frame sender section, add after each frame send:
```rust
tracker.record_send(CHANNEL_DISPLAY, h264_data.len() as u32);
```

- [ ] **Step 2: Add a test**

```rust
#[test]
fn bandwidth_tracker_records_after_dispatch() {
    let tracker = prism_session::ChannelBandwidthTracker::new();
    let header = prism_protocol::header::PrismHeader {
        version: 0, channel_id: 0x001, msg_type: 0, flags: 0,
        sequence: 0, timestamp_us: 0, payload_length: 5000,
    };
    prism_server::record_datagram_bandwidth(&tracker, &header);
    assert_eq!(tracker.recv_bytes(0x001), 5000);
}
```

This test may already exist — check the existing tests in recv_loop.rs.

- [ ] **Step 3: Verify, commit**

```bash
cargo test -p prism-server
git commit -m "feat(server): ensure channel bandwidth tracking in recv loop + frame sender"
```

---

## Task 4: TOML Config File

**Files:**
- Modify: `Cargo.toml` (workspace) — add `toml = "0.8"`
- Modify: `crates/prism-server/Cargo.toml` — add `toml = { workspace = true }`
- Modify: `crates/prism-server/src/config.rs`

- [ ] **Step 1: Add toml dependency**

- [ ] **Step 2: Make ServerConfig deserializable from TOML**

READ `crates/prism-server/src/config.rs` to see the current ServerConfig.

Add `serde::Deserialize` derive. Add a `load_from_file` method:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,
    // ... all fields with serde defaults
}

fn default_listen_addr() -> SocketAddr { "0.0.0.0:7000".parse().unwrap() }
// ... other default functions

impl ServerConfig {
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: ServerConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_or_default(path: &std::path::Path) -> Self {
        match Self::load_from_file(path) {
            Ok(config) => config,
            Err(_) => Self::default(),
        }
    }
}
```

Example TOML file (`prism-server.toml`):
```toml
listen_addr = "0.0.0.0:9876"
max_clients = 4
display_name = "My PC"
total_bandwidth_bps = 100000000
```

- [ ] **Step 3: Wire into server_app.rs**

In `ServerApp::new()`, try loading config from `prism-server.toml` in the current directory:

```rust
let config = ServerConfig::load_or_default(std::path::Path::new("prism-server.toml"));
```

- [ ] **Step 4: Add tests**

```rust
#[test]
fn config_from_toml_string() {
    let toml = r#"
        listen_addr = "127.0.0.1:5555"
        max_clients = 2
    "#;
    let config: ServerConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.listen_addr.port(), 5555);
    assert_eq!(config.max_clients, 2);
}

#[test]
fn config_default_values() {
    let config: ServerConfig = toml::from_str("").unwrap();
    assert_eq!(config.max_clients, 4); // default
}
```

- [ ] **Step 5: Verify, commit**

```bash
cargo test -p prism-server -- config
git commit -m "feat(server): TOML config file support with serde defaults"
```

---

## Task 5: Tombstone Reconnection Verification

**Files:**
- Modify: `crates/prism-server/tests/e2e_frame_flow.rs`

Verify the existing tombstone reconnection path works correctly with the refactored ServerApp.

- [ ] **Step 1: Add/verify test**

The test from Plan 6 (`integration_reconnect_via_tombstone`) should still exist and pass. Verify it. If it was lost during the ServerApp refactor, re-add it:

```rust
#[tokio::test]
async fn tombstone_reconnection_preserves_channels() {
    // This test already exists — verify it passes
    // If not: create SessionManager, connect A, disconnect, reconnect same device_id → channels restored
}
```

Also add a test verifying the full DDA recovery state machine (pure logic, no actual DDA):

```rust
#[test]
fn dda_capture_recovers_from_access_lost() {
    use prism_platform_windows::*;
    let mut cap = DdaCapture::new(DdaCaptureConfig::default());
    cap.init_pool(1920, 1080);
    // Simulate access lost
    let state = cap.handle_error(&PlatformError::AccessLost);
    assert_eq!(state, DdaCaptureState::RecoveringAccess);
    // After recovery, state should allow transitioning back to Capturing
}
```

- [ ] **Step 2: Verify workspace, commit**

```bash
cargo test --workspace
git commit -m "test: verify tombstone reconnection + DDA recovery state machine"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Input-triggered capture signal | 1 |
| 2 | Audit log recording | 0 (wiring) |
| 3 | Channel bandwidth tracking | 0-1 (verify existing) |
| 4 | TOML config file | 2 |
| 5 | Tombstone + DDA recovery verification | 1-2 |
| **Total** | | **~5** |

**After this plan:** Phase A is complete. All wiring gaps are closed. The system is reliable with:
- Input → immediate capture trigger
- Audit trail for connect/disconnect
- Per-channel bandwidth tracking
- TOML config file (no more 20 CLI flags)
- Tombstone reconnection verified
- DDA recovery verified

**Next:** Phase B (Make It Fast) — pipelined display, stream reuse, adaptive FPS, NV12, cursor prediction, capability negotiation, etc.
