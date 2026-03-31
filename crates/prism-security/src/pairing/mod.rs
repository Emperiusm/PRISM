pub mod methods;

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::{decrypt_aes_gcm, encrypt_aes_gcm, hkdf_derive};
use crate::identity::DeviceIdentity;

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum PairingError {
    #[error("device already paired")]
    AlreadyPaired,
    #[error("device not found")]
    NotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
}

impl From<crate::crypto::CryptoError> for PairingError {
    fn from(e: crate::crypto::CryptoError) -> Self {
        PairingError::Crypto(e.to_string())
    }
}

// ── Permission types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelPermissions {
    pub display: Permission,
    pub input: Permission,
    pub clipboard: Permission,
    pub fileshare: Permission,
    pub notify: Permission,
    pub camera: Permission,
    pub sensor: Permission,
    pub filesystem_browse: Permission,
}

impl Default for ChannelPermissions {
    fn default() -> Self {
        Self {
            display: Permission::Allow,
            input: Permission::Allow,
            clipboard: Permission::Allow,
            fileshare: Permission::Allow,
            notify: Permission::Allow,
            camera: Permission::Ask,
            sensor: Permission::Ask,
            filesystem_browse: Permission::Ask,
        }
    }
}

// ── Pairing state / entry ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PairingState {
    Paired,
    Blocked,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingEntry {
    pub device: DeviceIdentity,
    pub state: PairingState,
    pub permissions: ChannelPermissions,
    pub paired_at: u64,
    pub last_seen: u64,
}

// ── Serialisable form of the snapshot ────────────────────────────────────────

/// What we actually write to disk: just a flat list of entries.
#[derive(Serialize, Deserialize)]
struct PersistedSnapshot {
    entries: Vec<PairingEntry>,
}

// ── Snapshot (immutable read view) ──────────────────────────────────────────

/// The live, in-memory snapshot.  The two HashMaps are redundant views of the
/// same data for O(1) lookup by either key type.
#[derive(Debug, Default, Clone)]
pub struct PairingSnapshot {
    /// keyed by the device's `current_key` (X25519 public key bytes)
    pub by_key: HashMap<[u8; 32], Arc<PairingEntry>>,
    /// keyed by the device's UUID
    pub by_device_id: HashMap<Uuid, Arc<PairingEntry>>,
}

impl PairingSnapshot {
    fn from_entries(entries: Vec<PairingEntry>) -> Self {
        let mut snap = PairingSnapshot::default();
        for e in entries {
            snap.insert(e);
        }
        snap
    }

    fn to_entries(&self) -> Vec<PairingEntry> {
        self.by_device_id.values().map(|a| (**a).clone()).collect()
    }

    fn insert(&mut self, entry: PairingEntry) {
        let arc = Arc::new(entry);
        self.by_key.insert(arc.device.current_key, Arc::clone(&arc));
        self.by_device_id.insert(arc.device.device_id, arc);
    }

    fn remove(&mut self, device_id: Uuid) -> bool {
        if let Some(entry) = self.by_device_id.remove(&device_id) {
            self.by_key.remove(&entry.device.current_key);
            true
        } else {
            false
        }
    }
}

// ── PairingStore ─────────────────────────────────────────────────────────────

pub struct PairingStore {
    /// Lock-free read path.
    current: ArcSwap<PairingSnapshot>,
    /// Serialises writes so only one mutation runs at a time.
    writer: Mutex<()>,
    /// Optional path for persisted storage.
    path: Option<PathBuf>,
    /// Optional encryption key.
    encryption_key: Option<[u8; 32]>,
}

impl PairingStore {
    /// Create an in-memory store with no persistence.
    pub fn new() -> Self {
        PairingStore {
            current: ArcSwap::from_pointee(PairingSnapshot::default()),
            writer: Mutex::new(()),
            path: None,
            encryption_key: None,
        }
    }

    /// Create a store backed by an encrypted file.
    ///
    /// If the file already exists it is loaded immediately.
    pub fn with_encrypted_file(
        path: PathBuf,
        master_key: [u8; 32],
    ) -> Result<Self, PairingError> {
        let store = PairingStore {
            current: ArcSwap::from_pointee(PairingSnapshot::default()),
            writer: Mutex::new(()),
            path: Some(path),
            encryption_key: Some(master_key),
        };
        store.load()?;
        Ok(store)
    }

