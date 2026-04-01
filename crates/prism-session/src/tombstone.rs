// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Tombstone: session tombstone tracking for reconnection support.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::ClientId;

/// Describes what recovery action should be taken when a tombstoned client reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelRecoveryState {
    /// Re-send an IDR frame to resync the display stream.
    SendIdr,
    /// Reset the audio pipeline.
    AudioReset,
    /// No recovery needed for this channel.
    NoRecovery,
    /// Replay the latest clipboard item.
    ClipboardReplay,
    /// Resume an in-progress file share from last acknowledged offset.
    FileShareResume,
    /// Renegotiate camera codec parameters.
    CameraRenegotiate,
    /// Replay the notification backlog.
    NotificationReplay,
}

/// A serialisable snapshot of a recently-disconnected client session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tombstone {
    pub client_id: ClientId,
    pub device_id: Uuid,
    /// Unix timestamp (seconds) when this tombstone was created.
    pub created_at_secs: u64,
    /// Set of channel IDs the client was subscribed to.
    pub subscribed_channels: HashSet<u16>,
    /// Last measured round-trip time in microseconds.
    pub last_rtt_us: u64,
    /// Last measured bandwidth in bits-per-second.
    pub last_bandwidth_bps: u64,
}

impl Tombstone {
    /// Create a new tombstone, stamping `created_at_secs` from the wall clock.
    pub fn new(client_id: ClientId, device_id: Uuid, subscribed_channels: HashSet<u16>) -> Self {
        let created_at_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            client_id,
            device_id,
            created_at_secs,
            subscribed_channels,
            last_rtt_us: 0,
            last_bandwidth_bps: 0,
        }
    }

    /// Returns `true` if `now - created_at_secs >= max_age_secs` and `max_age_secs == 0`,
    /// or `now - created_at_secs > max_age_secs` otherwise.
    ///
    /// In practice: a tombstone created *right now* with `max_age_secs=0` is already expired.
    pub fn is_expired(&self, max_age_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age = now.saturating_sub(self.created_at_secs);
        if max_age_secs == 0 {
            age >= max_age_secs
        } else {
            age > max_age_secs
        }
    }
}

/// In-memory store of pending tombstones, keyed by `device_id`.
pub struct TombstoneStore {
    entries: HashMap<Uuid, Tombstone>,
    max_age_secs: u64,
}

impl TombstoneStore {
    pub fn new(max_age_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            max_age_secs,
        }
    }

    /// Insert or replace the tombstone for a device.
    pub fn insert(&mut self, tombstone: Tombstone) {
        self.entries.insert(tombstone.device_id, tombstone);
    }

    /// Remove and return the tombstone for `device_id`, but only if it has not expired.
    /// Returns `None` if no entry exists *or* if the entry is expired.
    pub fn claim_by_device(&mut self, device_id: Uuid) -> Option<Tombstone> {
        let tombstone = self.entries.remove(&device_id)?;
        if tombstone.is_expired(self.max_age_secs) {
            None
        } else {
            Some(tombstone)
        }
    }

    /// Number of tombstones currently held (including potentially expired ones).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove all expired tombstones.
    pub fn gc(&mut self) {
        let max_age = self.max_age_secs;
        self.entries.retain(|_, t| !t.is_expired(max_age));
    }

    /// Persist the store to a file as JSON.
    pub fn persist(&self, path: &std::path::Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&self.entries).map_err(std::io::Error::other)?;
        std::fs::write(path, bytes)
    }

    /// Load a store from a file, applying `gc()` immediately.
    pub fn restore(path: &std::path::Path, max_age_secs: u64) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let entries: HashMap<Uuid, Tombstone> =
            serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        let mut store = Self {
            entries,
            max_age_secs,
        };
        store.gc();
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn new_uuid() -> Uuid {
        Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext))
    }

    fn make_tombstone(device_id: Uuid) -> Tombstone {
        Tombstone::new(new_uuid(), device_id, HashSet::from([0x0001u16, 0x0002u16]))
    }

    #[test]
    fn create_and_claim_tombstone() {
        let device_id = new_uuid();
        let t = make_tombstone(device_id);
        let mut store = TombstoneStore::new(60);
        store.insert(t);
        assert_eq!(store.len(), 1);

        let claimed = store.claim_by_device(device_id);
        assert!(claimed.is_some(), "should have found tombstone");
        assert_eq!(claimed.unwrap().device_id, device_id);

        // Consumed — should be gone
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn claim_nonexistent_returns_none() {
        let mut store = TombstoneStore::new(60);
        let result = store.claim_by_device(new_uuid());
        assert!(result.is_none());
    }

    #[test]
    fn expired_tombstones_garbage_collected() {
        let device_id = new_uuid();
        let t = make_tombstone(device_id);

        // max_age_secs = 0 means anything older than 0 seconds is expired.
        let mut store = TombstoneStore::new(0);
        store.insert(t);

        // Sleep just long enough to ensure created_at < now.
        thread::sleep(Duration::from_millis(10));

        store.gc();
        assert!(store.is_empty(), "expired tombstone should have been GC'd");
    }

    #[test]
    fn channel_recovery_state_variants() {
        let variants = [
            ChannelRecoveryState::SendIdr,
            ChannelRecoveryState::AudioReset,
            ChannelRecoveryState::NoRecovery,
            ChannelRecoveryState::ClipboardReplay,
            ChannelRecoveryState::FileShareResume,
            ChannelRecoveryState::CameraRenegotiate,
            ChannelRecoveryState::NotificationReplay,
        ];
        // Ensure all 7 variants are distinct (use Debug for equality proxy).
        let names: Vec<_> = variants.iter().map(|v| format!("{v:?}")).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), 7, "all 7 variants must be distinct");
    }

    #[test]
    fn tombstone_json_roundtrip() {
        let device_id = new_uuid();
        let original = make_tombstone(device_id);
        let json = serde_json::to_string(&original).expect("serialize");
        let back: Tombstone = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.device_id, original.device_id);
        assert_eq!(back.client_id, original.client_id);
        assert_eq!(back.created_at_secs, original.created_at_secs);
        assert_eq!(back.subscribed_channels, original.subscribed_channels);
    }
}
