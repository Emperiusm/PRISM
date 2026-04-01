# Plan 8: Server Binary + Test Pattern Generator

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a runnable PRISM server binary that accepts QUIC connections, authenticates clients, creates sessions, runs a per-client datagram receive loop, and generates test pattern frames — proving the full pipeline end-to-end without platform-specific capture or encoding.

**Architecture:** The binary lives in `prism-server` as `src/main.rs`. A `TestPatternCapture` implements `PlatformCapture` to generate synthetic frames (gradient patterns with a moving rectangle). `ConnectionAcceptor` wraps `quinn::Endpoint` setup with self-signed TLS. `LiveRecvLoop` reads datagrams/streams from a `QuicConnection` and dispatches via `classify_datagram` + `ChannelDispatcher`. For development, an `AllowAllGate` bypasses Noise handshake — clients authenticate via TLS only. The full Noise IK handshake is wired in a later plan.

**Tech Stack:** `quinn`, `tokio`, `rcgen` (self-signed TLS), `rustls`, all PRISM crates

---

## File Structure

```
PRISM/
  crates/
    prism-server/
      Cargo.toml                    # add rcgen + rustls to [dependencies] (not just dev)
      src/
        main.rs                     # Binary entry point
        acceptor.rs                 # ConnectionAcceptor (QUIC endpoint setup)
        recv_loop.rs                # Add LiveRecvLoop (async task per client)
        test_pattern.rs             # TestPatternCapture (PlatformCapture impl)
        allow_all_gate.rs           # AllowAllGate (dev-mode SecurityGate)
        lib.rs                      # Add new modules
```

---

## Task 1: TestPatternCapture

**Files:**
- Create: `crates/prism-server/src/test_pattern.rs`
- Modify: `crates/prism-server/src/lib.rs`

A `PlatformCapture` implementation that generates synthetic frames — no Windows, no GPU, no FFI.

- [ ] **Step 1: Write tests**

```rust
use prism_display::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_returns_single_virtual_monitor() {
        let cap = TestPatternCapture::new(1920, 1080, 60);
        let monitors = cap.enumerate_monitors().unwrap();
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].resolution, (1920, 1080));
        assert!(monitors[0].is_virtual);
        assert!(monitors[0].primary);
    }

    #[test]
    fn next_frame_returns_frame_with_correct_dimensions() {
        let mut cap = TestPatternCapture::new(1920, 1080, 60);
        cap.start(CaptureConfig {
            display_id: DisplayId(0),
            capture_mode: CaptureMode::Virtual { resolution: (1920, 1080), refresh_rate: 60 },
            cursor: CursorCapture::None,
        }).unwrap();
        let frame = cap.next_frame().unwrap().unwrap();
        assert_eq!(frame.texture.width, 1920);
        assert_eq!(frame.texture.height, 1080);
        assert_eq!(frame.display_id, DisplayId(0));
    }

    #[test]
    fn frames_have_incrementing_sequence() {
        let mut cap = TestPatternCapture::new(640, 480, 30);
        cap.start(CaptureConfig {
            display_id: DisplayId(0),
            capture_mode: CaptureMode::Virtual { resolution: (640, 480), refresh_rate: 30 },
            cursor: CursorCapture::None,
        }).unwrap();
        let f1 = cap.next_frame().unwrap().unwrap();
        let f2 = cap.next_frame().unwrap().unwrap();
        let f3 = cap.next_frame().unwrap().unwrap();
        assert_eq!(f1.frame_seq, 0);
        assert_eq!(f2.frame_seq, 1);
        assert_eq!(f3.frame_seq, 2);
    }

    #[test]
    fn stopped_capture_returns_none() {
        let mut cap = TestPatternCapture::new(640, 480, 30);
        // Not started → returns None
        let frame = cap.next_frame().unwrap();
        assert!(frame.is_none());
    }

    #[test]
    fn start_stop_lifecycle() {
        let mut cap = TestPatternCapture::new(640, 480, 30);
        cap.start(CaptureConfig {
            display_id: DisplayId(0),
            capture_mode: CaptureMode::Virtual { resolution: (640, 480), refresh_rate: 30 },
            cursor: CursorCapture::None,
        }).unwrap();
        assert!(cap.next_frame().unwrap().is_some());
        cap.stop();
        assert!(cap.next_frame().unwrap().is_none());
    }

    #[test]
    fn pattern_data_is_non_empty() {
        let cap = TestPatternCapture::new(100, 100, 60);
        let data = cap.generate_pattern(0);
        assert_eq!(data.len(), 100 * 100 * 4); // BGRA8
    }

    #[test]
    fn pattern_changes_each_frame() {
        let cap = TestPatternCapture::new(100, 100, 60);
        let d1 = cap.generate_pattern(0);
        let d2 = cap.generate_pattern(1);
        assert_ne!(d1, d2); // moving rectangle changes pattern
    }
}
```