    // ── Read ────────────────────────────────────────────────────────────────

    /// Return a point-in-time snapshot.  Lock-free.
    pub fn snapshot(&self) -> Arc<PairingSnapshot> {
        self.current.load_full()
    }

    // ── Internal commit ──────────────────────────────────────────────────────

    /// Publish `new_snap` and persist (must be called while holding
    /// `self.writer`).
    fn commit(&self, new_snap: PairingSnapshot) -> Result<(), PairingError> {
        self.current.store(Arc::new(new_snap));
        self.persist()
    }

    // ── Mutations ────────────────────────────────────────────────────────────

    /// Add a new entry.  Returns `AlreadyPaired` if the device UUID is already
    /// present.
    pub fn add(&self, entry: PairingEntry) -> Result<(), PairingError> {
        let _guard = self.writer.lock().unwrap();
        let mut snap = (*self.current.load_full()).clone();
        if snap.by_device_id.contains_key(&entry.device.device_id) {
            return Err(PairingError::AlreadyPaired);
        }
        snap.insert(entry);
        self.commit(snap)
    }

    /// Remove a device entirely.
    pub fn remove(&self, device_id: Uuid) -> Result<(), PairingError> {
        let _guard = self.writer.lock().unwrap();
        let mut snap = (*self.current.load_full()).clone();
        if !snap.remove(device_id) {
            return Err(PairingError::NotFound);
        }
        self.commit(snap)
    }

    /// Set a device's state to `Blocked`.
    pub fn block(&self, device_id: Uuid) -> Result<(), PairingError> {
        let _guard = self.writer.lock().unwrap();
        let mut snap = (*self.current.load_full()).clone();
        let mut entry = snap.by_device_id.get(&device_id)
            .ok_or(PairingError::NotFound)?
            .as_ref()
            .clone();
        entry.state = PairingState::Blocked;
        snap.remove(device_id);
        snap.insert(entry);
        self.commit(snap)
    }

    /// Replace the `current_key` for an existing device (key rotation).
    pub fn update_key(&self, device_id: Uuid, new_key: [u8; 32]) -> Result<(), PairingError> {
        let _guard = self.writer.lock().unwrap();
        let mut snap = (*self.current.load_full()).clone();
        let mut entry = snap.by_device_id.get(&device_id)
            .ok_or(PairingError::NotFound)?
            .as_ref()
            .clone();
        let old_key = entry.device.current_key;
        entry.device.current_key = new_key;
        snap.by_device_id.remove(&device_id);
        snap.by_key.remove(&old_key);
        snap.insert(entry);
        self.commit(snap)
    }

    // ── Persistence ──────────────────────────────────────────────────────────

    /// Persist the current snapshot to disk.  No-op when no path is set.
    pub fn persist(&self) -> Result<(), PairingError> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()),
        };
        let snap = self.current.load_full();
        let persisted = PersistedSnapshot { entries: snap.to_entries() };
        let json = serde_json::to_vec(&persisted)?;
        let data = match &self.encryption_key {
            Some(master) => {
                let file_key = hkdf_derive(master, "prism:pairing:file");
                encrypt_aes_gcm(&file_key, &json)?
            }
            None => json,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &data)?;
        Ok(())
    }

    /// Load the snapshot from disk.  No-op when no path is set or file absent.
    pub fn load(&self) -> Result<(), PairingError> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()),
        };
        if !path.exists() {
            return Ok(());
        }
        let data = std::fs::read(path)?;
        let json = match &self.encryption_key {
            Some(master) => {
                let file_key = hkdf_derive(master, "prism:pairing:file");
                decrypt_aes_gcm(&file_key, &data)?
            }
            None => data,
        };
        let persisted: PersistedSnapshot = serde_json::from_slice(&json)?;
        let snap = PairingSnapshot::from_entries(persisted.entries);
        self.current.store(Arc::new(snap));
        Ok(())
    }
}

