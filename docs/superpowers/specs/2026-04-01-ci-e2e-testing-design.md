# CI + E2E Testing Design

**Date:** 2026-04-01
**Scope:** GitHub Actions CI pipeline + expanded E2E integration tests
**Status:** Approved

---

## 1. CI Pipeline

### Workflow: `.github/workflows/ci.yml`

Single job on `windows-latest` (public repo, unlimited minutes). Triggers on push to `main` and all PRs.

```yaml
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

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
- Concurrency group cancels in-progress runs on the same branch when a new push arrives — avoids wasting minutes on stale commits.

### Post-setup: Branch Protection

After CI is live, enable in GitHub repo Settings > Branches > Branch protection rules for `main`:
- Require status checks to pass before merging
- Required check: the CI job name
- This prevents merging PRs that fail fmt, clippy, or tests.

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

**Address notification:** `ServerApp::run()` binds to `:0` and resolves to a real port. The test harness needs this address before connecting clients. Solution: `ServerApp` gets a `bound_addr_tx: Option<tokio::sync::oneshot::Sender<SocketAddr>>` field. `run()` sends the bound address through it immediately after `ConnectionAcceptor::bind()`. `TestServer::start()` awaits the oneshot receiver — deterministic, no polling, no race condition.

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

**Cert trust:** The server currently generates its `SelfSignedCert` transiently in `run()`. To let the test client trust it, move cert generation into `with_config()` and store the `CertificateDer` as a field on `ServerApp`. Expose via `pub fn cert_der(&self) -> CertificateDer`. `run()` clones the stored cert instead of generating a new one. The test harness reads `cert_der()` before spawning the client to build its trust store. This also fixes the current bug where `run()` generates two independent certs (one in `new()` that's unused, one in `run()` that's actually bound).

### Helper: `timeout_secs(n, future)`

Wraps `tokio::time::timeout(Duration::from_secs(n), future)` with a descriptive panic message. Default 10 seconds (generous for CI runners under load).

### Flaky test prevention

- All tests use `#[tokio::test(flavor = "multi_thread")]` to avoid single-thread starvation where server and client tasks compete for the same thread.
- Timeouts are 10 seconds (not 5) — CI Windows runners can be slow under load.
- No retry loops — if a test flakes, the test is broken and must be fixed.
- Each test gets its own `TestServer` on a unique random port — full isolation, no shared state between tests.

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

1. **`ServerApp` — move cert generation to construction, store as field**
   - Generate `SelfSignedCert` in `with_config()` instead of `run()`
   - Store `cert_der: CertificateDer` on `ServerApp`
   - `run()` clones the stored cert instead of generating a fresh one
   - Add `pub fn cert_der(&self) -> CertificateDer` accessor
   - Tests use this to build client trust stores
   - Fixes existing inconsistency where two independent certs are generated

2. **`ServerApp` — bound address notification via oneshot**
   - Add `bound_addr_tx: Option<oneshot::Sender<SocketAddr>>` field
   - `with_config()` accepts an optional sender (default `None` for production)
   - `run()` sends the actual bound address through the channel after `ConnectionAcceptor::bind()`
   - Test harness awaits the receiver — deterministic, no polling, no race
   - Add convenience constructor: `with_config_and_addr_notify(...)` for tests

3. **`ServerApp` — shutdown handle**
   - `run()` currently blocks. Need a way to stop it from another task.
   - Add `shutdown_tx: tokio::sync::watch::Sender<bool>` that `run()` selects on
   - `pub fn shutdown_handle() -> ShutdownHandle` returns a cloneable trigger

These are small, non-breaking additions. Production callers are unaffected (all new fields have defaults or are optional).

---

## 7. Test Execution

All E2E tests run as part of `cargo test --workspace` — no special invocation needed. CI runs them automatically.

Expected total: **648 existing + 9 new = 657 tests**.

Test timeout: 10 seconds per test (via harness helper). Total CI time estimate: ~3-4 minutes (build + test on Windows runner with caching).