- [ ] **Step 2: Implement TestPatternCapture**

```rust
use std::time::Instant;
use prism_display::*;

/// Test pattern generator implementing PlatformCapture.
/// Generates BGRA8 gradient frames with a moving white rectangle.
/// No GPU, no platform dependencies — works everywhere.
pub struct TestPatternCapture {
    width: u32,
    height: u32,
    fps: u8,
    running: bool,
    frame_seq: u32,
    start_time: Option<Instant>,
}

impl TestPatternCapture {
    pub fn new(width: u32, height: u32, fps: u8) -> Self {
        Self { width, height, fps, running: false, frame_seq: 0, start_time: None }
    }

    /// Generate a BGRA8 pattern for a given frame number.
    /// Gradient background + moving white rectangle.
    pub fn generate_pattern(&self, frame_num: u32) -> Vec<u8> {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut data = vec![0u8; w * h * 4];

        // Gradient background
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 4;
                data[idx] = (x * 255 / w.max(1)) as u8;     // B
                data[idx + 1] = (y * 255 / h.max(1)) as u8;  // G
                data[idx + 2] = ((x + y) * 128 / (w + h).max(1)) as u8; // R
                data[idx + 3] = 255;                           // A
            }
        }

        // Moving white rectangle (100x100, bounces horizontally)
        let rect_w = 100.min(w);
        let rect_h = 100.min(h);
        let cycle = (w - rect_w).max(1);
        let rect_x = (frame_num as usize * 3) % (cycle * 2);
        let rect_x = if rect_x >= cycle { cycle * 2 - rect_x } else { rect_x };
        let rect_y = (h - rect_h) / 2;

        for y in rect_y..(rect_y + rect_h).min(h) {
            for x in rect_x..(rect_x + rect_w).min(w) {
                let idx = (y * w + x) * 4;
                data[idx] = 255;     // B
                data[idx + 1] = 255; // G
                data[idx + 2] = 255; // R
                data[idx + 3] = 255; // A
            }
        }

        data
    }
}

impl PlatformCapture for TestPatternCapture {
    fn start(&mut self, _config: CaptureConfig) -> Result<(), CaptureError> {
        self.running = true;
        self.frame_seq = 0;
        self.start_time = Some(Instant::now());
        Ok(())
    }

    fn stop(&mut self) {
        self.running = false;
        self.start_time = None;
    }

    fn trigger_capture(&mut self) -> Result<(), CaptureError> {
        Ok(()) // test pattern doesn't need triggers
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
        if !self.running { return Ok(None); }

        let seq = self.frame_seq;
        self.frame_seq += 1;

        let capture_time_us = self.start_time
            .map(|s| s.elapsed().as_micros() as u64)
            .unwrap_or(0);

        // In a real implementation, texture.handle would be a GPU handle.
        // For test patterns, we store the frame_seq as a pseudo-handle
        // and generate pixel data on demand.
        Ok(Some(CapturedFrame {
            texture: SharedTexture {
                handle: seq as u64,
                width: self.width,
                height: self.height,
                format: TextureFormat::Bgra8,
            },
            damage_rects: vec![Rect { x: 0, y: 0, w: self.width, h: self.height }],
            display_id: DisplayId(0),
            capture_time_us,
            frame_seq: seq,
            is_input_triggered: false,
            is_speculative: false,
        }))
    }

    fn enumerate_monitors(&self) -> Result<Vec<MonitorInfo>, CaptureError> {
        Ok(vec![MonitorInfo {
            display_id: DisplayId(0),
            name: "Test Pattern".to_string(),
            resolution: (self.width, self.height),
            position: (0, 0),
            scale_factor: 1.0,
            refresh_rate: self.fps,
            primary: true,
            is_virtual: true,
        }])
    }

    fn create_virtual_display(&mut self, _config: DisplayConfig) -> Result<DisplayId, CaptureError> {
        Ok(DisplayId(0))
    }

    fn destroy_virtual_display(&mut self, _id: DisplayId) -> Result<(), CaptureError> {
        Ok(())
    }
}
```