impl Default for PairingStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;

    fn make_entry(name: &str) -> PairingEntry {
        let id = LocalIdentity::generate(name);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        PairingEntry {
            device: id.identity,
            state: PairingState::Paired,
            permissions: ChannelPermissions::default(),
            paired_at: now,
            last_seen: now,
        }
    }

    #[test]
    fn add_and_lookup_by_key() {
        let store = PairingStore::new();
        let entry = make_entry("Alice");
        let key = entry.device.current_key;
        store.add(entry).unwrap();
        let snap = store.snapshot();
        assert!(snap.by_key.contains_key(&key));
    }

    #[test]
    fn add_and_lookup_by_device_id() {
        let store = PairingStore::new();
        let entry = make_entry("Bob");
        let id = entry.device.device_id;
        store.add(entry).unwrap();
        let snap = store.snapshot();
        assert!(snap.by_device_id.contains_key(&id));
    }

    #[test]
    fn duplicate_add_rejected() {
        let store = PairingStore::new();
        let entry = make_entry("Carol");
        let entry2 = entry.clone();
        store.add(entry).unwrap();
        let err = store.add(entry2).unwrap_err();
        assert!(matches!(err, PairingError::AlreadyPaired));
    }

    #[test]
    fn remove_device() {
        let store = PairingStore::new();
        let entry = make_entry("Dave");
        let id = entry.device.device_id;
        store.add(entry).unwrap();
        store.remove(id).unwrap();
        let snap = store.snapshot();
        assert!(!snap.by_device_id.contains_key(&id));
    }

    #[test]
    fn block_device() {
        let store = PairingStore::new();
        let entry = make_entry("Eve");
        let id = entry.device.device_id;
        store.add(entry).unwrap();
        store.block(id).unwrap();
        let snap = store.snapshot();
        assert_eq!(snap.by_device_id[&id].state, PairingState::Blocked);
    }

    #[test]
    fn update_key() {
        let store = PairingStore::new();
        let entry = make_entry("Frank");
        let id = entry.device.device_id;
        let old_key = entry.device.current_key;
        store.add(entry).unwrap();
        let new_key = [99u8; 32];
        store.update_key(id, new_key).unwrap();
        let snap = store.snapshot();
        assert!(!snap.by_key.contains_key(&old_key));
        assert!(snap.by_key.contains_key(&new_key));
        assert_eq!(snap.by_device_id[&id].device.current_key, new_key);
    }

    #[test]
    fn snapshot_independence() {
        let store = PairingStore::new();
        let e1 = make_entry("Grace");
        store.add(e1).unwrap();

        // Capture snapshot before next mutation.
        let old_snap = store.snapshot();
        assert_eq!(old_snap.by_device_id.len(), 1);

        let e2 = make_entry("Heidi");
        store.add(e2).unwrap();

        // Old snapshot is unchanged.
        assert_eq!(old_snap.by_device_id.len(), 1);
        // New snapshot sees both.
        assert_eq!(store.snapshot().by_device_id.len(), 2);
    }

    #[test]
    fn encrypted_file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pairs.db");
        let master_key = [7u8; 32];

        let device_id;
        {
            let store = PairingStore::with_encrypted_file(path.clone(), master_key).unwrap();
            let entry = make_entry("Ivan");
            device_id = entry.device.device_id;
            store.add(entry).unwrap();
            assert_eq!(store.snapshot().by_device_id.len(), 1);
        }

        // Reload with the correct key.
        let store2 = PairingStore::with_encrypted_file(path.clone(), master_key).unwrap();
        assert!(store2.snapshot().by_device_id.contains_key(&device_id));

        // Wrong key must fail.
        let wrong_key = [8u8; 32];
        let result = PairingStore::with_encrypted_file(path.clone(), wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn thread_safe_concurrent_adds() {
        use std::sync::Arc as StdArc;
        use std::thread;

        let store = StdArc::new(PairingStore::new());
        let mut handles = Vec::new();
        for i in 0..4 {
            let s = StdArc::clone(&store);
            handles.push(thread::spawn(move || {
                let entry = make_entry(&format!("Device-{}", i));
                s.add(entry).unwrap();
            }));
        }
        for h in handles { h.join().unwrap(); }
        assert_eq!(store.snapshot().by_device_id.len(), 4);
    }

    #[test]
    fn default_permissions() {
        let perms = ChannelPermissions::default();
        assert_eq!(perms.clipboard, Permission::Allow);
        assert_eq!(perms.fileshare, Permission::Allow);
        assert_eq!(perms.notify, Permission::Allow);
        assert_eq!(perms.input, Permission::Allow);
        assert_eq!(perms.camera, Permission::Ask);
        assert_eq!(perms.sensor, Permission::Ask);
    }
}
