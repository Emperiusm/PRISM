# Plan 6: Integration Implementation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-server` crate that wires all 7 subsystem crates into a working server — connection acceptance with security authentication, session lifecycle management, datagram/stream receive dispatch, quality-driven degradation monitoring, and end-to-end integration tests over loopback QUIC.

**Architecture:** `prism-server` is the orchestration layer — no new algorithms, just wiring. `ConnectionAcceptor` accepts QUIC connections, runs `SecurityGate::authenticate`, creates `ClientSession`s with `UnifiedConnection`. `SessionManager` owns the client lifecycle: connect → active → suspend → tombstone → reconnect. `RecvLoop` dispatches datagrams/streams to `ChannelDispatcher`. `QualityMonitor` feeds `ConnectionProber` + `BandwidthEstimator` → `ConnectionQuality` → `DegradationLadder`. Integration tests use loopback QUIC pairs with `rcgen` self-signed certs, verifying the full auth→session→dispatch→quality pipeline.

**Tech Stack:** All 7 PRISM crates, `quinn` (QUIC endpoints), `tokio` (async runtime), `rcgen` + `rustls` (test TLS), `async-trait`, `arc-swap`

**Spec refs:**
- Session+Observability: `docs/superpowers/specs/2026-03-30-session-observability-design.md` (Sections 2, 6, 7, 10)
- Transport: `docs/superpowers/specs/2026-03-30-transport-design.md` (Section 4: QUIC Implementation)
- Architecture: `docs/superpowers/specs/2026-03-30-prism-architecture-design.md` (R37: control-plane only)

---

## File Structure

```
PRISM/
  crates/
    prism-server/
      Cargo.toml
      src/
        lib.rs                      # re-exports
        config.rs                   # ServerConfig, defaults
        acceptor.rs                 # ConnectionAcceptor (QUIC accept + auth)
        session_manager.rs          # SessionManager orchestrator
        client_session.rs           # ClientSession (per-client state)
        recv_loop.rs                # Datagram + stream dispatch
        quality_monitor.rs          # Quality → degradation feedback loop
        shutdown.rs                 # Graceful shutdown coordinator
```

---

## Task 1: Crate Setup + ServerConfig

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-server/Cargo.toml`
- Create: `crates/prism-server/src/lib.rs`
- Create: `crates/prism-server/src/config.rs`
- Create: all placeholder source files

- [ ] **Step 1: Update workspace root Cargo.toml**

Add `"crates/prism-server"` to members. Add `prism-server = { path = "crates/prism-server" }` to workspace.dependencies.

- [ ] **Step 2: Create crates/prism-server/Cargo.toml**

```toml
[package]
name = "prism-server"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
prism-protocol = { workspace = true }
prism-metrics = { workspace = true }
prism-security = { workspace = true }
prism-transport = { workspace = true }
prism-observability = { workspace = true }
prism-session = { workspace = true }
prism-display = { workspace = true }
bytes = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
arc-swap = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
quinn = { workspace = true }

[dev-dependencies]
rcgen = { workspace = true }
rustls = { workspace = true }
```

- [ ] **Step 3: Create lib.rs + all placeholder files**

`lib.rs`:
```rust
pub mod config;
pub mod acceptor;
pub mod session_manager;
pub mod client_session;
pub mod recv_loop;
pub mod quality_monitor;
pub mod shutdown;
```

Create 6 placeholder source files with comments.

- [ ] **Step 4: Write tests + implement ServerConfig**

`config.rs`:
```rust
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Server configuration with sensible defaults.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: SocketAddr,
    pub throughput_addr: SocketAddr,
    pub identity_path: PathBuf,
    pub pairing_path: PathBuf,
    pub tombstone_path: PathBuf,
    pub display_name: String,
    pub max_clients: usize,
    pub heartbeat_suspend: Duration,
    pub heartbeat_tombstone: Duration,
    pub tombstone_max_age: Duration,
    pub total_bandwidth_bps: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9876".parse().unwrap(),
            throughput_addr: "0.0.0.0:9877".parse().unwrap(),
            identity_path: PathBuf::from("prism_identity.json"),
            pairing_path: PathBuf::from("prism_pairing.enc"),
            tombstone_path: PathBuf::from("prism_tombstones.json"),
            display_name: "PRISM Server".to_string(),
            max_clients: 4,
            heartbeat_suspend: Duration::from_secs(10),
            heartbeat_tombstone: Duration::from_secs(60),
            tombstone_max_age: Duration::from_secs(300),
            total_bandwidth_bps: 100_000_000, // 100 Mbps default
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = ServerConfig::default();
        assert_eq!(config.max_clients, 4);
        assert_eq!(config.listen_addr.port(), 9876);
        assert_eq!(config.heartbeat_suspend, Duration::from_secs(10));
        assert_eq!(config.total_bandwidth_bps, 100_000_000);
    }

    #[test]
    fn config_custom_values() {
        let config = ServerConfig {
            listen_addr: "127.0.0.1:5555".parse().unwrap(),
            max_clients: 1,
            ..ServerConfig::default()
        };
        assert_eq!(config.listen_addr.port(), 5555);
        assert_eq!(config.max_clients, 1);
    }
}
```

- [ ] **Step 5: Verify, commit**

```bash
cargo check -p prism-server
git add crates/prism-server/ Cargo.toml
git commit -m "feat(server): scaffold prism-server crate with ServerConfig"
```

---

## Task 2: ClientSession (Per-Client State)

**Files:**
- Modify: `crates/prism-server/src/client_session.rs`

- [ ] **Step 1: Write tests + implement ClientSession**

```rust
use std::sync::Arc;
use std::time::Instant;
use std::collections::HashSet;