- [ ] **Step 3: Update lib.rs**

Add `pub mod test_pattern;` and `pub use test_pattern::TestPatternCapture;`

- [ ] **Step 4: Verify, commit**

```bash
cargo test -p prism-server
git add crates/prism-server/src/test_pattern.rs crates/prism-server/src/lib.rs
git commit -m "feat(server): TestPatternCapture implementing PlatformCapture with gradient + moving rect"
```

---

## Task 2: ConnectionAcceptor (QUIC Endpoint Setup)

**Files:**
- Modify: `crates/prism-server/Cargo.toml` (move rcgen + rustls to [dependencies])
- Modify: `crates/prism-server/src/acceptor.rs`

- [ ] **Step 1: Update Cargo.toml**

Move `rcgen` and `rustls` from `[dev-dependencies]` to `[dependencies]`:

```toml
[dependencies]
# ... existing deps unchanged, add:
rcgen = { workspace = true }
rustls = { workspace = true }
```

Remove them from `[dev-dependencies]` if present.

- [ ] **Step 2: Write tests + implement ConnectionAcceptor**

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use quinn::Endpoint;
use prism_transport::quic::config::latency_transport_config;

/// Self-signed TLS certificate + key for development.
pub struct SelfSignedCert {
    pub cert_der: rustls::pki_types::CertificateDer<'static>,
    pub key_der: rustls::pki_types::PrivatePkcs8KeyDer<'static>,
}

impl SelfSignedCert {
    /// Generate a self-signed certificate for "localhost".
    pub fn generate() -> Result<Self, Box<dyn std::error::Error>> {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
        Ok(Self { cert_der, key_der })
    }
}

/// Sets up a QUIC server endpoint with TLS.
pub struct ConnectionAcceptor {
    endpoint: Endpoint,
    local_addr: SocketAddr,
}

impl ConnectionAcceptor {
    /// Create a QUIC server endpoint bound to the given address.
    pub fn bind(
        addr: SocketAddr,
        tls_cert: &SelfSignedCert,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut server_config = quinn::ServerConfig::with_single_cert(
            vec![tls_cert.cert_der.clone()],
            tls_cert.key_der.clone_key().into(),
        )?;
        server_config.transport_config(Arc::new(latency_transport_config(None)));

        let endpoint = Endpoint::server(server_config, addr)?;
        let local_addr = endpoint.local_addr()?;

        Ok(Self { endpoint, local_addr })
    }

    /// The actual address the server is listening on (useful when binding to port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Accept the next incoming connection.
    pub async fn accept(&self) -> Option<quinn::Incoming> {
        self.endpoint.accept().await
    }

    /// Close the endpoint.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }

    /// Get a reference to the underlying endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_signed_cert_generates() {
        let cert = SelfSignedCert::generate().unwrap();
        assert!(!cert.cert_der.is_empty());
    }

    #[tokio::test]
    async fn acceptor_binds_to_port() {
        let cert = SelfSignedCert::generate().unwrap();
        let acceptor = ConnectionAcceptor::bind(
            "127.0.0.1:0".parse().unwrap(),
            &cert,
        ).unwrap();
        assert!(acceptor.local_addr().port() > 0);
        acceptor.close();
    }

    #[tokio::test]
    async fn acceptor_accepts_connection() {
        let cert = SelfSignedCert::generate().unwrap();
        let acceptor = ConnectionAcceptor::bind(
            "127.0.0.1:0".parse().unwrap(),
            &cert,
        ).unwrap();
        let server_addr = acceptor.local_addr();

        // Create a client that connects
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert.cert_der.clone()).unwrap();
        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).unwrap(),
        ));

        let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);

        // Connect client to server
        let connecting = client_endpoint.connect(server_addr, "localhost").unwrap();

        // Server accepts
        let incoming = acceptor.accept().await.unwrap();
        let server_conn = incoming.await.unwrap();
        let client_conn = connecting.await.unwrap();

        // Both sides connected
        assert_eq!(server_conn.remote_address(), client_endpoint.local_addr().unwrap());
        assert_eq!(client_conn.remote_address(), server_addr);

        acceptor.close();
        client_endpoint.close(0u32.into(), b"done");
    }
}
```

- [ ] **Step 3: Update lib.rs**

Add `pub mod acceptor;` and:
```rust
pub use acceptor::{ConnectionAcceptor, SelfSignedCert};
```

- [ ] **Step 4: Verify, commit**

```bash
cargo test -p prism-server
git add crates/prism-server/
git commit -m "feat(server): ConnectionAcceptor with self-signed TLS + QUIC endpoint"
```

---

## Task 3: AllowAllGate (Dev-Mode Auth)

**Files:**
- Create: `crates/prism-server/src/allow_all_gate.rs`
- Modify: `crates/prism-server/src/lib.rs`

Development-mode SecurityGate that authenticates every client without Noise handshake. Useful for testing the pipeline before Noise integration.

- [ ] **Step 1: Write tests + implement**

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use uuid::Uuid;
use prism_security::gate::{AuthResult, SecurityGate};
use prism_security::context::{ChannelDecision, SecurityContext};
use prism_security::identity::DeviceIdentity;
use prism_security::audit::AuditEvent;
use prism_security::pairing::{PairingEntry, PairingState, ChannelPermissions, Permission};

/// Development-mode gate that authenticates ALL clients.
/// Skips Noise handshake — clients connect via QUIC TLS only.
/// NOT for production use.
pub struct AllowAllGate {
    auth_count: AtomicU32,
}

impl AllowAllGate {
    pub fn new() -> Self {
        Self { auth_count: AtomicU32::new(0) }
    }

    pub fn auth_count(&self) -> u32 {
        self.auth_count.load(Ordering::Relaxed)
    }
}

impl Default for AllowAllGate {
    fn default() -> Self { Self::new() }
}

impl SecurityGate for AllowAllGate {
    fn authenticate(&self, client_key: &[u8; 32], device_identity: &DeviceIdentity) -> AuthResult {
        self.auth_count.fetch_add(1, Ordering::Relaxed);

        let pairing_entry = PairingEntry {
            device: device_identity.clone(),
            state: PairingState::Paired,
            permissions: ChannelPermissions::allow_all(),
            paired_at: 0,
            last_seen: 0,
        };

        let mut channel_decisions = [ChannelDecision::AllowAll; 256];
        let ctx = SecurityContext {
            device: Arc::new(pairing_entry),
            channel_decisions,
            active_filters: std::collections::HashMap::new(),
            is_0rtt_safe: [true; 256],
        };

        AuthResult::Authenticated(Arc::new(ctx))
    }

    fn security_context(&self, _device_id: &Uuid) -> Option<Arc<SecurityContext>> {
        None // AllowAllGate doesn't cache contexts
    }

    fn audit(&self, _event: AuditEvent) {
        // No-op in dev mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_security::identity::Platform;

    fn test_device() -> DeviceIdentity {
        DeviceIdentity {
            device_id: Uuid::from_bytes([1; 16]),
            display_name: "Test Device".to_string(),
            platform: Platform::Windows,
            current_key: [42u8; 32],
            signing_key: [43u8; 32],
            created_at: 0,
        }
    }

    #[test]
    fn authenticates_any_client() {
        let gate = AllowAllGate::new();
        let device = test_device();
        let result = gate.authenticate(&[0u8; 32], &device);
        assert!(matches!(result, AuthResult::Authenticated(_)));
    }

    #[test]
    fn counts_authentications() {
        let gate = AllowAllGate::new();
        let device = test_device();
        gate.authenticate(&[0u8; 32], &device);
        gate.authenticate(&[1u8; 32], &device);
        assert_eq!(gate.auth_count(), 2);
    }

    #[test]
    fn context_has_allow_all_decisions() {
        let gate = AllowAllGate::new();
        let device = test_device();
        if let AuthResult::Authenticated(ctx) = gate.authenticate(&[0u8; 32], &device) {
            assert!(matches!(ctx.channel_decisions[1], ChannelDecision::AllowAll));
            assert!(matches!(ctx.channel_decisions[100], ChannelDecision::AllowAll));
        } else {
            panic!("expected Authenticated");
        }
    }

    #[test]
    fn audit_is_noop() {
        let gate = AllowAllGate::new();
        // Should not panic
        gate.audit(AuditEvent::ClientAuthenticated {
            device_id: Uuid::from_bytes([1; 16]),
            device_name: "test".into(),
        });
    }
}
```

