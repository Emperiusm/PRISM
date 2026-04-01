// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// SessionManager: orchestrates client lifecycle across all subsystems.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;
use uuid::Uuid;

use prism_session::{
    BandwidthArbiter, ChannelGrantResult, ChannelRegistry, ClientId, ConnectionProfile,
    HeartbeatMonitor, RouteEntry, RoutingMutation, RoutingTable, SessionError, SessionEvent,
    Tombstone, TombstoneStore,
};
use prism_transport::UnifiedConnection;

use crate::client_session::ClientSession;
use crate::config::ServerConfig;

/// Core orchestrator managing client lifecycle, routing, and resource allocation.
#[allow(dead_code)]
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
    /// Initialise all subsystems with the provided server configuration.
    pub fn new(config: ServerConfig) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        let tombstone_max = config.tombstone_max_age().as_secs();
        let suspend = config.heartbeat_suspend();
        let tombstone_dur = config.heartbeat_tombstone();
        let total_bps = config.total_bandwidth_bps;
        Self {
            clients: HashMap::new(),
            routing_table: Arc::new(RoutingTable::new()),
            channel_registry: ChannelRegistry::with_defaults(),
            arbiter: BandwidthArbiter::new(total_bps),
            tombstones: TombstoneStore::new(tombstone_max),
            heartbeat: HeartbeatMonitor::new(suspend, tombstone_dur),
            event_tx,
            config,
        }
    }

    /// Clone the Arc-wrapped routing table for read access.
    pub fn routing_table(&self) -> Arc<RoutingTable> {
        Arc::clone(&self.routing_table)
    }

    /// Subscribe to session events.
    pub fn events(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    /// Connect a new client session.
    ///
    /// Checks for an existing tombstone (for reconnection), creates a
    /// `ClientSession`, requests channels, batch-updates the routing table,
    /// registers with the heartbeat monitor, and emits `ClientConnected`.
    ///
    /// Returns the list of channel IDs that were actually granted.
    pub fn new_session(
        &mut self,
        client_id: ClientId,
        device_id: Uuid,
        connection: Arc<UnifiedConnection>,
        profile: ConnectionProfile,
        requested_channels: &[u16],
    ) -> Result<Vec<u16>, SessionError> {
        // Claim a tombstone for this device if one exists (reconnect path).
        let _tombstone = self.tombstones.claim_by_device(device_id);

        let mut session = ClientSession::new(client_id, device_id, connection, profile);

        let mut mutations = Vec::new();
        let mut granted_channels = Vec::new();

        for &channel_id in requested_channels {
            let result = self
                .channel_registry
                .request_channel(channel_id, client_id)?;
            match result {
                ChannelGrantResult::Granted
                | ChannelGrantResult::AlreadyOwned
                | ChannelGrantResult::Transferred { .. } => {
                    session.subscribe(channel_id);
                    mutations.push(RoutingMutation::AddRoute {
                        channel_id,
                        entry: RouteEntry { client_id },
                    });
                    granted_channels.push(channel_id);
                }
                ChannelGrantResult::Denied { .. } | ChannelGrantResult::PendingApproval { .. } => {
                    // Channel not granted — skip silently.
                }
            }
        }

        self.routing_table.batch_update(mutations);
        self.heartbeat.register(client_id);
        self.clients.insert(client_id, session);

        let _ = self.event_tx.send(SessionEvent::ClientConnected {
            client_id,
            device_name: format!("{device_id}"),
        });

        Ok(granted_channels)
    }

    /// Disconnect a client: release channels, clean up routing, create tombstone,
    /// and emit `ClientDisconnected`.
    pub fn disconnect(&mut self, client_id: ClientId, reason: String) {
        let Some(session) = self.clients.remove(&client_id) else {
            return;
        };

        self.channel_registry.release_all(client_id);
        self.routing_table
            .batch_update(vec![RoutingMutation::RemoveClient(client_id)]);
        self.heartbeat.unregister(client_id);
        self.arbiter.remove_client(client_id);

        let tombstone = Tombstone::new(
            client_id,
            session.device_id,
            session.subscribed_channels.clone(),
        );
        self.tombstones.insert(tombstone);

        let _ = self
            .event_tx
            .send(SessionEvent::ClientDisconnected { client_id, reason });
    }

    /// Touch a client's session and heartbeat monitor.
    pub fn activity(&mut self, client_id: ClientId) {
        if let Some(session) = self.clients.get_mut(&client_id) {
            session.touch();
        }
        self.heartbeat.activity(client_id);
    }

    /// Number of currently active client sessions.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Borrow a session by client ID.
    pub fn get_session(&self, client_id: ClientId) -> Option<&ClientSession> {
        self.clients.get(&client_id)
    }

    /// Check for stale clients; tombstone expired ones and suspend overdue ones.
    ///
    /// Returns `(client_id, was_tombstoned)` for each client that transitioned.
    pub fn check_heartbeats(&mut self) -> Vec<(ClientId, bool)> {
        let stale = self.heartbeat.stale_clients();
        let mut results = Vec::new();

        // Collect IDs to avoid borrow-conflict while mutating self.
        let tombstone_ids: Vec<ClientId> = stale
            .iter()
            .filter(|(_, _, needs_tombstone)| *needs_tombstone)
            .map(|(id, _, _)| *id)
            .collect();

        let suspend_ids: Vec<ClientId> = stale
            .iter()
            .filter(|(_, needs_suspend, needs_tombstone)| *needs_suspend && !*needs_tombstone)
            .map(|(id, _, _)| *id)
            .collect();

        for client_id in tombstone_ids {
            self.disconnect(client_id, "heartbeat timeout".to_string());
            results.push((client_id, true));
        }

        for client_id in suspend_ids {
            if let Some(session) = self.clients.get_mut(&client_id) {
                session.suspend();
            }
            results.push((client_id, false));
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use prism_transport::{
        OwnedRecvStream, OwnedSendStream, PrismConnection, StreamPriority, TransportError,
        TransportEvent, TransportMetrics, TransportType,
    };
    use tokio::sync::broadcast;

    // ── Minimal stub connection (MockConnection is pub(crate) in prism-transport) ──

    struct StubConnection;

    #[async_trait]
    impl PrismConnection for StubConnection {
        fn try_send_datagram(&self, _data: Bytes) -> Result<(), TransportError> {
            Ok(())
        }
        async fn send_datagram(&self, _data: Bytes) -> Result<(), TransportError> {
            Ok(())
        }
        async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
            std::future::pending().await
        }
        async fn open_bi(
            &self,
            _p: StreamPriority,
        ) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
            Err(TransportError::ConnectionClosed)
        }
        async fn open_uni(&self, _p: StreamPriority) -> Result<OwnedSendStream, TransportError> {
            Err(TransportError::ConnectionClosed)
        }
        async fn accept_bi(&self) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
            std::future::pending().await
        }
        async fn accept_uni(&self) -> Result<OwnedRecvStream, TransportError> {
            std::future::pending().await
        }
        fn metrics(&self) -> TransportMetrics {
            TransportMetrics::default()
        }
        fn transport_type(&self) -> TransportType {
            TransportType::Quic
        }
        fn max_datagram_size(&self) -> usize {
            1200
        }
        fn events(&self) -> broadcast::Receiver<TransportEvent> {
            let (tx, rx) = broadcast::channel(1);
            drop(tx);
            rx
        }
        async fn close(&self) {}
    }

    fn make_conn() -> Arc<UnifiedConnection> {
        Arc::new(UnifiedConnection::new(Box::new(StubConnection), None))
    }

    fn new_id() -> Uuid {
        Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext))
    }

    fn make_manager() -> SessionManager {
        // Use large values so heartbeat expiry only triggers when explicitly
        // tested; precise sub-second timing is not needed here.
        let config = ServerConfig {
            heartbeat_suspend_secs: 3600,
            heartbeat_tombstone_secs: 7200,
            tombstone_max_age_secs: 300,
            ..ServerConfig::default()
        };
        SessionManager::new(config)
    }

    // Display = 0x001 (exclusive), Control = 0x006 (shared)
    const DISPLAY: u16 = 0x001;
    const CONTROL: u16 = 0x006;

    #[test]
    fn new_session_registers_client() {
        let mut mgr = make_manager();
        let client_id = new_id();
        let device_id = new_id();

        mgr.new_session(
            client_id,
            device_id,
            make_conn(),
            ConnectionProfile::gaming(),
            &[CONTROL],
        )
        .unwrap();

        assert_eq!(mgr.client_count(), 1);
        assert!(mgr.get_session(client_id).is_some());
    }

    #[test]
    fn new_session_updates_routing_table() {
        let mut mgr = make_manager();
        let client_id = new_id();
        let device_id = new_id();

        let granted = mgr
            .new_session(
                client_id,
                device_id,
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL],
            )
            .unwrap();

        assert_eq!(granted.len(), 2);

        let snap = mgr.routing_table().snapshot();
        assert!(snap.channel_routes.contains_key(&DISPLAY));
        assert!(snap.channel_routes.contains_key(&CONTROL));
        assert_eq!(snap.channel_routes[&DISPLAY][0].client_id, client_id);
    }

    #[test]
    fn disconnect_removes_client() {
        let mut mgr = make_manager();
        let client_id = new_id();
        let device_id = new_id();

        mgr.new_session(
            client_id,
            device_id,
            make_conn(),
            ConnectionProfile::gaming(),
            &[DISPLAY],
        )
        .unwrap();

        mgr.disconnect(client_id, "test".to_string());

        assert_eq!(mgr.client_count(), 0);
        let snap = mgr.routing_table().snapshot();
        assert!(!snap.channel_routes.contains_key(&DISPLAY));
    }

    #[test]
    fn disconnect_creates_tombstone_allows_reconnect() {
        let mut mgr = make_manager();
        let client_id_a = new_id();
        let device_id = new_id();

        // First connect + disconnect.
        mgr.new_session(
            client_id_a,
            device_id,
            make_conn(),
            ConnectionProfile::gaming(),
            &[DISPLAY],
        )
        .unwrap();
        mgr.disconnect(client_id_a, "gone".to_string());

        // Reconnect same device with a new client_id; tombstone still valid so
        // the channel_registry slot was freed by disconnect and can be re-granted.
        let client_id_b = new_id();
        let granted = mgr
            .new_session(
                client_id_b,
                device_id,
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY],
            )
            .unwrap();

        assert!(
            !granted.is_empty(),
            "reconnecting device should get channels"
        );
        assert_eq!(mgr.client_count(), 1);
    }

    #[test]
    fn activity_updates_heartbeat() {
        let mut mgr = make_manager();
        let client_id = new_id();
        let device_id = new_id();

        mgr.new_session(
            client_id,
            device_id,
            make_conn(),
            ConnectionProfile::gaming(),
            &[CONTROL],
        )
        .unwrap();

        // Touch immediately; client should not be tombstoned.
        mgr.activity(client_id);
        let stale = mgr.check_heartbeats();
        assert!(
            !stale
                .iter()
                .any(|(id, tombstoned)| *id == client_id && *tombstoned),
            "client should not be tombstoned immediately after activity"
        );
    }

    #[test]
    fn session_events_emitted() {
        let mut mgr = make_manager();
        let mut rx = mgr.events();
        let client_id = new_id();
        let device_id = new_id();

        mgr.new_session(
            client_id,
            device_id,
            make_conn(),
            ConnectionProfile::gaming(),
            &[CONTROL],
        )
        .unwrap();

        let event = rx
            .try_recv()
            .expect("should have received ClientConnected event");
        match event {
            SessionEvent::ClientConnected { client_id: id, .. } => {
                assert_eq!(id, client_id);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn exclusive_channel_denied_to_second_client() {
        let mut mgr = make_manager();
        let a = new_id();
        let b = new_id();

        // Client A gets Display (exclusive) and Control (shared).
        let a_channels = mgr
            .new_session(
                a,
                new_id(),
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL],
            )
            .unwrap();
        assert!(a_channels.contains(&DISPLAY));
        assert!(a_channels.contains(&CONTROL));

        // Client B: Display denied (exclusive, A holds it), Control granted (shared).
        let b_channels = mgr
            .new_session(
                b,
                new_id(),
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL],
            )
            .unwrap();

        assert!(
            !b_channels.contains(&DISPLAY),
            "Display must be denied to B"
        );
        assert!(
            b_channels.contains(&CONTROL),
            "Control (shared) must be granted to B"
        );
    }

    // ── Integration tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn integration_session_with_mock_transport() {
        let mut mgr = make_manager();
        let client_id = new_id();
        let device_id = new_id();

        // Connect with Display + Control, verify routing snapshot.
        let granted = mgr
            .new_session(
                client_id,
                device_id,
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL],
            )
            .unwrap();

        assert!(granted.contains(&DISPLAY));
        assert!(granted.contains(&CONTROL));
        assert_eq!(mgr.client_count(), 1);

        let snap = mgr.routing_table().snapshot();
        assert!(snap.channel_routes.contains_key(&DISPLAY));
        assert!(snap.channel_routes.contains_key(&CONTROL));
        assert_eq!(snap.channel_routes[&DISPLAY][0].client_id, client_id);
        assert_eq!(snap.channel_routes[&CONTROL][0].client_id, client_id);

        // Disconnect and verify cleanup.
        mgr.disconnect(client_id, "done".to_string());
        assert_eq!(mgr.client_count(), 0);
        assert!(mgr.get_session(client_id).is_none());

        let snap_after = mgr.routing_table().snapshot();
        assert!(!snap_after.channel_routes.contains_key(&DISPLAY));
        assert!(!snap_after.channel_routes.contains_key(&CONTROL));
    }

    #[tokio::test]
    async fn integration_multi_client_channel_conflict() {
        // CLIPBOARD = 0x004, shared channel.
        const CLIPBOARD: u16 = 0x004;

        let mut mgr = make_manager();
        let a = new_id();
        let b = new_id();

        // Client A gets Display (exclusive) + Control (shared) + Clipboard (shared).
        let a_channels = mgr
            .new_session(
                a,
                new_id(),
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL, CLIPBOARD],
            )
            .unwrap();
        assert!(
            a_channels.contains(&DISPLAY),
            "A must get exclusive Display"
        );
        assert!(a_channels.contains(&CONTROL));
        assert!(a_channels.contains(&CLIPBOARD));

        // Client B: Display denied (A holds it), Control + Clipboard granted (shared).
        let b_channels = mgr
            .new_session(
                b,
                new_id(),
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL, CLIPBOARD],
            )
            .unwrap();
        assert!(
            !b_channels.contains(&DISPLAY),
            "B must be denied exclusive Display"
        );
        assert!(b_channels.contains(&CONTROL), "B must get shared Control");
        assert!(
            b_channels.contains(&CLIPBOARD),
            "B must get shared Clipboard"
        );

        // Routing: Display has exactly 1 route (A), Control has 2 routes (A + B).
        let snap = mgr.routing_table().snapshot();
        assert_eq!(snap.channel_routes[&DISPLAY].len(), 1);
        assert_eq!(snap.channel_routes[&DISPLAY][0].client_id, a);
        assert_eq!(snap.channel_routes[&CONTROL].len(), 2);
    }

    #[tokio::test]
    async fn integration_reconnect_via_tombstone() {
        let mut mgr = make_manager();
        let client_id_a = new_id();
        let device_id = new_id();

        // Initial connection: client A claims Display.
        let first_granted = mgr
            .new_session(
                client_id_a,
                device_id,
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL],
            )
            .unwrap();
        assert!(first_granted.contains(&DISPLAY));

        // Disconnect creates a tombstone for this device.
        mgr.disconnect(client_id_a, "network drop".to_string());
        assert_eq!(mgr.client_count(), 0);

        // Reconnect as a fresh client_id but the same device_id.
        // Tombstone is claimed internally; Display should be re-granted because
        // disconnect freed the exclusive slot.
        let client_id_b = new_id();
        let reconnect_granted = mgr
            .new_session(
                client_id_b,
                device_id,
                make_conn(),
                ConnectionProfile::gaming(),
                &[DISPLAY, CONTROL],
            )
            .unwrap();

        assert!(
            reconnect_granted.contains(&DISPLAY),
            "reconnecting device must regain Display channel"
        );
        assert_eq!(mgr.client_count(), 1);

        // Routing table should now point to the new client_id.
        let snap = mgr.routing_table().snapshot();
        assert_eq!(snap.channel_routes[&DISPLAY][0].client_id, client_id_b);
    }
}