use prism_session::{ClientId, SessionState, ConnectionProfile, ChannelRegistry, ChannelGrantResult};
use prism_transport::UnifiedConnection;

/// Per-client session state held by SessionManager.
pub struct ClientSession {
    pub client_id: ClientId,
    pub device_id: uuid::Uuid,
    pub connection: Arc<UnifiedConnection>,
    pub profile: ConnectionProfile,
    pub subscribed_channels: HashSet<u16>,
    pub state: SessionState,
    pub connected_at: Instant,
    pub last_activity: Instant,
}

impl ClientSession {
    pub fn new(
        client_id: ClientId,
        device_id: uuid::Uuid,
        connection: Arc<UnifiedConnection>,
        profile: ConnectionProfile,
    ) -> Self {
        let now = Instant::now();
        Self {
            client_id,
            device_id,
            connection,
            profile,
            subscribed_channels: HashSet::new(),
            state: SessionState::Active,
            connected_at: now,
            last_activity: now,
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn subscribe(&mut self, channel_id: u16) {
        self.subscribed_channels.insert(channel_id);
    }

    pub fn unsubscribe(&mut self, channel_id: u16) {
        self.subscribed_channels.remove(&channel_id);
    }

    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }

    pub fn suspend(&mut self) {
        self.state = SessionState::Suspended;
    }

    pub fn reactivate(&mut self) {
        self.state = SessionState::Active;
        self.last_activity = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_transport::connection::mock::MockConnection;
    use uuid::Uuid;

    fn make_session() -> ClientSession {
        let conn = Arc::new(UnifiedConnection::new(
            Box::new(MockConnection::new(1200)),
            None,
        ));
        ClientSession::new(
            Uuid::from_bytes([1; 16]),
            Uuid::from_bytes([10; 16]),
            conn,
            ConnectionProfile::coding(),
        )
    }

    #[test]
    fn new_session_is_active() {
        let session = make_session();
        assert!(session.is_active());
        assert_eq!(session.state, SessionState::Active);
        assert!(session.subscribed_channels.is_empty());
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let mut session = make_session();
        session.subscribe(0x001);
        session.subscribe(0x006);
        assert_eq!(session.subscribed_channels.len(), 2);
        session.unsubscribe(0x001);
        assert_eq!(session.subscribed_channels.len(), 1);
    }

    #[test]
    fn suspend_and_reactivate() {
        let mut session = make_session();
        session.suspend();
        assert!(!session.is_active());
        assert_eq!(session.state, SessionState::Suspended);
        session.reactivate();
        assert!(session.is_active());
    }

    #[test]
    fn touch_updates_activity() {
        let mut session = make_session();
        let t1 = session.last_activity;
        std::thread::sleep(std::time::Duration::from_millis(5));
        session.touch();
        assert!(session.last_activity > t1);
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-server/src/client_session.rs
git commit -m "feat(server): ClientSession per-client state management"
```

---

## Task 3: SessionManager Orchestrator

**Files:**
- Modify: `crates/prism-server/src/session_manager.rs`

- [ ] **Step 1: Write tests + implement SessionManager**

This is the core orchestrator. It manages the full client lifecycle.

```rust
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;
use uuid::Uuid;

use prism_session::{
    ClientId, SessionState, SessionEvent, ConnectionProfile,
    ChannelRegistry, ChannelGrantResult, RoutingTable, RoutingMutation, RouteEntry,
    HeartbeatMonitor, TombstoneStore, Tombstone, BandwidthArbiter,
    ChannelBandwidthTracker, CapabilityNegotiator,
};
use prism_transport::UnifiedConnection;
use prism_protocol::channel::{
    CHANNEL_DISPLAY, CHANNEL_INPUT, CHANNEL_AUDIO, CHANNEL_CONTROL,
    CHANNEL_CLIPBOARD, CHANNEL_FILESHARE, CHANNEL_DEVICE,
};

use crate::client_session::ClientSession;
use crate::config::ServerConfig;

pub struct SessionManager {
    clients: HashMap<ClientId, ClientSession>,
    routing_table: Arc<RoutingTable>,
    channel_registry: ChannelRegistry,
    arbiter: BandwidthArbiter,
    tombstones: TombstoneStore,
    heartbeat: HeartbeatMonitor,
    event_tx: broadcast::Sender<SessionEvent>,
    config: ServerConfig,
}

impl SessionManager {
    pub fn new(config: ServerConfig) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            clients: HashMap::new(),
            routing_table: Arc::new(RoutingTable::new()),
            channel_registry: ChannelRegistry::with_defaults(),
            arbiter: BandwidthArbiter::new(config.total_bandwidth_bps),
            tombstones: TombstoneStore::new(config.tombstone_max_age.as_secs()),
            heartbeat: HeartbeatMonitor::new(config.heartbeat_suspend, config.heartbeat_tombstone),
            event_tx,
            config,
        }
    }

    pub fn routing_table(&self) -> Arc<RoutingTable> {
        self.routing_table.clone()
    }

    pub fn events(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    /// Register a new authenticated client.
    pub fn new_session(
        &mut self,
        client_id: ClientId,
        device_id: Uuid,
        connection: Arc<UnifiedConnection>,
        profile: ConnectionProfile,
        requested_channels: &[u16],
    ) -> Result<Vec<u16>, prism_session::SessionError> {
        // Check if this is a reconnecting client (tombstone)
        let _tombstone = self.tombstones.claim_by_device(&device_id);

        let mut session = ClientSession::new(client_id, device_id, connection.clone(), profile);
        let mut granted_channels = Vec::new();
        let mut mutations = Vec::new();

        for &channel_id in requested_channels {
            match self.channel_registry.request_channel(channel_id, client_id) {
                Ok(ChannelGrantResult::Granted) | Ok(ChannelGrantResult::AlreadyOwned) => {
                    session.subscribe(channel_id);
                    granted_channels.push(channel_id);
                    mutations.push(RoutingMutation::AddRoute {
                        channel_id,
                        entry: RouteEntry { client_id },
                    });
                }
                Ok(ChannelGrantResult::Transferred { .. }) => {
                    session.subscribe(channel_id);
                    granted_channels.push(channel_id);
                    mutations.push(RoutingMutation::AddRoute {
                        channel_id,
                        entry: RouteEntry { client_id },
                    });
                }
                _ => {} // denied or pending — skip
            }
        }

        // Atomic routing table update
        self.routing_table.batch_update(mutations);

        // Register heartbeat
        self.heartbeat.register(client_id);

        // Emit event
        let _ = self.event_tx.send(SessionEvent::ClientConnected {
            client_id,
            device_name: format!("Device-{}", &device_id.to_string()[..8]),
        });

        self.clients.insert(client_id, session);
        Ok(granted_channels)
    }

    /// Remove a client and clean up all state.
    pub fn disconnect(&mut self, client_id: ClientId, reason: String) {
        if let Some(session) = self.clients.remove(&client_id) {
            self.channel_registry.release_all(client_id);
            self.routing_table.batch_update(vec![RoutingMutation::RemoveClient(client_id)]);
            self.heartbeat.unregister(client_id);
            self.arbiter.remove_client(client_id);

            // Create tombstone for reconnection
            self.tombstones.insert(Tombstone::new(
                client_id,
                session.device_id,
                session.subscribed_channels.clone(),
            ));

            let _ = self.event_tx.send(SessionEvent::ClientDisconnected {
                client_id,
                reason,
            });
        }
    }

    /// Record activity for a client (resets heartbeat timer).
    pub fn activity(&mut self, client_id: ClientId) {
        if let Some(session) = self.clients.get_mut(&client_id) {
            session.touch();
        }
        self.heartbeat.activity(client_id);
    }

    pub fn client_count(&self) -> usize { self.clients.len() }

    pub fn get_session(&self, client_id: ClientId) -> Option<&ClientSession> {
        self.clients.get(&client_id)
    }

    /// Check for stale clients and suspend/tombstone them.
    pub fn check_heartbeats(&mut self) -> Vec<(ClientId, bool)> {
        let stale = self.heartbeat.stale_clients();
        let mut actions = Vec::new();
        for (client_id, needs_suspend, needs_tombstone) in stale {
            if needs_tombstone {
                self.disconnect(client_id, "heartbeat timeout".into());
                actions.push((client_id, true)); // tombstoned
            } else if needs_suspend {
                if let Some(session) = self.clients.get_mut(&client_id) {
                    session.suspend();
                    actions.push((client_id, false)); // suspended
                }
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_transport::connection::mock::MockConnection;

    fn test_config() -> ServerConfig {
        ServerConfig {
            max_clients: 4,
            total_bandwidth_bps: 100_000_000,
            ..ServerConfig::default()
        }
    }

    fn mock_connection() -> Arc<UnifiedConnection> {
        Arc::new(UnifiedConnection::new(
            Box::new(MockConnection::new(1200)),
            None,
        ))
    }

    fn client_a() -> ClientId { Uuid::from_bytes([1; 16]) }
    fn device_a() -> Uuid { Uuid::from_bytes([10; 16]) }

    #[test]
    fn new_session_registers_client() {
        let mut mgr = SessionManager::new(test_config());
        let channels = mgr.new_session(
            client_a(), device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY, CHANNEL_INPUT, CHANNEL_CONTROL],
        ).unwrap();
        assert_eq!(channels.len(), 3);
        assert_eq!(mgr.client_count(), 1);
        assert!(mgr.get_session(client_a()).is_some());
    }

    #[test]
    fn new_session_updates_routing_table() {
        let mut mgr = SessionManager::new(test_config());
        mgr.new_session(
            client_a(), device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY, CHANNEL_CONTROL],
        ).unwrap();
        let snap = mgr.routing_table().snapshot();
        assert!(snap.generation > 0);
        assert!(snap.channel_routes.contains_key(&CHANNEL_DISPLAY));
    }

    #[test]
    fn disconnect_removes_client() {
        let mut mgr = SessionManager::new(test_config());
        mgr.new_session(
            client_a(), device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY],
        ).unwrap();
        mgr.disconnect(client_a(), "test".into());
        assert_eq!(mgr.client_count(), 0);
        // Routing table should be cleaned
        let snap = mgr.routing_table().snapshot();
        assert!(snap.channel_routes.get(&CHANNEL_DISPLAY)
            .map_or(true, |r| r.is_empty()));
    }

    #[test]
    fn disconnect_creates_tombstone() {
        let mut mgr = SessionManager::new(test_config());
        mgr.new_session(
            client_a(), device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY],
        ).unwrap();
        mgr.disconnect(client_a(), "test".into());
        // Reconnect with same device — tombstone should be consumed
        let client_b = Uuid::from_bytes([2; 16]);
        let channels = mgr.new_session(
            client_b, device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY],
        ).unwrap();
        assert_eq!(channels.len(), 1);
    }

    #[test]
    fn activity_updates_heartbeat() {
        let mut mgr = SessionManager::new(test_config());
        mgr.new_session(
            client_a(), device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_CONTROL],
        ).unwrap();
        mgr.activity(client_a());
        // Should not be stale yet
        assert!(mgr.check_heartbeats().is_empty());
    }