**Note:** This task depends on the exact fields of `PairingEntry`, `ChannelPermissions`, `SecurityContext`, `ChannelDecision`, `DeviceIdentity`, `AuditEvent`. The implementer MUST read the actual source files to get the exact struct fields and constructors. The code above is the intent — adapt field names to match what actually exists in prism-security.

- [ ] **Step 2: Update lib.rs**

Add `pub mod allow_all_gate;` and `pub use allow_all_gate::AllowAllGate;`

- [ ] **Step 3: Verify, commit**

```bash
cargo test -p prism-server
git add crates/prism-server/src/allow_all_gate.rs crates/prism-server/src/lib.rs
git commit -m "feat(server): AllowAllGate dev-mode SecurityGate for testing"
```

---

## Task 4: LiveRecvLoop

**Files:**
- Modify: `crates/prism-server/src/recv_loop.rs`

Add the async per-client receive loop that reads from a `PrismConnection` and dispatches.

- [ ] **Step 1: Write tests + implement LiveRecvLoop**

Add to the existing `recv_loop.rs` (which already has `classify_datagram` and `record_datagram_bandwidth`):

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use prism_transport::{PrismConnection, TransportError};
use prism_session::{ClientId, ChannelDispatcher, ChannelBandwidthTracker};
use prism_transport::quality::prober::ProbeEcho;

/// Per-client receive loop handle. Spawns as a tokio task.
pub struct RecvLoopHandle {
    cancel_tx: mpsc::Sender<()>,
}

impl RecvLoopHandle {
    /// Signal the recv loop to stop.
    pub async fn stop(&self) {
        let _ = self.cancel_tx.send(()).await;
    }
}

