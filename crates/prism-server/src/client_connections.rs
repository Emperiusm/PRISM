use std::collections::HashMap;
use std::sync::Mutex;
use bytes::Bytes;
use uuid::Uuid;

/// Thread-safe store of client QUIC connections for sending frames.
pub struct ClientConnectionStore {
    connections: Mutex<HashMap<Uuid, quinn::Connection>>,
}

impl ClientConnectionStore {
    pub fn new() -> Self {
        Self { connections: Mutex::new(HashMap::new()) }
    }

    pub fn add(&self, client_id: Uuid, conn: quinn::Connection) {
        self.connections.lock().unwrap().insert(client_id, conn);
    }

    pub fn remove(&self, client_id: &Uuid) {
        self.connections.lock().unwrap().remove(client_id);
    }

    /// Send a datagram to all connected clients. Returns (sent_count, error_count).
    pub fn broadcast_datagram(&self, data: &Bytes) -> (usize, usize) {
        let conns = self.connections.lock().unwrap();
        let mut sent = 0;
        let mut errors = 0;
        for (_, conn) in conns.iter() {
            match conn.send_datagram(data.clone()) {
                Ok(()) => sent += 1,
                Err(_) => errors += 1,
            }
        }
        (sent, errors)
    }

    pub fn client_count(&self) -> usize {
        self.connections.lock().unwrap().len()
    }

    /// Snapshot all current connections for async use (avoids holding the mutex
    /// across await points).
    pub fn snapshot(&self) -> Vec<quinn::Connection> {
        self.connections.lock().unwrap().values().cloned().collect()
    }

    /// Snapshot all current (client_id, connection) pairs for async use.
    pub fn snapshot_with_ids(&self) -> Vec<(Uuid, quinn::Connection)> {
        self.connections
            .lock()
            .unwrap()
            .iter()
            .map(|(id, conn)| (*id, conn.clone()))
            .collect()
    }
}

impl Default for ClientConnectionStore {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_store() {
        let store = ClientConnectionStore::new();
        assert_eq!(store.client_count(), 0);
    }

    #[test]
    fn broadcast_empty_returns_zero() {
        let store = ClientConnectionStore::new();
        let (sent, errors) = store.broadcast_datagram(&Bytes::from_static(b"test"));
        assert_eq!(sent, 0);
        assert_eq!(errors, 0);
    }
}