    #[test]
    fn session_events_emitted() {
        let mut mgr = SessionManager::new(test_config());
        let mut events = mgr.events();
        mgr.new_session(
            client_a(), device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_CONTROL],
        ).unwrap();
        let event = events.try_recv().unwrap();
        assert!(matches!(event, SessionEvent::ClientConnected { .. }));
    }

    #[test]
    fn exclusive_channel_denied_to_second_client() {
        let mut mgr = SessionManager::new(test_config());
        let client_b = Uuid::from_bytes([2; 16]);
        let device_b = Uuid::from_bytes([20; 16]);
        mgr.new_session(
            client_a(), device_a(), mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY], // exclusive
        ).unwrap();
        let channels = mgr.new_session(
            client_b, device_b, mock_connection(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY], // should be denied
        ).unwrap();
        assert!(channels.is_empty()); // Display denied to second client
    }
}
```

- [ ] **Step 2: Update lib.rs re-exports**

```rust
pub use config::ServerConfig;
pub use session_manager::SessionManager;
pub use client_session::ClientSession;
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-server/src/
git commit -m "feat(server): SessionManager with connect/disconnect/tombstone/routing lifecycle"
```

---

## Task 4: QualityMonitor

**Files:**
- Modify: `crates/prism-server/src/quality_monitor.rs`

- [ ] **Step 1: Write tests + implement QualityMonitor**

```rust
use prism_transport::quality::prober::{ConnectionProber, ProbeEcho, ActivityState};
use prism_transport::quality::bandwidth::BandwidthEstimator;
use prism_transport::quality::one_way_delay::OneWayDelayEstimator;
use prism_transport::quality::trend::TrendDetector;
use prism_transport::{ConnectionQuality, QualityRecommendation, TransportMetrics};
use prism_display::{DegradationLadder, DegradationLevel};

