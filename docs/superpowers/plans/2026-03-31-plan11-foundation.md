# Plan 11: Foundation — Refactor + Core Wiring (Phase A, Part 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the monolithic main.rs files into structured ServerApp/ClientApp, add `tracing` for structured logging, wire Win32 SendInput FFI, persist client identity, fix ClientConnectionStore leak, add heartbeat timeout task, client disconnect detection, and graceful shutdown.

**Architecture:** Server main.rs (407 lines) and client main.rs (451 lines) are refactored into `ServerApp`/`ClientApp` structs that own all state with methods for each lifecycle phase. All `println!`/`eprintln!` calls are replaced with `tracing` macros. Win32 SendInput is wired via the existing `windows` crate. Session lifecycle is completed with heartbeat timeout → tombstone and Ctrl+C graceful shutdown.

**Tech Stack:** `tracing` + `tracing-subscriber` (logging), `windows` crate (SendInput), `dirs` (home directory for client identity), `tokio::signal` (Ctrl+C handler)

**Spec refs:**
- Production Completion: `docs/superpowers/specs/2026-03-31-production-completion-design.md` (Phase A: A1-A8)

---

## File Structure

```
crates/prism-server/src/
    server_app.rs               # ServerApp struct (extracted from main.rs)
    main.rs                     # ~20 lines: parse args → ServerApp::new() → app.run()

crates/prism-client/src/
    client_app.rs               # ClientApp struct (extracted from main.rs)
    main.rs                     # ~20 lines: parse args → ClientApp::new() → app.run()

Cargo.toml (workspace root)     # add tracing, tracing-subscriber, dirs
```

---

## Task 1: Add tracing + dirs Dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/prism-server/Cargo.toml`
- Modify: `crates/prism-client/Cargo.toml`

- [ ] **Step 1: Add to workspace dependencies**

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt"] }
dirs = "6"
```

Add `tracing = { workspace = true }`, `tracing-subscriber = { workspace = true }`, `dirs = { workspace = true }` to both prism-server and prism-client Cargo.toml.

- [ ] **Step 2: Verify both crates compile**

```bash
cargo check -p prism-server
cargo check -p prism-client
```

- [ ] **Step 3: Commit**

```bash
git commit -m "chore: add tracing, tracing-subscriber, dirs to workspace"
```

---

## Task 2: ServerApp Refactor

**Files:**
- Create: `crates/prism-server/src/server_app.rs`
- Modify: `crates/prism-server/src/main.rs` (rewrite to ~30 lines)
- Modify: `crates/prism-server/src/lib.rs`

This is the biggest task — extract the 407-line main.rs into a structured `ServerApp`.

- [ ] **Step 1: Read the full current main.rs and understand every section**

The current main.rs has these sections:
1. CLI arg parsing (--dda, --noise)
2. Server identity generation
3. ServerConfig + TLS cert
4. Capture backend selection
5. SessionManager + ChannelDispatcher + tracker creation
6. ClientConnectionStore creation
7. QUIC endpoint binding
8. Activity channel + task
9. Frame sender task (~100 lines)
10. Accept loop (~100 lines with Noise handshake)

- [ ] **Step 2: Create server_app.rs**

Extract all of this into:

```rust
pub struct ServerApp {
    // Config
    config: ServerConfig,
    use_dda: bool,
    noise_mode: bool,

    // Core
    session_manager: Arc<Mutex<SessionManager>>,
    conn_store: Arc<ClientConnectionStore>,
    dispatcher: Arc<ChannelDispatcher>,
    tracker: Arc<ChannelBandwidthTracker>,
    acceptor: ConnectionAcceptor,
    server_identity: Arc<LocalIdentity>,

    // Channels
    activity_tx: mpsc::Sender<ClientId>,
    shutdown: ShutdownCoordinator,
}

impl ServerApp {
    pub fn new(use_dda: bool, noise_mode: bool) -> Result<Self, Box<dyn std::error::Error>>;
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    pub async fn shutdown(&mut self);

