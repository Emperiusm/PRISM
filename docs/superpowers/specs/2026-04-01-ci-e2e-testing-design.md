# CI + E2E Testing Design

**Date:** 2026-04-01
**Scope:** GitHub Actions CI pipeline + expanded E2E integration tests
**Status:** Approved

---

## 1. CI Pipeline

### Workflow: `.github/workflows/ci.yml`

Single job on `windows-latest` (public repo, unlimited minutes). Triggers on push to `main` and all PRs.

```yaml
Steps:
  1. Checkout
  2. Install Rust stable (with rustfmt + clippy)
  3. Cache cargo registry + target dir
  4. cargo fmt --all -- --check
  5. cargo clippy --workspace -- -D warnings
  6. cargo test --workspace
```

**Design decisions:**
- Single job, not split — Windows runner boot time (~45s) dominates, so parallelizing into separate jobs wastes more time than it saves.
- Windows runner required because `prism-platform-windows` only compiles on Windows.
- Rust `stable` toolchain, pinned via `dtolnay/rust-toolchain`.
- Cargo caching via `Swatinem/rust-cache` to avoid rebuilding 30+ dependencies on every run.

---

## 2. E2E Test Crate

### Structure

```
crates/prism-tests/
├── Cargo.toml
├── src/
│   └── lib.rs              # empty (required by Cargo)
└── tests/
    ├── harness/
    │   └── mod.rs           # TestServer + TestClient + helpers
    ├── session_lifecycle.rs  # 5 tests
    └── resilience.rs         # 4 tests
```

### Dependencies

```toml
[dev-dependencies]
prism-server = { path = "../prism-server" }
prism-client = { path = "../prism-client" }
prism-protocol = { path = "../prism-protocol" }
prism-security = { path = "../prism-security" }
prism-transport = { path = "../prism-transport" }
tokio = { version = "1", features = ["full", "test-util"] }
quinn = "0.11"
rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }
bytes = "1"
hex = "0.4"
```

---

## 3. Test Harness (`harness/mod.rs`)

### `TestServer`

- Wraps `ServerApp::with_config()` with test-friendly defaults:
  - `listen_addr_str = "127.0.0.1:0"` (random port)
  - `heartbeat_suspend_secs = 2` (fast timeout for tests)
  - `heartbeat_tombstone_secs = 5`
  - `tombstone_max_age_secs = 10`
  - `max_clients = 4`
- Test pattern mode (no DDA) — headless, no GPU required
- Runs `ServerApp::run()` in a `tokio::spawn` background task
- Exposes:
  - `addr() -> SocketAddr` — for clients to connect
  - `shutdown()` — clean stop

**Problem:** `ServerApp::run()` binds its own QUIC endpoint internally using `config.listen_addr()`. The test harness needs the actual bound address (with resolved `:0` port). Current implementation doesn't expose this.

**Solution:** After `ServerApp::run()` binds, the actual address is logged but not returned. The harness will need to either:
  - (a) Accept a pre-bound `quinn::Endpoint` (requires refactoring `ServerApp`), or
  - (b) Use a known port (e.g., `127.0.0.1:17000 + random offset`) — fragile but no refactor needed, or
  - (c) Add a method `ServerApp::local_addr() -> Option<SocketAddr>` that exposes the bound address after `run()` starts — minimal refactor.

**Recommendation:** Option (c) — store the bound `SocketAddr` in an `Arc<OnceCell>` that `run()` fills after bind. `TestServer` polls it briefly after spawning. Minimal change to production code.

### `TestClient`

- Wraps a raw `quinn::Endpoint` configured to trust the server's self-signed cert
- **Not** a full `ClientApp` — no minifb window, no render loop (headless)
- Connects to `TestServer::addr()`
- Exposes:
  - `connection() -> &quinn::Connection`
  - `send_datagram(Bytes)`
  - `recv_datagram() -> Bytes` (with timeout)
  - `open_bi() -> (SendStream, RecvStream)`
  - `close()` — clean disconnect
  - `drop_abruptly()` — simulate crash (drops endpoint without close)

**Challenge:** `TestClient` needs to trust the server's self-signed cert. The server generates its cert internally in `ServerApp::run()`.