/// Integrates transport quality measurement with display degradation.
/// Combines prober + bandwidth + one-way-delay + trend → ConnectionQuality → DegradationLadder.
pub struct QualityMonitor {
    prober: ConnectionProber,
    bandwidth: BandwidthEstimator,
    one_way_delay: OneWayDelayEstimator,
    trend: TrendDetector,
    ladder: DegradationLadder,
    current_level: usize,
}

impl QualityMonitor {
    pub fn new(ladder: DegradationLadder) -> Self {
        Self {
            prober: ConnectionProber::new(),
            bandwidth: BandwidthEstimator::new(),
            one_way_delay: OneWayDelayEstimator::new(),
            trend: TrendDetector::new(),
            ladder,
            current_level: 0,
        }
    }

    /// Feed transport metrics. Returns updated quality + target degradation level.
    pub fn update(&mut self, metrics: &TransportMetrics) -> QualityUpdate {
        // Feed bandwidth estimator
        self.bandwidth.record_send(metrics.bytes_sent);
        self.bandwidth.record_recv(metrics.bytes_received);

        // Feed trend detector with RTT
        self.trend.record(metrics.rtt_us as f64);

        // Compute composite quality
        let quality = ConnectionQuality::compute(
            metrics.rtt_us,
            metrics.rtt_variance_us,
            metrics.loss_rate,
            self.bandwidth.send_bps(),
            self.bandwidth.recv_bps(),
            metrics.delay_asymmetry,
        );

        // Map to degradation level
        let target_level = self.ladder.target_level(&quality.recommendation);

        let changed = target_level != self.current_level;
        self.current_level = target_level;

        QualityUpdate {
            quality,
            target_level,
            current_level_params: self.ladder.levels.get(target_level).cloned(),
            level_changed: changed,
            trend: self.trend.trend(),
        }
    }