/// Spawn a per-client datagram receive loop.
/// Returns a handle that can be used to stop the loop.
pub fn spawn_recv_loop(
    client_id: ClientId,
    connection: Arc<dyn PrismConnection>,
    dispatcher: Arc<ChannelDispatcher>,
    tracker: Arc<ChannelBandwidthTracker>,
    activity_tx: mpsc::Sender<ClientId>,
) -> RecvLoopHandle {
    let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                result = connection.recv_datagram() => {
                    match result {
                        Ok(data) => {
                            // Signal activity (heartbeat)
                            let _ = activity_tx.try_send(client_id);

                            match classify_datagram(&data) {
                                DatagramAction::ProbeResponse => {
                                    // Probe handling would go here
                                }
                                DatagramAction::ChannelDispatch { channel_id } => {
                                    // Record bandwidth
                                    if let Ok(header) = prism_protocol::header::PrismHeader::decode_from_slice(&data) {
                                        record_datagram_bandwidth(&tracker, &header);
                                    }
                                    // Dispatch to channel handler
                                    dispatcher.dispatch(client_id, channel_id, data).await;
                                }
                                DatagramAction::Drop => {}
                            }
                        }
                        Err(TransportError::ConnectionClosed) => break,
                        Err(_) => continue,
                    }
                }
                _ = cancel_rx.recv() => break,
            }
        }
    });

    RecvLoopHandle { cancel_tx }
}

// Add tests to existing test module:
#[cfg(test)]
mod tests {
    // ... existing tests for classify_datagram and record_datagram_bandwidth ...