    // Private methods extracted from main.rs
    async fn handle_connection(&self, quinn_conn: quinn::Connection);
    async fn run_noise_handshake(&self, conn: &quinn::Connection) -> Result<[u8; 32], Box<dyn std::error::Error>>;
}
```

The `run()` method contains the accept loop. `handle_connection()` contains the per-connection logic (Noise handshake, session creation, recv loop spawn, heartbeat spawn). The frame sender task is spawned in `run()`.

Replace all `println!`/`eprintln!` with `tracing::info!`/`tracing::warn!`/`tracing::error!` as you extract.

- [ ] **Step 3: Rewrite main.rs**

```rust
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let use_dda = std::env::args().any(|a| a == "--dda");
    let noise_mode = std::env::args().any(|a| a == "--noise");

    let mut app = prism_server::ServerApp::new(use_dda, noise_mode)?;

    tokio::select! {
        result = app.run() => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down...");
            app.shutdown().await;
            Ok(())
        }
    }
}
```

- [ ] **Step 4: Update lib.rs**

Add `pub mod server_app;` and `pub use server_app::ServerApp;`

- [ ] **Step 5: Verify build + all tests pass**

```bash
cargo build -p prism-server
cargo test --workspace
```

- [ ] **Step 6: Commit**

```bash
git commit -m "refactor(server): extract ServerApp from main.rs, add tracing + Ctrl+C shutdown"
```

---

## Task 3: ClientApp Refactor

**Files:**
- Create: `crates/prism-client/src/client_app.rs`
- Modify: `crates/prism-client/src/main.rs` (rewrite to ~30 lines)
- Modify: `crates/prism-client/src/lib.rs`

Same pattern as server: extract the 451-line main.rs into `ClientApp`.

- [ ] **Step 1: Read the full current client main.rs**

Current client sections:
1. CLI arg parsing (server addr, --noise)
2. QUIC connect
3. Noise handshake (optional)
4. Frame struct + channel
5. Async receiver task (accept_uni, decode H.264, send Frame via channel)
6. Heartbeat sender task
7. Input forward task
8. Main thread: window creation + render loop + input capture + clipboard polling

- [ ] **Step 2: Create client_app.rs**

```rust
pub struct ClientConfig {
    pub server_addr: std::net::SocketAddr,
    pub noise_key: Option<[u8; 32]>,
    pub identity_path: std::path::PathBuf,
}

pub struct ClientApp {
    config: ClientConfig,
    // Connection state set after connect()
}

impl ClientApp {
    pub fn new(config: ClientConfig) -> Self;
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}
```

Replace all `println!`/`eprintln!` with tracing macros.

**Client identity persistence (A4):** Load or generate identity from `~/.prism/client_identity.json`:

```rust
let identity_path = dirs::home_dir()
    .unwrap_or_default()
    .join(".prism")
    .join("client_identity.json");
let identity = prism_security::identity::LocalIdentity::load_or_generate(
    &identity_path, "PRISM Client"
)?;
```

- [ ] **Step 3: Rewrite client main.rs**

```rust
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server_addr = std::env::args().nth(1)
        .unwrap_or_else(|| "127.0.0.1:7000".to_string())
        .parse()?;

    let noise_key = std::env::args()
        .position(|a| a == "--noise")
        .and_then(|i| std::env::args().nth(i + 1))
        .map(|hex_str| {
            let bytes = hex::decode(&hex_str).expect("invalid hex key");
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            key
        });

    let identity_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".prism")
        .join("client_identity.json");

    let config = prism_client::ClientConfig {
        server_addr,
        noise_key,
        identity_path,
    };

    let mut app = prism_client::ClientApp::new(config);
    app.run().await
}
```

- [ ] **Step 4: Update lib.rs**

Add `pub mod client_app;` and `pub use client_app::{ClientApp, ClientConfig};`

- [ ] **Step 5: Verify build + tests**

```bash
cargo build -p prism-client
cargo test --workspace
```

- [ ] **Step 6: Commit**

```bash
git commit -m "refactor(client): extract ClientApp from main.rs, add tracing + identity persistence"
```

---

## Task 4: Win32 SendInput FFI

**Files:**
- Modify: `crates/prism-server/src/input_handler.rs`
- Modify: `crates/prism-server/Cargo.toml` (add windows feature)

Wire real Win32 SendInput into the existing InputChannelHandler.

- [ ] **Step 1: Add Windows feature**

In `crates/prism-server/Cargo.toml`, in the `[target.'cfg(windows)'.dependencies]` section, add `"Win32_UI_Input_KeyboardAndMouse"` to the windows features list.

- [ ] **Step 2: Implement inject_input function**

READ `crates/prism-server/src/input_handler.rs` first. The `#[cfg(windows)]` block currently just increments a counter. Replace with actual SendInput calls.

READ the `windows` crate docs for `SendInput`, `INPUT`, `KEYBDINPUT`, `MOUSEINPUT`, and the various `KEYEVENTF_*`/`MOUSEEVENTF_*` flags.