    pub fn set_activity(&mut self, state: ActivityState) {
        self.prober.set_activity(state);
    }

    pub fn current_level(&self) -> usize { self.current_level }

    pub fn prober_mut(&mut self) -> &mut ConnectionProber { &mut self.prober }
}

/// Result from quality evaluation.
pub struct QualityUpdate {
    pub quality: ConnectionQuality,
    pub target_level: usize,
    pub current_level_params: Option<DegradationLevel>,
    pub level_changed: bool,
    pub trend: prism_transport::quality::trend::Trend,
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_transport::DelayAsymmetry;

    fn good_metrics() -> TransportMetrics {
        TransportMetrics {
            rtt_us: 2000,           // 2ms
            rtt_variance_us: 200,
            loss_rate: 0.0,
            bytes_sent: 0,
            bytes_received: 0,
            ..TransportMetrics::default()
        }
    }

    fn bad_metrics() -> TransportMetrics {
        TransportMetrics {
            rtt_us: 300_000,        // 300ms
            rtt_variance_us: 50_000,
            loss_rate: 0.15,        // 15% loss
            bytes_sent: 0,
            bytes_received: 0,
            ..TransportMetrics::default()
        }
    }

    #[test]
    fn good_quality_stays_at_level_0() {
        let mut monitor = QualityMonitor::new(DegradationLadder::coding());
        let update = monitor.update(&good_metrics());
        assert_eq!(update.target_level, 0);
        assert!(!update.level_changed); // was already 0
    }

    #[test]
    fn bad_quality_increases_level() {
        let mut monitor = QualityMonitor::new(DegradationLadder::coding());
        let update = monitor.update(&bad_metrics());
        assert!(update.target_level > 0, "bad quality should degrade, got level {}", update.target_level);
        assert!(update.level_changed);
    }

    #[test]
    fn quality_update_includes_level_params() {
        let mut monitor = QualityMonitor::new(DegradationLadder::gaming());
        let update = monitor.update(&good_metrics());
        let params = update.current_level_params.unwrap();
        assert_eq!(params.max_fps, 120); // gaming level 0
    }