    #[tokio::test]
    async fn recv_loop_handle_stops() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ClientId>(16);
        let handle = RecvLoopHandle { cancel_tx: tokio::sync::mpsc::channel(1).0 };
        // Just verify the handle type works — actual recv loop needs a live connection
        handle.stop().await;
    }
}
```

- [ ] **Step 2: Update lib.rs re-exports**

Add: `pub use recv_loop::{RecvLoopHandle, spawn_recv_loop};`

- [ ] **Step 3: Verify, commit**

```bash
cargo test -p prism-server
git add crates/prism-server/src/recv_loop.rs crates/prism-server/src/lib.rs
git commit -m "feat(server): LiveRecvLoop async per-client datagram dispatch"
```

---

## Task 5: Server Binary (main.rs)

**Files:**
- Create: `crates/prism-server/src/main.rs`
- Modify: `crates/prism-server/Cargo.toml` (add [[bin]] section)

- [ ] **Step 1: Update Cargo.toml**

Add a `[[bin]]` section:

```toml
[[bin]]
name = "prism-server"
path = "src/main.rs"
```

- [ ] **Step 2: Implement main.rs**

```rust
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use prism_server::{
    ServerConfig, SessionManager, ConnectionAcceptor, SelfSignedCert,
    AllowAllGate, TestPatternCapture, spawn_recv_loop,
};
use prism_session::{
    ClientId, ConnectionProfile, ChannelDispatcher, ChannelBandwidthTracker,
};
use prism_transport::{QuicConnection, UnifiedConnection};
use prism_display::PlatformCapture;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PRISM Server v0.1.0 ===");

    // 1. Configuration
    let config = ServerConfig::default();
    println!("Listening on {}", config.listen_addr);

    // 2. Generate self-signed TLS certificate
    let cert = SelfSignedCert::generate()?;
    println!("Generated self-signed TLS certificate");

    // 3. Create security gate (dev mode — accepts all clients)
    let gate = Arc::new(AllowAllGate::new());
    println!("Security: AllowAllGate (dev mode)");

    // 4. Initialize test pattern capture
    let mut capture = TestPatternCapture::new(1920, 1080, 60);
    let monitors = capture.enumerate_monitors()?;
    println!("Capture: TestPattern {}x{} @ {}fps",
        monitors[0].resolution.0, monitors[0].resolution.1, monitors[0].refresh_rate);

    // 5. Create session manager
    let session_manager = Arc::new(Mutex::new(SessionManager::new(config.clone())));

    // 6. Create shared dispatcher and bandwidth tracker
    let dispatcher = Arc::new(ChannelDispatcher::new());
    let tracker = Arc::new(ChannelBandwidthTracker::new());

    // 7. Bind QUIC endpoint
    let acceptor = ConnectionAcceptor::bind(config.listen_addr, &cert)?;
    println!("QUIC endpoint bound to {}", acceptor.local_addr());
    println!("Waiting for connections...\n");

    // 8. Activity channel (heartbeat signals from recv loops)
    let (activity_tx, mut activity_rx) = mpsc::channel::<ClientId>(256);

    // Spawn activity processor
    let sm_activity = session_manager.clone();
    tokio::spawn(async move {
        while let Some(client_id) = activity_rx.recv().await {
            sm_activity.lock().await.activity(client_id);
        }
    });

    // 9. Accept loop
    loop {
        let incoming = match acceptor.accept().await {
            Some(incoming) => incoming,
            None => {
                println!("Endpoint closed");
                break;
            }
        };

        let gate = gate.clone();
        let sm = session_manager.clone();
        let disp = dispatcher.clone();
        let track = tracker.clone();
        let act_tx = activity_tx.clone();

        tokio::spawn(async move {
            match incoming.await {
                Ok(quinn_conn) => {
                    let remote = quinn_conn.remote_address();
                    println!("[{}] Connected", remote);

                    // Wrap in QuicConnection → UnifiedConnection
                    let qc = QuicConnection::new(quinn_conn);
                    let unified = Arc::new(UnifiedConnection::new(Box::new(qc), None));

                    // Generate client ID
                    let client_id = Uuid::now_v7();
                    let device_id = Uuid::now_v7();

                    // Create session
                    let channels = {
                        let mut mgr = sm.lock().await;
                        mgr.new_session(
                            client_id,
                            device_id,
                            unified.clone(),
                            ConnectionProfile::coding(),
                            &[
                                prism_protocol::channel::CHANNEL_DISPLAY,
                                prism_protocol::channel::CHANNEL_INPUT,
                                prism_protocol::channel::CHANNEL_CONTROL,
                            ],
                        )
                    };

                    match channels {
                        Ok(granted) => {
                            println!("[{}] Session created: {} channels granted", remote, granted.len());

                            // Spawn recv loop
                            let _handle = spawn_recv_loop(
                                client_id,
                                unified.clone() as Arc<dyn prism_transport::PrismConnection>,
                                disp,
                                track,
                                act_tx,
                            );

                            println!("[{}] Recv loop started for client {}", remote, &client_id.to_string()[..8]);
                        }
                        Err(e) => {
                            println!("[{}] Session creation failed: {}", remote, e);
                        }
                    }
                }
                Err(e) => {
                    println!("Connection failed: {}", e);
                }
            }
        });
    }

    Ok(())
}
```

- [ ] **Step 3: Verify build**

```bash
cargo build -p prism-server
```

Expected: compiles successfully, produces `target/debug/prism-server` (or `.exe` on Windows).

- [ ] **Step 4: Verify all tests still pass**

```bash
cargo test --workspace
```

- [ ] **Step 5: Commit**

```bash
git add crates/prism-server/src/main.rs crates/prism-server/Cargo.toml
git commit -m "feat(server): runnable PRISM server binary with QUIC accept loop"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | TestPatternCapture (PlatformCapture impl) | 7 |
| 2 | ConnectionAcceptor (QUIC endpoint + TLS) | 3 |
| 3 | AllowAllGate (dev-mode SecurityGate) | 4 |
| 4 | LiveRecvLoop (async datagram dispatch) | 1 |
| 5 | Server binary (main.rs) | 0 (build verification) |
| **Total** | | **~15** |

**What this plan delivers:**
- `cargo run -p prism-server` — a running PRISM server
- Accepts QUIC connections with self-signed TLS
- Creates sessions with channel ownership + routing
- Spawns per-client datagram recv loops
- TestPatternCapture generates synthetic frames (gradient + moving rect)
- AllowAllGate bypasses auth for development

**What a connecting client would see:**
- QUIC TLS handshake succeeds
- Server assigns Display + Input + Control channels
- Datagram recv loop processes incoming data
- (Frame sending to client not in this plan — requires channel handler wiring)

**Next steps after this plan:**
- Channel handlers (Display, Control) that use TestPatternCapture to send frames to connected clients
- Client binary that connects, receives frames, decodes, renders
- Wire Noise IK handshake into the accept path (replacing AllowAllGate)