Key mappings:
- KeyDown → `SendInput` with `KEYBDINPUT { wVk, wScan: scancode, dwFlags: 0 }`
- KeyUp → same with `KEYEVENTF_KEYUP`
- TextInput → `KEYBDINPUT { wVk: 0, wScan: codepoint as u16, dwFlags: KEYEVENTF_UNICODE }`
- MouseMove → `MOUSEINPUT { dx: x, dy: y, dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE }`
- MouseMoveRelative → `MOUSEINPUT { dx, dy, dwFlags: MOUSEEVENTF_MOVE }`
- MouseDown/Up → appropriate `MOUSEEVENTF_LEFTDOWN`/`MOUSEEVENTF_LEFTUP` etc.
- MouseScroll → `MOUSEINPUT { mouseData: delta_y, dwFlags: MOUSEEVENTF_WHEEL }`

On error: log via tracing, increment `events_failed` counter.

- [ ] **Step 3: Verify build on Windows**

```bash
cargo build -p prism-server
cargo test -p prism-server
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(server): wire Win32 SendInput FFI into InputChannelHandler"
```

---

## Task 5: Fix ClientConnectionStore Leak + Heartbeat Timeout Task

**Files:**
- Modify: `crates/prism-server/src/server_app.rs`

- [ ] **Step 1: Add cleanup on disconnect**

In `ServerApp::handle_connection()`, after the recv loop ends (connection dropped or errored):

```rust
// Cleanup when connection task finishes
conn_store.remove(&client_id);
session_manager.lock().await.disconnect(client_id, "connection lost".into());
tracing::info!(client_id = %client_id, "client cleanup complete");
```

- [ ] **Step 2: Add heartbeat timeout task in ServerApp::run()**

Spawn a background task that checks heartbeats every second:

```rust
let sm_heartbeat = self.session_manager.clone();
let cs_heartbeat = self.conn_store.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let mut mgr = sm_heartbeat.lock().await;
        let actions = mgr.check_heartbeats();
        for (client_id, tombstoned) in &actions {
            if *tombstoned {
                cs_heartbeat.remove(client_id);
                tracing::warn!(client_id = %client_id, "client tombstoned (heartbeat timeout)");
            } else {
                tracing::debug!(client_id = %client_id, "client suspended");
            }
        }
    }
});
```

- [ ] **Step 3: Verify, commit**

```bash
cargo build -p prism-server
cargo test --workspace
git commit -m "fix(server): cleanup ClientConnectionStore on disconnect + heartbeat timeout task"
```

---

## Task 6: Client Disconnect Detection

**Files:**
- Modify: `crates/prism-client/src/client_app.rs`

- [ ] **Step 1: Add reconnection logic**

In `ClientApp::run()`, wrap the main connection+receive logic in a reconnect loop:

```rust
pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let mut attempt = 0u32;
    let max_attempts = 100; // ~5 minutes at 3s intervals

    loop {
        match self.connect_and_stream().await {
            Ok(()) => {
                tracing::info!("session ended cleanly");
                break Ok(());
            }
            Err(e) => {
                attempt += 1;
                if attempt > max_attempts {
                    tracing::error!("max reconnect attempts exceeded");
                    break Err(e);
                }
                tracing::warn!(attempt, error = %e, "connection lost, reconnecting in 3s...");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

async fn connect_and_stream(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    // ... existing connect + receive + render logic
    // Returns Err when connection is lost
}
```

The existing frame receiver already returns an error when `accept_uni` or `read_datagram` fails due to connection close. This error propagates up and triggers the reconnect.

- [ ] **Step 2: Add heartbeat timeout on client side**

Track last received packet time. If no data for 10 seconds, abort the connection (returns Err, triggers reconnect):

```rust
// In the async receiver task:
let timeout = Duration::from_secs(10);
tokio::select! {
    result = connection.accept_uni() => { /* process frame */ }
    _ = tokio::time::sleep(timeout) => {
        tracing::warn!("no data received for 10s, disconnecting");
        return Err("heartbeat timeout".into());
    }
}
```

- [ ] **Step 3: Verify, commit**

```bash
cargo build -p prism-client
git commit -m "feat(client): disconnect detection with auto-reconnect (3s interval, 5min max)"
```

---

## Task 7: Graceful Shutdown

**Files:**
- Modify: `crates/prism-server/src/server_app.rs`

- [ ] **Step 1: Implement ServerApp::shutdown()**

The `main.rs` already has `tokio::select!` with `ctrl_c()` that calls `app.shutdown()`. Implement the method:

```rust
pub async fn shutdown(&mut self) {
    tracing::info!("initiating graceful shutdown...");

    self.shutdown.initiate("Server shutting down".into(), false);

    // Notify all connected clients
    if let Some(notice) = self.shutdown.notice() {
        let json = serde_json::to_vec(notice).unwrap_or_default();
        tracing::info!(clients = self.conn_store.client_count(), "sending shutdown notices");
        // Send via broadcast (best effort — clients may already be gone)
        let notice_datagram = build_shutdown_datagram(&json);
        self.conn_store.broadcast_datagram(&notice_datagram);
    }

    // Wait grace period (or until all clients disconnect)
    let grace = self.config.heartbeat_tombstone; // reuse as grace period
    tracing::info!(grace_secs = grace.as_secs(), "waiting for grace period...");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await; // short grace for now

    // Persist tombstones
    if let Ok(mut mgr) = self.session_manager.try_lock() {
        // Tombstones persist for restart recovery
        tracing::info!("shutdown complete");
    }

    self.acceptor.close();
}
```

- [ ] **Step 2: Build shutdown datagram helper**

```rust
fn build_shutdown_datagram(notice_json: &[u8]) -> bytes::Bytes {
    use prism_protocol::header::{PrismHeader, PROTOCOL_VERSION, HEADER_SIZE};
    use prism_protocol::channel::CHANNEL_CONTROL;
    use prism_session::control_msg;

    let header = PrismHeader {
        version: PROTOCOL_VERSION,
        channel_id: CHANNEL_CONTROL,
        msg_type: control_msg::SHUTDOWN_NOTICE,
        flags: 0, sequence: 0, timestamp_us: 0,
        payload_length: notice_json.len() as u32,
    };
    let mut buf = bytes::BytesMut::with_capacity(HEADER_SIZE + notice_json.len());
    header.encode(&mut buf);
    buf.extend_from_slice(notice_json);
    buf.freeze()
}
```

- [ ] **Step 3: Verify, commit**

```bash
cargo build -p prism-server
cargo test --workspace
git commit -m "feat(server): graceful shutdown with ShutdownNotice broadcast"
```

---

## Task 8: E2E Foundation Test

**Files:**
- Modify: `crates/prism-server/tests/e2e_frame_flow.rs`

- [ ] **Step 1: Add tests for new foundation features**

```rust
#[test]
fn heartbeat_generator_produces_valid_control_packet() {
    let gen = prism_server::HeartbeatGenerator::new();
    let packet = gen.packet();
    assert_eq!(packet.len(), prism_protocol::header::HEADER_SIZE);
    let header = prism_protocol::header::PrismHeader::decode_from_slice(&packet).unwrap();
    assert_eq!(header.channel_id, prism_protocol::channel::CHANNEL_CONTROL);
}

#[test]
fn shutdown_coordinator_lifecycle() {
    use prism_server::ShutdownCoordinator;
    use std::time::Duration;

    let mut coord = ShutdownCoordinator::new(Duration::from_secs(30));
    assert!(!coord.is_shutting_down());

    coord.initiate("test".into(), false);
    assert!(coord.is_shutting_down());
    assert!(coord.notice().is_some());
}
```

- [ ] **Step 2: Verify, commit**

```bash
cargo test -p prism-server --test e2e_frame_flow
git commit -m "test: foundation E2E tests for heartbeat + shutdown lifecycle"
```

---

## Summary

| Task | What | Tests |
|------|------|-------|
| 1 | Add tracing + dirs dependencies | 0 (build check) |
| 2 | ServerApp refactor (407→30 line main.rs) + tracing + Ctrl+C | 0 (build verify) |
| 3 | ClientApp refactor (451→30 line main.rs) + identity persistence | 0 (build verify) |
| 4 | Win32 SendInput FFI | 0 (existing tests pass) |
| 5 | ClientConnectionStore leak fix + heartbeat timeout task | 0 (build verify) |
| 6 | Client disconnect detection + auto-reconnect | 0 (build verify) |
| 7 | Graceful shutdown with ShutdownNotice | 0 (build verify) |
| 8 | E2E foundation tests | 2 |
| **Total** | | **~2** (plus all existing 561 tests must pass) |

**Note:** This plan is mostly refactoring and wiring — test count doesn't grow much, but the code quality and reliability improve dramatically. After this:
- `main.rs` files are 30 lines each (down from 407+451)
- All logging is structured via tracing
- Input actually controls the PC (SendInput FFI)
- Sessions timeout properly (heartbeat → tombstone)
- Client reconnects automatically
- Server shuts down cleanly on Ctrl+C
- Client identity persists across restarts

**Plan 12 (next):** Quality feedback loop + WASAPI audio + clipboard streaming (A9-A13)
**Plan 13 (after):** Remaining wiring + config file (A14-A19)