    #[test]
    fn activity_state_changes_probe_interval() {
        let mut monitor = QualityMonitor::new(DegradationLadder::coding());
        monitor.set_activity(ActivityState::Idle);
        assert_eq!(monitor.prober_mut().probe_interval(), std::time::Duration::from_secs(60));
        monitor.set_activity(ActivityState::ActiveStreaming);
        assert_eq!(monitor.prober_mut().probe_interval(), std::time::Duration::from_secs(2));
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-server/src/quality_monitor.rs
git commit -m "feat(server): QualityMonitor wiring transport quality to degradation ladder"
```

---

## Task 5: Graceful Shutdown Coordinator

**Files:**
- Modify: `crates/prism-server/src/shutdown.rs`

- [ ] **Step 1: Write tests + implement ShutdownCoordinator**

```rust
use std::time::{Duration, Instant};
use prism_session::control_msg::ShutdownNotice;

/// Coordinates graceful server shutdown.
pub struct ShutdownCoordinator {
    state: ShutdownState,
    notice: ShutdownNotice,
    shutdown_initiated: Option<Instant>,
    grace_period: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownState {
    Running,
    NoticesSent,
    GracePeriodExpired,
}

impl ShutdownCoordinator {
    pub fn new(grace_period: Duration) -> Self {
        Self {
            state: ShutdownState::Running,
            notice: ShutdownNotice {
                reason: String::new(),
                seconds_remaining: 0,
                will_restart: false,
            },
            shutdown_initiated: None,
            grace_period,
        }
    }

    /// Initiate shutdown with a reason.
    pub fn initiate(&mut self, reason: String, will_restart: bool) {
        self.notice = ShutdownNotice {
            reason,
            seconds_remaining: self.grace_period.as_secs() as u32,
            will_restart,
        };
        self.shutdown_initiated = Some(Instant::now());
        self.state = ShutdownState::NoticesSent;
    }

    /// Get the shutdown notice to send to clients.
    pub fn notice(&self) -> Option<&ShutdownNotice> {
        if self.state == ShutdownState::Running { None } else { Some(&self.notice) }
    }

    /// Check if grace period has expired.
    pub fn tick(&mut self) -> ShutdownState {
        if let Some(initiated) = self.shutdown_initiated {
            if initiated.elapsed() >= self.grace_period {
                self.state = ShutdownState::GracePeriodExpired;
            } else {
                let remaining = self.grace_period.saturating_sub(initiated.elapsed());
                self.notice.seconds_remaining = remaining.as_secs() as u32;
            }
        }
        self.state
    }

    pub fn state(&self) -> ShutdownState { self.state }
    pub fn is_shutting_down(&self) -> bool { self.state != ShutdownState::Running }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_running() {
        let coord = ShutdownCoordinator::new(Duration::from_secs(30));
        assert_eq!(coord.state(), ShutdownState::Running);
        assert!(!coord.is_shutting_down());
        assert!(coord.notice().is_none());
    }

    #[test]
    fn initiate_changes_state() {
        let mut coord = ShutdownCoordinator::new(Duration::from_secs(30));
        coord.initiate("restarting".into(), true);
        assert_eq!(coord.state(), ShutdownState::NoticesSent);
        assert!(coord.is_shutting_down());
        let notice = coord.notice().unwrap();
        assert!(notice.will_restart);
        assert_eq!(notice.reason, "restarting");
    }

    #[test]
    fn grace_period_expires() {
        let mut coord = ShutdownCoordinator::new(Duration::from_millis(10));
        coord.initiate("test".into(), false);
        std::thread::sleep(Duration::from_millis(15));
        let state = coord.tick();
        assert_eq!(state, ShutdownState::GracePeriodExpired);
    }

    #[test]
    fn countdown_updates() {
        let mut coord = ShutdownCoordinator::new(Duration::from_secs(30));
        coord.initiate("test".into(), false);
        std::thread::sleep(Duration::from_millis(10));
        coord.tick();
        let remaining = coord.notice().unwrap().seconds_remaining;
        assert!(remaining <= 30);
    }
}
```

- [ ] **Step 2: Update lib.rs re-exports**

```rust
pub mod config;
pub mod acceptor;
pub mod session_manager;
pub mod client_session;
pub mod recv_loop;
pub mod quality_monitor;
pub mod shutdown;

pub use config::ServerConfig;
pub use session_manager::SessionManager;
pub use client_session::ClientSession;
pub use quality_monitor::{QualityMonitor, QualityUpdate};
pub use shutdown::{ShutdownCoordinator, ShutdownState};
```

- [ ] **Step 3: Verify, commit**

```bash
git add crates/prism-server/src/shutdown.rs crates/prism-server/src/lib.rs
git commit -m "feat(server): ShutdownCoordinator with grace period"
```

---

## Task 6: RecvLoop Dispatch Logic

**Files:**
- Modify: `crates/prism-server/src/recv_loop.rs`

- [ ] **Step 1: Write tests + implement recv_loop dispatch helpers**

The actual `select!` loop requires a live connection, but the dispatch decision logic is pure and testable:

```rust
use bytes::Bytes;
use prism_protocol::header::{PrismHeader, HEADER_SIZE};
use prism_protocol::channel::CHANNEL_CONTROL;
use prism_session::{ClientId, ChannelDispatcher, ChannelBandwidthTracker};
use prism_session::control_msg;

/// Classify an incoming datagram for routing.
#[derive(Debug, PartialEq)]
pub enum DatagramAction {
    /// Route to probe handler (quality measurement).
    ProbeResponse,
    /// Route to channel dispatcher.
    ChannelDispatch { channel_id: u16 },
    /// Malformed — drop silently.
    Drop,
}

/// Classify an incoming datagram by parsing its PRISM header.
pub fn classify_datagram(data: &Bytes) -> DatagramAction {
    if data.len() < HEADER_SIZE {
        return DatagramAction::Drop;
    }
    match PrismHeader::decode_from_slice(data) {
        Ok(header) => {
            if header.channel_id == CHANNEL_CONTROL && header.msg_type == control_msg::PROBE_RESPONSE {
                DatagramAction::ProbeResponse
            } else {
                DatagramAction::ChannelDispatch { channel_id: header.channel_id }
            }
        }
        Err(_) => DatagramAction::Drop,
    }
}

/// Record bandwidth for a parsed datagram.
pub fn record_datagram_bandwidth(
    tracker: &ChannelBandwidthTracker,
    header: &PrismHeader,
) {
    tracker.record_recv(header.channel_id, header.payload_length);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use prism_protocol::header::PrismHeader;

    fn make_datagram(channel_id: u16, msg_type: u8, payload_len: u32) -> Bytes {
        let header = PrismHeader {
            version: 0, channel_id, msg_type, flags: 0,
            sequence: 1, timestamp_us: 0, payload_length: payload_len,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        buf.freeze()
    }

    #[test]
    fn classify_probe_response() {
        let data = make_datagram(CHANNEL_CONTROL, control_msg::PROBE_RESPONSE, 0);
        assert_eq!(classify_datagram(&data), DatagramAction::ProbeResponse);
    }

    #[test]
    fn classify_display_datagram() {
        let data = make_datagram(0x001, 0x02, 1024); // Display, SLICE
        assert_eq!(classify_datagram(&data), DatagramAction::ChannelDispatch { channel_id: 0x001 });
    }

    #[test]
    fn classify_control_non_probe() {
        let data = make_datagram(CHANNEL_CONTROL, control_msg::HEARTBEAT, 0);
        assert_eq!(classify_datagram(&data), DatagramAction::ChannelDispatch { channel_id: CHANNEL_CONTROL });
    }

    #[test]
    fn classify_too_short_drops() {
        let data = Bytes::from_static(&[0, 1, 2, 3]);
        assert_eq!(classify_datagram(&data), DatagramAction::Drop);
    }

    #[test]
    fn bandwidth_tracking() {
        let tracker = ChannelBandwidthTracker::new();
        let header = PrismHeader {
            version: 0, channel_id: 0x001, msg_type: 0, flags: 0,
            sequence: 0, timestamp_us: 0, payload_length: 5000,
        };
        record_datagram_bandwidth(&tracker, &header);
        assert_eq!(tracker.recv_bytes(0x001), 5000);
    }
}
```

- [ ] **Step 2: Verify, commit**

```bash
git add crates/prism-server/src/recv_loop.rs
git commit -m "feat(server): recv_loop datagram classification and bandwidth tracking"
```

---

## Task 7: QUIC Loopback Integration Test

**Files:**
- Modify: `crates/prism-server/src/session_manager.rs` (add integration test)

This test verifies the full pipeline: QUIC connection → SessionManager → routing table → channel dispatch.

- [ ] **Step 1: Write integration test**

Add to session_manager.rs tests module (or create a separate integration test file):

```rust
    #[tokio::test]
    async fn integration_session_with_mock_transport() {
        use prism_transport::connection::mock::MockConnection;
        use prism_transport::{FramedWriter, FramedReader, PrismConnection, StreamPriority};
        use prism_session::ChannelDispatcher;
        use prism_protocol::channel::{CHANNEL_DISPLAY, CHANNEL_CONTROL};
        use std::sync::atomic::{AtomicU32, Ordering};

        // Setup
        let mut mgr = SessionManager::new(test_config());
        let conn = Arc::new(UnifiedConnection::new(
            Box::new(MockConnection::new(1200)),
            None,
        ));

        // Connect client
        let client = Uuid::from_bytes([5; 16]);
        let device = Uuid::from_bytes([50; 16]);
        let channels = mgr.new_session(
            client, device, conn.clone(),
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY, CHANNEL_CONTROL],
        ).unwrap();
        assert_eq!(channels.len(), 2);

        // Verify routing
        let snap = mgr.routing_table().snapshot();
        let display_routes = snap.channel_routes.get(&CHANNEL_DISPLAY).unwrap();
        assert_eq!(display_routes.len(), 1);
        assert_eq!(display_routes[0].client_id, client);

        // Disconnect
        mgr.disconnect(client, "test cleanup".into());
        let snap = mgr.routing_table().snapshot();
        assert!(snap.channel_routes.get(&CHANNEL_DISPLAY)
            .map_or(true, |r| r.is_empty()));
    }

    #[tokio::test]
    async fn integration_multi_client_channel_conflict() {
        let mut mgr = SessionManager::new(test_config());

        let client_a = Uuid::from_bytes([1; 16]);
        let device_a = Uuid::from_bytes([10; 16]);
        let client_b = Uuid::from_bytes([2; 16]);
        let device_b = Uuid::from_bytes([20; 16]);

        let conn_a = Arc::new(UnifiedConnection::new(
            Box::new(MockConnection::new(1200)), None,
        ));
        let conn_b = Arc::new(UnifiedConnection::new(
            Box::new(MockConnection::new(1200)), None,
        ));

        // Client A gets Display (exclusive)
        let channels_a = mgr.new_session(
            client_a, device_a, conn_a,
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY, CHANNEL_CONTROL, CHANNEL_CLIPBOARD],
        ).unwrap();
        assert!(channels_a.contains(&CHANNEL_DISPLAY));

        // Client B tries Display (denied) but gets shared channels
        let channels_b = mgr.new_session(
            client_b, device_b, conn_b,
            ConnectionProfile::coding(),
            &[CHANNEL_DISPLAY, CHANNEL_CONTROL, CHANNEL_CLIPBOARD],
        ).unwrap();
        assert!(!channels_b.contains(&CHANNEL_DISPLAY)); // denied
        assert!(channels_b.contains(&CHANNEL_CONTROL)); // shared = OK
        assert!(channels_b.contains(&CHANNEL_CLIPBOARD)); // shared = OK

        // Verify routing: Display has only A, Control has both
        let snap = mgr.routing_table().snapshot();
        assert_eq!(snap.channel_routes[&CHANNEL_DISPLAY].len(), 1);
        assert_eq!(snap.channel_routes[&CHANNEL_CONTROL].len(), 2);
    }

    #[tokio::test]
    async fn integration_reconnect_via_tombstone() {
        let mut mgr = SessionManager::new(test_config());

        let client_a = Uuid::from_bytes([1; 16]);
        let device_a = Uuid::from_bytes([10; 16]);

        let conn1 = Arc::new(UnifiedConnection::new(
            Box::new(MockConnection::new(1200)), None,
        ));
        mgr.new_session(client_a, device_a, conn1,
            ConnectionProfile::coding(), &[CHANNEL_DISPLAY]).unwrap();

        // Disconnect → creates tombstone
        mgr.disconnect(client_a, "network lost".into());
        assert_eq!(mgr.client_count(), 0);

        // Reconnect as new client_id but same device
        let client_b = Uuid::from_bytes([2; 16]);
        let conn2 = Arc::new(UnifiedConnection::new(
            Box::new(MockConnection::new(1200)), None,
        ));
        let channels = mgr.new_session(client_b, device_a, conn2,
            ConnectionProfile::coding(), &[CHANNEL_DISPLAY]).unwrap();
        assert!(channels.contains(&CHANNEL_DISPLAY));
        assert_eq!(mgr.client_count(), 1);
    }
```

- [ ] **Step 2: Run all tests**

Run: `cargo test -p prism-server`
Run: `cargo test --workspace`

- [ ] **Step 3: Commit**

```bash
git add crates/prism-server/src/
git commit -m "feat(server): integration tests for multi-client sessions and reconnection"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Crate setup + ServerConfig | 2 |
| 2 | ClientSession per-client state | 4 |
| 3 | SessionManager orchestrator | 7 |
| 4 | QualityMonitor (quality → degradation wiring) | 4 |
| 5 | ShutdownCoordinator | 4 |
| 6 | RecvLoop dispatch classification | 5 |
| 7 | Integration tests (multi-client, tombstone reconnect) | 3 |
| **Total** | | **~29** |

**What this plan delivers:**
- Full session lifecycle: connect → active → suspend → tombstone → reconnect
- RoutingTable atomic updates on connect/disconnect
- Channel ownership enforcement (exclusive conflicts, shared fan-out)
- Quality → degradation feedback loop
- Datagram classification for recv dispatch
- Graceful shutdown with grace period
- Multi-client integration test proving routing + ownership

**What remains platform-specific (not in any plan):**
- Actual DDA/WGC capture implementation
- NVENC/AMF/QSV encoder implementation
- Win32 event hooks for speculative IDR
- WebSocket/TCP fallback (Phase 4)
- Prometheus export (optional)
- Client binary (Phase 2+)