**Solution:** The harness generates a `SelfSignedCert` externally, passes it into `ServerApp` (requires a `with_cert` parameter or a cert field on `ServerConfig`), and reuses the same cert DER to build the client trust store. This follows the pattern already used in `e2e_frame_flow.rs`.

**Alternative:** Add a `ServerApp::cert_der() -> CertificateDer` accessor. But this requires the cert to be stored on `ServerApp`, which currently creates it transiently.

**Recommendation:** Store the `SelfSignedCert`'s DER on `ServerApp` as a field set during construction. Expose via `cert_der()`. The test harness reads it before spawning the client.

### Helper: `timeout_secs(n, future)`

Wraps `tokio::time::timeout(Duration::from_secs(n), future)` with a panic message including the test name. Default 5 seconds.

---

## 4. Session Lifecycle Tests

### 4.1 `connect_and_receive_frames`
- Start TestServer (test pattern)
- TestClient connects, reads datagrams for 1 second
- Assert: at least 1 display datagram with valid DISPLAY channel header
- **Validates:** QUIC bind, accept, frame sender task delivers frames

### 4.2 `input_round_trip`
- Connect client, build InputEvent::KeyDown datagram, send to server
- Assert: server receives the datagram (verified from server-side connection)
- **Validates:** bidirectional QUIC, input channel routing

### 4.3 `noise_encrypted_session`
- Start TestServer with noise_mode = true
- TestClient performs Noise IK handshake over bi-stream
- Assert: handshake completes, frames flow post-handshake
- **Validates:** encryption doesn't break frame pipeline

### 4.4 `heartbeat_keeps_session_alive`
- Connect client, send periodic heartbeat control packets
- Wait longer than heartbeat_suspend_secs (2s in test config)
- Assert: connection stays open, frames still arriving
- **Validates:** heartbeat prevents premature teardown

### 4.5 `graceful_disconnect`
- Connect client, receive frames, close QUIC connection cleanly
- Assert: server doesn't panic, accepts new connections afterward
- **Validates:** clean departure doesn't crash server

---

## 5. Resilience Tests

### 5.1 `client_abrupt_disconnect_server_survives`
- Connect client, receive frames, drop endpoint without closing (simulate crash)
- Assert: server detects within heartbeat_suspend_secs, continues running, accepts new client
- **Validates:** abrupt loss doesn't poison server state

### 5.2 `reconnect_after_disconnect`
- Connect client A, receive frames, disconnect
- Connect client B to same server
- Assert: client B receives frames normally
- **Validates:** session cleanup allows new sessions

### 5.3 `multiple_sequential_clients`
- Connect and disconnect 5 clients in sequence
- Assert: all 5 connect and receive at least one frame, server never crashes
- **Validates:** no resource leaks across repeated sessions

### 5.4 `server_shutdown_during_active_session`
- Connect client, start receiving frames
- Call server.shutdown() while client active
- Assert: client gets connection error (not hang), no panic
- **Validates:** graceful shutdown propagates to clients

---

## 6. Required Production Code Changes

Minimal changes to support the test harness:

1. **`ServerApp` — store cert DER as field**
   - Store `cert_der: CertificateDer` on `ServerApp` during construction
   - Add `pub fn cert_der(&self) -> &CertificateDer` accessor
   - Tests use this to build client trust stores

2. **`ServerApp` — expose bound address**
   - Add `local_addr: Arc<tokio::sync::OnceCell<SocketAddr>>` field
   - `run()` fills it after `ConnectionAcceptor::bind()`
   - Add `pub fn local_addr(&self) -> Option<SocketAddr>` accessor
   - Tests poll this after spawning to learn the actual port

3. **`ServerApp` — shutdown handle**
   - `run()` currently blocks. Need a way to stop it from another task.
   - Add `shutdown_tx: tokio::sync::watch::Sender<bool>` that `run()` selects on
   - `pub fn shutdown_handle() -> ShutdownHandle` returns a cloneable trigger

These are small, non-breaking additions to existing structs.

---

## 7. Test Execution

All E2E tests run as part of `cargo test --workspace` — no special invocation needed. CI runs them automatically.

Expected total: **648 existing + 9 new = 657 tests**.

Test timeout: 5 seconds per test (via harness helper). Total CI time estimate: ~3-4 minutes (build + test on Windows runner with caching).
