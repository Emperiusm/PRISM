// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Heartbeat: session liveness detection.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::types::ClientId;

struct HeartbeatState {
    last_activity: Instant,
}

/// Tracks per-client liveness and classifies clients as healthy, suspend-worthy,
/// or tombstone-worthy based on configurable inactivity thresholds.
pub struct HeartbeatMonitor {
    clients: HashMap<ClientId, HeartbeatState>,
    suspend_threshold: Duration,
    tombstone_threshold: Duration,
}

impl HeartbeatMonitor {
    pub fn new(suspend_threshold: Duration, tombstone_threshold: Duration) -> Self {
        Self {
            clients: HashMap::new(),
            suspend_threshold,
            tombstone_threshold,
        }
    }

    /// Register a new client, resetting its activity timer to now.
    pub fn register(&mut self, client_id: ClientId) {
        self.clients.insert(client_id, HeartbeatState { last_activity: Instant::now() });
    }

    /// Remove a client from monitoring.
    pub fn unregister(&mut self, client_id: ClientId) {
        self.clients.remove(&client_id);
    }

    /// Reset the activity timer for an existing client.
    /// No-op if the client is not registered.
    pub fn activity(&mut self, client_id: ClientId) {
        if let Some(state) = self.clients.get_mut(&client_id) {
            state.last_activity = Instant::now();
        }
    }

    /// `true` when the client's elapsed idle time exceeds `suspend_threshold`.
    /// Returns `false` if the client is not registered.
    pub fn needs_suspend(&self, client_id: ClientId) -> bool {
        self.clients
            .get(&client_id)
            .map(|s| s.last_activity.elapsed() > self.suspend_threshold)
            .unwrap_or(false)
    }

    /// `true` when the client's elapsed idle time exceeds `tombstone_threshold`.
    /// Returns `false` if the client is not registered.
    pub fn needs_tombstone(&self, client_id: ClientId) -> bool {
        self.clients
            .get(&client_id)
            .map(|s| s.last_activity.elapsed() > self.tombstone_threshold)
            .unwrap_or(false)
    }

    /// Returns all clients that are stale (need either suspend or tombstone),
    /// as tuples of `(client_id, needs_suspend, needs_tombstone)`.
    pub fn stale_clients(&self) -> Vec<(ClientId, bool, bool)> {
        self.clients
            .iter()
            .filter_map(|(id, state)| {
                let elapsed = state.last_activity.elapsed();
                let ns = elapsed > self.suspend_threshold;
                let nt = elapsed > self.tombstone_threshold;
                if ns || nt {
                    Some((*id, ns, nt))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use uuid::Uuid;

    fn client() -> ClientId {
        Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext))
    }

    fn monitor() -> HeartbeatMonitor {
        HeartbeatMonitor::new(
            Duration::from_millis(10),
            Duration::from_millis(15),
        )
    }

    #[test]
    fn new_client_is_alive() {
        let mut m = monitor();
        let id = client();
        m.register(id);
        assert!(!m.needs_suspend(id));
        assert!(!m.needs_tombstone(id));
    }

    #[test]
    fn activity_resets_timer() {
        let mut m = monitor();
        let id = client();
        m.register(id);
        thread::sleep(Duration::from_millis(8));
        m.activity(id);
        thread::sleep(Duration::from_millis(8));
        // Total idle after reset: ~8 ms < 10 ms threshold.
        assert!(!m.needs_suspend(id));
    }

    #[test]
    fn needs_suspend_after_threshold() {
        let mut m = monitor();
        let id = client();
        m.register(id);
        thread::sleep(Duration::from_millis(15));
        assert!(m.needs_suspend(id));
    }

    #[test]
    fn needs_tombstone_after_threshold() {
        let mut m = monitor();
        let id = client();
        m.register(id);
        thread::sleep(Duration::from_millis(20));
        assert!(m.needs_tombstone(id));
    }

    #[test]
    fn unregister_removes_client() {
        let mut m = monitor();
        let id = client();
        m.register(id);
        m.unregister(id);
        // Unregistered clients read as false, not stale.
        assert!(!m.needs_suspend(id));
    }
}
