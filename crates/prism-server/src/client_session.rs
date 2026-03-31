// Per-client session state: identity, connection handle, channel subscriptions, lifecycle.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use prism_session::{ClientId, ConnectionProfile, SessionState};
use prism_transport::UnifiedConnection;

/// All runtime state associated with a single connected (or suspended) client.
pub struct ClientSession {
    /// Unique identifier for this client (stable across reconnections within the
    /// same pairing).
    pub client_id: ClientId,

    /// Device-level identity UUID (comes from the pairing record).
    pub device_id: Uuid,

    /// The live transport connection to the client.
    pub connection: Arc<UnifiedConnection>,

    /// Negotiated connection profile (display quality, frame-rate, etc.).
    pub profile: ConnectionProfile,

    /// Set of channel IDs this client has subscribed to.
    pub subscribed_channels: HashSet<u16>,

    /// Logical session lifecycle state.
    pub state: SessionState,

    /// Wall-clock instant when the session was first established.
    pub connected_at: Instant,

    /// Wall-clock instant of the most recent activity (heartbeat, datagram, etc.).
    pub last_activity: Instant,
}

impl ClientSession {
    /// Create a new session in the `Active` state.
    pub fn new(
        client_id: ClientId,
        device_id: Uuid,
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

    /// Update `last_activity` to now.
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Subscribe the client to the given channel.
    /// Returns `true` if the channel was not already subscribed.
    pub fn subscribe(&mut self, channel_id: u16) -> bool {
        self.subscribed_channels.insert(channel_id)
    }

    /// Unsubscribe the client from the given channel.
    /// Returns `true` if the channel was previously subscribed.
    pub fn unsubscribe(&mut self, channel_id: u16) -> bool {
        self.subscribed_channels.remove(&channel_id)
    }

    /// Returns `true` when the session is in the `Active` state.
    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }

    /// Move the session to the `Suspended` state.
    /// Has no effect if already suspended or tombstoned.
    pub fn suspend(&mut self) {
        if self.state == SessionState::Active {
            self.state = SessionState::Suspended;
        }
    }

    /// Move a `Suspended` session back to `Active` and update `last_activity`.
    /// Has no effect if the session is not currently suspended.
    pub fn reactivate(&mut self) {
        if self.state == SessionState::Suspended {
            self.state = SessionState::Active;
            self.touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use tokio::sync::broadcast;
    use prism_transport::{
        PrismConnection, TransportError, TransportMetrics, TransportType, TransportEvent,
        StreamPriority, OwnedSendStream, OwnedRecvStream,
    };

    // ── Minimal PrismConnection mock (MockConnection is pub(crate) in prism-transport) ──

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
        async fn open_bi(&self, _p: StreamPriority) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
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

    fn make_session() -> ClientSession {
        let conn = Arc::new(UnifiedConnection::new(Box::new(StubConnection), None));
        ClientSession::new(
            Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
            Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
            conn,
            ConnectionProfile::gaming(),
        )
    }

    #[test]
    fn new_session_is_active() {
        let session = make_session();
        assert!(session.is_active());
        assert_eq!(session.state, SessionState::Active);
        assert!(session.subscribed_channels.is_empty());
        // connected_at and last_activity should be very close to each other
        assert!(session.last_activity >= session.connected_at);
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let mut session = make_session();

        // Subscribe returns true on first insert, false on duplicate
        assert!(session.subscribe(1));
        assert!(!session.subscribe(1));
        assert!(session.subscribe(2));
        assert_eq!(session.subscribed_channels.len(), 2);
        assert!(session.subscribed_channels.contains(&1));
        assert!(session.subscribed_channels.contains(&2));

        // Unsubscribe returns true when channel was present
        assert!(session.unsubscribe(1));
        assert!(!session.subscribed_channels.contains(&1));
        // Unsubscribe returns false for a channel that wasn't subscribed
        assert!(!session.unsubscribe(99));
    }

    #[test]
    fn suspend_and_reactivate() {
        let mut session = make_session();
        assert!(session.is_active());

        session.suspend();
        assert_eq!(session.state, SessionState::Suspended);
        assert!(!session.is_active());

        // Calling suspend again on an already-suspended session is a no-op
        session.suspend();
        assert_eq!(session.state, SessionState::Suspended);

        session.reactivate();
        assert!(session.is_active());
        assert_eq!(session.state, SessionState::Active);

        // Calling reactivate on an active session is a no-op
        session.reactivate();
        assert!(session.is_active());
    }

    #[test]
    fn touch_updates_activity() {
        let mut session = make_session();
        let before = session.last_activity;

        // Spin briefly so Instant::now() returns a value after `before`
        std::thread::sleep(std::time::Duration::from_millis(1));
        session.touch();

        assert!(session.last_activity >= before);
    }
}
