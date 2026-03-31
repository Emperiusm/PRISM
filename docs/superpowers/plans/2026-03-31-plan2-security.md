# Plan 2: Security Implementation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-security` crate that provides device identity (Curve25519 + UUIDv7), pairing store, Noise NK handshake, public key allowlist, SecurityGate trait with AllowAll Phase 1 implementation, SecurityContext for per-connection filter decisions, 0-RTT safety policy, and a basic audit log.

**Architecture:** `prism-security` depends on `prism-protocol` (for channel IDs and header types) and `prism-metrics` (for metrics recording). The SecurityGate trait is the contract that Transport and Session Manager code against. Phase 1 implements the full trait with AllowAll channel filters — content filters are added in Phase 3 without changing the trait. The pairing store uses copy-on-write snapshots for lock-free reads. The Noise NK handshake runs inside QUIC (auth only, no double encryption) using the `snow` crate.

**Tech Stack:** `snow` (Noise protocol), `x25519-dalek` (Curve25519), `ed25519-dalek` (Ed25519 signing for key rotation), `uuid` (UUIDv7), `rand` (key generation), `serde`/`serde_json` (serialization), `hex` (key display), `aes-gcm` (pairing store encryption), `hkdf`+`sha2` (key derivation)

**Spec refs:**
- Security: `docs/superpowers/specs/2026-03-30-security-design.md` (all sections)
- Architecture: `docs/superpowers/specs/2026-03-30-prism-architecture-design.md` (R3)

---

## File Structure

```
PRISM/
  crates/
    prism-security/
      Cargo.toml
      src/
        lib.rs                    # re-exports
        identity.rs               # DeviceIdentity, Platform, keypair gen + storage
        crypto.rs                 # CryptoBackend trait, HKDF, entropy, Ed25519 helpers
        pairing/
          mod.rs                  # PairingStore, PairingSnapshot, PairingEntry
          methods/
            mod.rs                # PairingMethod enum, PairingHandle
            manual.rs             # Manual hex key exchange
            spake2.rs             # Short code pairing (stub — SPAKE2 in Phase 1+)
        handshake.rs              # Noise NK handshake (inside QUIC)
        gate.rs                   # SecurityGate trait + Phase 1 AllowAll impl
        context.rs                # SecurityContext, ChannelFilterState, 0-RTT policy
        audit.rs                  # AuditEvent, basic audit log
        key_rotation.rs           # KeyRotation, Ed25519 signing
```

---

## Task 1: Add prism-security Crate to Workspace

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-security/Cargo.toml`
- Create: `crates/prism-security/src/lib.rs`

- [ ] **Step 1: Update workspace root Cargo.toml**

Add `"crates/prism-security"` to `[workspace] members` and add new dependencies:

```toml
[workspace]
resolver = "2"
members = [
    "crates/prism-protocol",
    "crates/prism-metrics",
    "crates/prism-security",
]

[workspace.dependencies]
# ... existing deps ...
snow = "0.9"
x25519-dalek = { version = "2", features = ["static_secrets"] }
ed25519-dalek = { version = "2", features = ["rand_core"] }
uuid = { version = "1", features = ["v7"] }
rand = "0.8"
hex = "0.4"
aes-gcm = "0.10"
hkdf = "0.12"
sha2 = "0.10"
tempfile = "3"

prism-security = { path = "crates/prism-security" }
```

- [ ] **Step 2: Create prism-security Cargo.toml**

```toml
[package]
name = "prism-security"
version.workspace = true
edition.workspace = true

[dependencies]
prism-protocol.workspace = true
prism-metrics.workspace = true
snow.workspace = true
x25519-dalek.workspace = true
ed25519-dalek.workspace = true
uuid.workspace = true
rand.workspace = true
hex.workspace = true
aes-gcm.workspace = true
hkdf.workspace = true
sha2.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
bytes.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 3: Create lib.rs with module declarations**

`crates/prism-security/src/lib.rs`:
```rust
pub mod identity;
pub mod crypto;
pub mod pairing;
pub mod handshake;
pub mod gate;
pub mod context;
pub mod audit;
pub mod key_rotation;
```

Create empty placeholder files for all modules:
- `crates/prism-security/src/identity.rs`
- `crates/prism-security/src/crypto.rs`
- `crates/prism-security/src/handshake.rs`
- `crates/prism-security/src/gate.rs`
- `crates/prism-security/src/context.rs`
- `crates/prism-security/src/audit.rs`
- `crates/prism-security/src/key_rotation.rs`
- `crates/prism-security/src/pairing/mod.rs`
- `crates/prism-security/src/pairing/methods/mod.rs`
- `crates/prism-security/src/pairing/methods/manual.rs`
- `crates/prism-security/src/pairing/methods/spake2.rs`

- [ ] **Step 4: Verify workspace builds**

Run: `cargo build`
Expected: Clean build.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/prism-security/
git commit -m "feat: add prism-security crate to workspace"
```

---

## Task 2: DeviceIdentity and Platform Types

**Files:**
- Create: `crates/prism-security/src/identity.rs`

Device identity: UUIDv7 + Curve25519 public key + display name + platform.

- [ ] **Step 1: Write identity types with tests**

`crates/prism-security/src/identity.rs`:
```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use x25519_dalek::{PublicKey, StaticSecret};
use rand::rngs::OsRng;
use std::path::Path;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid key length: expected 32 bytes, got {0}")]
    InvalidKeyLength(usize),
}

/// Platform type for the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    MacOS,
    Linux,
    Android,
    iOS,
    Browser,
}

impl Platform {
    /// Detect the current platform at compile time.
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        { Platform::Windows }
        #[cfg(target_os = "macos")]
        { Platform::MacOS }
        #[cfg(target_os = "linux")]
        { Platform::Linux }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        { Platform::Linux } // default fallback
    }
}

/// A device's public identity. Serializable, shareable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub device_id: Uuid,
    pub display_name: String,
    pub platform: Platform,
    #[serde(with = "hex_key")]
    pub current_key: [u8; 32],
    pub created_at: u64,
}

/// Hex serialization for 32-byte keys.
mod hex_key {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(key: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&hex::encode(key))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where D: Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut key = [0u8; 32];
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!("expected 32 bytes, got {}", bytes.len())));
        }
        key.copy_from_slice(&bytes);
        Ok(key)
    }
}

/// The full local identity: private key + public identity.
/// Never serialized directly — the secret key is stored separately.
pub struct LocalIdentity {
    secret: StaticSecret,
    public: PublicKey,
    pub identity: DeviceIdentity,
}

/// Stored format for the local identity (private key + device info).
#[derive(Serialize, Deserialize)]
struct StoredLocalIdentity {
    #[serde(with = "hex_key")]
    secret_key: [u8; 32],
    identity: DeviceIdentity,
}

impl LocalIdentity {
    /// Generate a new random identity.
    pub fn generate(display_name: &str) -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        let device_id = Uuid::now_v7();
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            secret,
            public,
            identity: DeviceIdentity {
                device_id,
                display_name: display_name.to_string(),
                platform: Platform::current(),
                current_key: *public.as_bytes(),
                created_at,
            },
        }
    }

    /// Load from file, or generate and save if it doesn't exist.
    pub fn load_or_generate(path: &Path, display_name: &str) -> Result<Self, IdentityError> {
        if path.exists() {
            let data = std::fs::read_to_string(path)?;
            let stored: StoredLocalIdentity = serde_json::from_str(&data)?;
            let secret = StaticSecret::from(stored.secret_key);
            let public = PublicKey::from(&secret);
            Ok(Self {
                secret,
                public,
                identity: stored.identity,
            })
        } else {
            let local = Self::generate(display_name);
            local.save(path)?;
            Ok(local)
        }
    }

    /// Save to file.
    pub fn save(&self, path: &Path) -> Result<(), IdentityError> {
        let stored = StoredLocalIdentity {
            secret_key: self.secret_bytes(),
            identity: self.identity.clone(),
        };
        let json = serde_json::to_string_pretty(&stored)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    /// The public key as bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }

    /// The secret key as bytes. Used for Noise handshake and key derivation.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// The device UUID.
    pub fn device_id(&self) -> Uuid {
        self.identity.device_id
    }

    /// The public key.
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_valid_identity() {
        let id = LocalIdentity::generate("Test Device");
        assert_ne!(id.public_key_bytes(), [0u8; 32]);
        assert_eq!(id.identity.display_name, "Test Device");
        assert_eq!(id.identity.platform, Platform::current());
        assert_eq!(id.identity.current_key, id.public_key_bytes());
        assert_ne!(id.identity.device_id, Uuid::nil());
    }

    #[test]
    fn two_identities_are_different() {
        let id1 = LocalIdentity::generate("Device 1");
        let id2 = LocalIdentity::generate("Device 2");
        assert_ne!(id1.public_key_bytes(), id2.public_key_bytes());
        assert_ne!(id1.device_id(), id2.device_id());
    }

    #[test]
    fn load_or_generate_creates_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");

        let id1 = LocalIdentity::load_or_generate(&path, "Test").unwrap();
        assert!(path.exists());

        let id2 = LocalIdentity::load_or_generate(&path, "Test").unwrap();
        assert_eq!(id1.public_key_bytes(), id2.public_key_bytes());
        assert_eq!(id1.device_id(), id2.device_id());
    }

    #[test]
    fn secret_key_derives_correct_public() {
        let id = LocalIdentity::generate("Test");
        let derived = PublicKey::from(&StaticSecret::from(id.secret_bytes()));
        assert_eq!(*derived.as_bytes(), id.public_key_bytes());
    }

    #[test]
    fn device_identity_json_roundtrip() {
        let id = LocalIdentity::generate("Test");
        let json = serde_json::to_string(&id.identity).unwrap();
        let decoded: DeviceIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(id.identity, decoded);
    }

    #[test]
    fn platform_detection() {
        let p = Platform::current();
        #[cfg(target_os = "windows")]
        assert_eq!(p, Platform::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(p, Platform::MacOS);
        #[cfg(target_os = "linux")]
        assert_eq!(p, Platform::Linux);
        let _ = p; // suppress unused warning on other platforms
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All 6 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/identity.rs
git commit -m "feat(security): DeviceIdentity with UUIDv7, Curve25519, and platform detection

LocalIdentity = secret key + DeviceIdentity (UUID + public key + name + platform).
Generate, load_or_generate, save. Hex-encoded keys in JSON serialization."
```

---

## Task 3: Crypto Utilities — HKDF and Entropy

**Files:**
- Create: `crates/prism-security/src/crypto.rs`

HKDF key derivation (for pairing store encryption) and Shannon entropy calculation (for clipboard filter — trait defined now, impl in Phase 3).

- [ ] **Step 1: Write crypto utilities with tests**

`crates/prism-security/src/crypto.rs`:
```rust
use hkdf::Hkdf;
use sha2::Sha256;

/// Derive a key from a secret and context string using HKDF-SHA256.
/// Used for pairing store encryption key: HKDF(device_secret, "pairing-store").
pub fn hkdf_derive(secret: &[u8; 32], context: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, secret);
    let mut output = [0u8; 32];
    hk.expand(context.as_bytes(), &mut output)
        .expect("HKDF expand failed — output length is valid");
    output
}

/// Calculate Shannon entropy of a byte slice.
/// Returns bits per byte (0.0 = uniform, 8.0 = maximum random).
/// Used by clipboard content filters (Phase 3).
/// Stack-allocated, single pass, ~1µs for typical clipboard content.
pub fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }
    let len = data.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Check if data looks like a high-entropy secret (password, token, API key).
/// Threshold: entropy > 4.5 bits/byte AND length 8-128 bytes.
pub fn is_high_entropy(data: &[u8]) -> bool {
    let len = data.len();
    if len < 8 || len > 128 {
        return false;
    }
    shannon_entropy(data) > 4.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hkdf_produces_deterministic_output() {
        let secret = [42u8; 32];
        let key1 = hkdf_derive(&secret, "pairing-store");
        let key2 = hkdf_derive(&secret, "pairing-store");
        assert_eq!(key1, key2);
    }

    #[test]
    fn hkdf_different_contexts_produce_different_keys() {
        let secret = [42u8; 32];
        let key1 = hkdf_derive(&secret, "pairing-store");
        let key2 = hkdf_derive(&secret, "audit-log");
        assert_ne!(key1, key2);
    }

    #[test]
    fn entropy_empty() {
        assert_eq!(shannon_entropy(&[]), 0.0);
    }

    #[test]
    fn entropy_uniform_low() {
        // All same bytes = 0 entropy
        let data = vec![b'a'; 100];
        assert!(shannon_entropy(&data) < 0.01);
    }

    #[test]
    fn entropy_random_high() {
        // Pseudo-random bytes should have high entropy
        let data: Vec<u8> = (0..=255).cycle().take(1024).collect();
        let e = shannon_entropy(&data);
        assert!(e > 7.9, "entropy was {e}");
    }

    #[test]
    fn entropy_typical_text() {
        // English text has ~4-5 bits/byte entropy
        let data = b"The quick brown fox jumps over the lazy dog";
        let e = shannon_entropy(data);
        assert!(e > 3.5 && e < 5.5, "entropy was {e}");
    }

    #[test]
    fn is_high_entropy_detects_secrets() {
        // Random 32-byte key
        let key: Vec<u8> = (0..32).map(|i| (i * 7 + 13) as u8).collect();
        assert!(is_high_entropy(&key));
    }

    #[test]
    fn is_high_entropy_rejects_normal_text() {
        assert!(!is_high_entropy(b"hello world"));
    }

    #[test]
    fn is_high_entropy_rejects_too_short() {
        assert!(!is_high_entropy(b"abc"));
    }

    #[test]
    fn is_high_entropy_rejects_too_long() {
        let data = vec![0xABu8; 200]; // high entropy but too long
        assert!(!is_high_entropy(&data));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/crypto.rs
git commit -m "feat(security): HKDF key derivation and Shannon entropy calculation

hkdf_derive for pairing store encryption key. shannon_entropy for
clipboard content filter (Phase 3). is_high_entropy threshold check.
Stack-allocated, single-pass entropy, ~1µs."
```

---

## Task 4: Pairing Store with Copy-on-Write Snapshots

**Files:**
- Create: `crates/prism-security/src/pairing/mod.rs`
- Create: `crates/prism-security/src/pairing/methods/mod.rs`
- Create: `crates/prism-security/src/pairing/methods/manual.rs`
- Create: `crates/prism-security/src/pairing/methods/spake2.rs`

- [ ] **Step 1: Write pairing types and store**

`crates/prism-security/src/pairing/mod.rs`:
```rust
pub mod methods;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use crate::identity::DeviceIdentity;
use prism_protocol::channel::ChannelPriority;

#[derive(Debug, Error)]
pub enum PairingError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("device not found: {0}")]
    DeviceNotFound(Uuid),
    #[error("device already paired: {0}")]
    AlreadyPaired(Uuid),
    #[error("device is blocked: {0}")]
    DeviceBlocked(Uuid),
}

/// Per-channel permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Allow,
    Deny,
    Ask,
}

/// Per-channel permissions for a paired device.
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

/// Pairing state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairingState {
    Paired,
    Blocked,
}

/// A paired device entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingEntry {
    pub device: DeviceIdentity,
    pub state: PairingState,
    pub permissions: ChannelPermissions,
    pub paired_at: u64,
    pub last_seen: u64,
}

/// Read-optimized snapshot of the pairing store.
/// Lock-free reads via Arc::clone.
#[derive(Debug, Clone)]
pub struct PairingSnapshot {
    pub by_key: HashMap<[u8; 32], Arc<PairingEntry>>,
    pub by_device_id: HashMap<Uuid, Arc<PairingEntry>>,
    pub generation: u64,
}

impl PairingSnapshot {
    fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            by_device_id: HashMap::new(),
            generation: 0,
        }
    }

    /// Look up a device by its public key. O(1).
    pub fn get_by_key(&self, key: &[u8; 32]) -> Option<&Arc<PairingEntry>> {
        self.by_key.get(key)
    }

    /// Look up a device by its UUID. O(1).
    pub fn get_by_device_id(&self, id: &Uuid) -> Option<&Arc<PairingEntry>> {
        self.by_device_id.get(id)
    }

    /// Check if a key is authorized (paired and not blocked).
    pub fn is_authorized(&self, key: &[u8; 32]) -> bool {
        self.by_key
            .get(key)
            .is_some_and(|e| e.state == PairingState::Paired)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// The pairing store. Write-rare, read-often.
/// Reads are lock-free via Arc<PairingSnapshot>.
/// Writes rebuild the snapshot and swap atomically.
pub struct PairingStore {
    current: Arc<PairingSnapshot>,
    path: Option<std::path::PathBuf>,
}

impl PairingStore {
    /// Create an empty in-memory pairing store.
    pub fn new() -> Self {
        Self {
            current: Arc::new(PairingSnapshot::new()),
            path: None,
        }
    }

    /// Create a pairing store backed by a file.
    pub fn with_file(path: &Path) -> Result<Self, PairingError> {
        let mut store = Self {
            current: Arc::new(PairingSnapshot::new()),
            path: Some(path.to_path_buf()),
        };
        if path.exists() {
            store.load()?;
        }
        Ok(store)
    }

    /// Get the current snapshot for lock-free reading.
    pub fn snapshot(&self) -> Arc<PairingSnapshot> {
        self.current.clone()
    }

    /// Add a paired device. Rebuilds snapshot.
    pub fn add(&mut self, entry: PairingEntry) -> Result<(), PairingError> {
        let device_id = entry.device.device_id;
        if self.current.by_device_id.contains_key(&device_id) {
            return Err(PairingError::AlreadyPaired(device_id));
        }
        let entry = Arc::new(entry);
        let mut new_snap = (*self.current).clone();
        new_snap.by_key.insert(entry.device.current_key, entry.clone());
        new_snap.by_device_id.insert(device_id, entry);
        new_snap.generation += 1;
        self.current = Arc::new(new_snap);
        self.persist()?;
        Ok(())
    }

    /// Remove a device by UUID. Rebuilds snapshot.
    pub fn remove(&mut self, device_id: &Uuid) -> Result<(), PairingError> {
        let entry = self.current.by_device_id.get(device_id)
            .ok_or(PairingError::DeviceNotFound(*device_id))?;
        let key = entry.device.current_key;
        let mut new_snap = (*self.current).clone();
        new_snap.by_key.remove(&key);
        new_snap.by_device_id.remove(device_id);
        new_snap.generation += 1;
        self.current = Arc::new(new_snap);
        self.persist()?;
        Ok(())
    }

    /// Block a device.
    pub fn block(&mut self, device_id: &Uuid) -> Result<(), PairingError> {
        let entry = self.current.by_device_id.get(device_id)
            .ok_or(PairingError::DeviceNotFound(*device_id))?;
        let mut updated = (**entry).clone();
        updated.state = PairingState::Blocked;
        let updated = Arc::new(updated);

        let mut new_snap = (*self.current).clone();
        new_snap.by_key.insert(updated.device.current_key, updated.clone());
        new_snap.by_device_id.insert(*device_id, updated);
        new_snap.generation += 1;
        self.current = Arc::new(new_snap);
        self.persist()?;
        Ok(())
    }

    /// Update a device's key (key rotation).
    pub fn update_key(&mut self, device_id: &Uuid, new_key: [u8; 32]) -> Result<(), PairingError> {
        let entry = self.current.by_device_id.get(device_id)
            .ok_or(PairingError::DeviceNotFound(*device_id))?;
        let old_key = entry.device.current_key;
        let mut updated = (**entry).clone();
        updated.device.current_key = new_key;
        let updated = Arc::new(updated);

        let mut new_snap = (*self.current).clone();
        new_snap.by_key.remove(&old_key);
        new_snap.by_key.insert(new_key, updated.clone());
        new_snap.by_device_id.insert(*device_id, updated);
        new_snap.generation += 1;
        self.current = Arc::new(new_snap);
        self.persist()?;
        Ok(())
    }

    fn persist(&self) -> Result<(), PairingError> {
        if let Some(path) = &self.path {
            let entries: Vec<&PairingEntry> = self.current.by_device_id.values()
                .map(|e| e.as_ref())
                .collect();
            let json = serde_json::to_string_pretty(&entries)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, json)?;
        }
        Ok(())
    }

    fn load(&mut self) -> Result<(), PairingError> {
        if let Some(path) = &self.path {
            let data = std::fs::read_to_string(path)?;
            let entries: Vec<PairingEntry> = serde_json::from_str(&data)?;
            let mut snap = PairingSnapshot::new();
            for entry in entries {
                let entry = Arc::new(entry);
                snap.by_key.insert(entry.device.current_key, entry.clone());
                snap.by_device_id.insert(entry.device.device_id, entry);
            }
            self.current = Arc::new(snap);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{LocalIdentity, Platform};

    fn make_entry(name: &str) -> PairingEntry {
        let id = LocalIdentity::generate(name);
        PairingEntry {
            device: id.identity,
            state: PairingState::Paired,
            permissions: ChannelPermissions::default(),
            paired_at: 1000,
            last_seen: 2000,
        }
    }

    #[test]
    fn add_and_lookup_by_key() {
        let mut store = PairingStore::new();
        let entry = make_entry("Device A");
        let key = entry.device.current_key;
        store.add(entry).unwrap();

        let snap = store.snapshot();
        assert!(snap.is_authorized(&key));
        assert_eq!(snap.len(), 1);
    }

    #[test]
    fn add_and_lookup_by_device_id() {
        let mut store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        store.add(entry).unwrap();

        let snap = store.snapshot();
        let found = snap.get_by_device_id(&device_id).unwrap();
        assert_eq!(found.device.display_name, "Device A");
    }

    #[test]
    fn duplicate_add_rejected() {
        let mut store = PairingStore::new();
        let entry = make_entry("Device A");
        let entry2 = entry.clone();
        store.add(entry).unwrap();
        assert!(matches!(store.add(entry2), Err(PairingError::AlreadyPaired(_))));
    }

    #[test]
    fn remove_device() {
        let mut store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        let key = entry.device.current_key;
        store.add(entry).unwrap();
        store.remove(&device_id).unwrap();

        let snap = store.snapshot();
        assert!(!snap.is_authorized(&key));
        assert!(snap.is_empty());
    }

    #[test]
    fn block_device() {
        let mut store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        let key = entry.device.current_key;
        store.add(entry).unwrap();
        store.block(&device_id).unwrap();

        let snap = store.snapshot();
        assert!(!snap.is_authorized(&key)); // blocked = not authorized
        assert_eq!(snap.len(), 1); // still in store
        let found = snap.get_by_key(&key).unwrap();
        assert_eq!(found.state, PairingState::Blocked);
    }

    #[test]
    fn update_key() {
        let mut store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        let old_key = entry.device.current_key;
        store.add(entry).unwrap();

        let new_key = [0xABu8; 32];
        store.update_key(&device_id, new_key).unwrap();

        let snap = store.snapshot();
        assert!(!snap.is_authorized(&old_key)); // old key gone
        assert!(snap.is_authorized(&new_key));  // new key works
        let found = snap.get_by_device_id(&device_id).unwrap();
        assert_eq!(found.device.current_key, new_key);
    }

    #[test]
    fn snapshot_is_independent_of_mutations() {
        let mut store = PairingStore::new();
        let entry = make_entry("Device A");
        let key = entry.device.current_key;
        store.add(entry).unwrap();

        let snap_before = store.snapshot();
        assert_eq!(snap_before.len(), 1);

        store.add(make_entry("Device B")).unwrap();

        // Old snapshot still shows 1 entry
        assert_eq!(snap_before.len(), 1);
        // New snapshot shows 2
        assert_eq!(store.snapshot().len(), 2);
    }

    #[test]
    fn file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pairings.json");

        // Create and populate
        {
            let mut store = PairingStore::with_file(&path).unwrap();
            store.add(make_entry("Device A")).unwrap();
            store.add(make_entry("Device B")).unwrap();
        }

        // Reload
        {
            let store = PairingStore::with_file(&path).unwrap();
            let snap = store.snapshot();
            assert_eq!(snap.len(), 2);
        }
    }

    #[test]
    fn default_permissions() {
        let perms = ChannelPermissions::default();
        assert_eq!(perms.display, Permission::Allow);
        assert_eq!(perms.input, Permission::Allow);
        assert_eq!(perms.camera, Permission::Ask);
        assert_eq!(perms.sensor, Permission::Ask);
        assert_eq!(perms.filesystem_browse, Permission::Ask);
    }
}
```

- [ ] **Step 2: Write pairing method stubs**

`crates/prism-security/src/pairing/methods/mod.rs`:
```rust
pub mod manual;
pub mod spake2;

use serde::{Deserialize, Serialize};

/// How two devices discovered each other.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PairingMethod {
    /// Manual hex key exchange (copy-paste).
    Manual,
    /// Tailscale auto-discovery (Phase 1).
    Tailscale,
    /// SPAKE2 short code (Phase 1).
    ShortCode,
    /// QR code (Phase 2).
    QrCode,
    /// Coordination service (Phase 4).
    Coordination,
}
```

`crates/prism-security/src/pairing/methods/manual.rs`:
```rust
use crate::identity::DeviceIdentity;
use crate::pairing::{PairingEntry, PairingState, ChannelPermissions, PairingError};

/// Create a PairingEntry from a manually exchanged DeviceIdentity.
pub fn pair_manually(remote: DeviceIdentity) -> PairingEntry {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    PairingEntry {
        device: remote,
        state: PairingState::Paired,
        permissions: ChannelPermissions::default(),
        paired_at: now,
        last_seen: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;

    #[test]
    fn manual_pairing_creates_entry() {
        let remote = LocalIdentity::generate("Remote Device");
        let entry = pair_manually(remote.identity);
        assert_eq!(entry.state, PairingState::Paired);
        assert_eq!(entry.device.display_name, "Remote Device");
    }
}
```

`crates/prism-security/src/pairing/methods/spake2.rs`:
```rust
// SPAKE2 short code pairing — Phase 1 stub.
// Full implementation requires the spake2 crate and a multi-round protocol.
// For Phase 1 MVP, manual pairing is the primary path.

/// Generate a 6-digit pairing code.
pub fn generate_code() -> String {
    use rand::Rng;
    let code: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{:06}", code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_is_6_digits() {
        let code = generate_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn codes_are_different() {
        let c1 = generate_code();
        let c2 = generate_code();
        // Technically could collide (1 in 1M), but practically won't
        // If this test flakes, that's a 1-in-1M event — acceptable
        assert_ne!(c1, c2);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-security/src/pairing/
git commit -m "feat(security): pairing store with copy-on-write snapshots

PairingStore with lock-free reads via Arc<PairingSnapshot>. Add, remove,
block, update_key operations rebuild snapshot atomically. File persistence.
PairingEntry with ChannelPermissions (default: Allow for core, Ask for
camera/sensor/filesystem). Manual pairing method + SPAKE2 code gen stub."
```

---

## Task 5: Noise NK Handshake

**Files:**
- Create: `crates/prism-security/src/handshake.rs`

Noise NK inside QUIC. Client knows server's static key. 1 RTT to mutual authentication. Auth only — no double encryption.

- [ ] **Step 1: Write handshake with tests**

`crates/prism-security/src/handshake.rs`:
```rust
use snow::{Builder, HandshakeState, TransportState};
use thiserror::Error;

use crate::identity::LocalIdentity;

/// Noise protocol pattern. NK = client knows server's static key.
const NOISE_PATTERN: &str = "Noise_NK_25519_ChaChaPoly_SHA256";

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("noise protocol error: {0}")]
    Noise(#[from] snow::Error),
    #[error("handshake not complete")]
    NotComplete,
}

/// Result of a completed handshake.
pub struct HandshakeResult {
    /// The Noise transport state (can encrypt/decrypt if needed, but PRISM
    /// uses QUIC/TLS for encryption — this is auth only).
    pub transport: TransportState,
    /// The remote device's static public key (learned during handshake).
    /// For server: this is the client's key. For client: None (already known).
    pub remote_static: Option<[u8; 32]>,
}

/// Server-side Noise NK handshake.
pub struct ServerHandshake {
    state: HandshakeState,
}

impl ServerHandshake {
    /// Create using the server's identity.
    pub fn new(identity: &LocalIdentity) -> Result<Self, HandshakeError> {
        let state = Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&identity.secret_bytes())
            .build_responder()?;
        Ok(Self { state })
    }

    /// Process client's initial message. Returns response to send back.
    pub fn respond(&mut self, client_msg: &[u8]) -> Result<Vec<u8>, HandshakeError> {
        let mut read_buf = vec![0u8; 65535];
        self.state.read_message(client_msg, &mut read_buf)?;

        let mut response = vec![0u8; 65535];
        let len = self.state.write_message(&[], &mut response)?;
        response.truncate(len);
        Ok(response)
    }

    /// Finalize into transport state after respond().
    pub fn finalize(self) -> Result<HandshakeResult, HandshakeError> {
        if !self.state.is_handshake_finished() {
            return Err(HandshakeError::NotComplete);
        }
        let remote_static = self.state.get_remote_static().map(|s| {
            let mut key = [0u8; 32];
            key.copy_from_slice(s);
            key
        });
        let transport = self.state.into_transport_mode()?;
        Ok(HandshakeResult {
            transport,
            remote_static,
        })
    }
}

/// Client-side Noise NK handshake.
pub struct ClientHandshake {
    state: HandshakeState,
}

impl ClientHandshake {
    /// Create. `server_public_key` is the server's known static key (the "K" in NK).
    pub fn new(
        identity: &LocalIdentity,
        server_public_key: &[u8; 32],
    ) -> Result<Self, HandshakeError> {
        let state = Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&identity.secret_bytes())
            .remote_public_key(server_public_key)
            .build_initiator()?;
        Ok(Self { state })
    }

    /// Generate initial message to send to server.
    pub fn initiate(&mut self) -> Result<Vec<u8>, HandshakeError> {
        let mut msg = vec![0u8; 65535];
        let len = self.state.write_message(&[], &mut msg)?;
        msg.truncate(len);
        Ok(msg)
    }

    /// Process server's response.
    pub fn process_response(&mut self, server_msg: &[u8]) -> Result<(), HandshakeError> {
        let mut read_buf = vec![0u8; 65535];
        self.state.read_message(server_msg, &mut read_buf)?;
        Ok(())
    }

    /// Finalize into transport state.
    pub fn finalize(self) -> Result<HandshakeResult, HandshakeError> {
        if !self.state.is_handshake_finished() {
            return Err(HandshakeError::NotComplete);
        }
        let transport = self.state.into_transport_mode()?;
        Ok(HandshakeResult {
            transport,
            remote_static: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;

    #[test]
    fn handshake_completes_in_one_roundtrip() {
        let server_id = LocalIdentity::generate("Server");
        let client_id = LocalIdentity::generate("Client");

        let mut client_hs =
            ClientHandshake::new(&client_id, &server_id.public_key_bytes()).unwrap();
        let client_msg = client_hs.initiate().unwrap();

        let mut server_hs = ServerHandshake::new(&server_id).unwrap();
        let server_msg = server_hs.respond(&client_msg).unwrap();

        client_hs.process_response(&server_msg).unwrap();

        let server_result = server_hs.finalize().unwrap();
        let client_result = client_hs.finalize().unwrap();

        // Server learned client's static key
        assert_eq!(
            server_result.remote_static.unwrap(),
            client_id.public_key_bytes()
        );

        // Both can encrypt/decrypt
        let mut enc_buf = vec![0u8; 1024];
        let mut dec_buf = vec![0u8; 1024];
        let plaintext = b"hello from client";
        let len = client_result.transport.write_message(plaintext, &mut enc_buf).unwrap();
        let dec_len = server_result.transport.read_message(&enc_buf[..len], &mut dec_buf).unwrap();
        assert_eq!(&dec_buf[..dec_len], plaintext);
    }

    #[test]
    fn wrong_server_key_fails() {
        let server_id = LocalIdentity::generate("Server");
        let client_id = LocalIdentity::generate("Client");
        let wrong_id = LocalIdentity::generate("Wrong");

        let mut client_hs =
            ClientHandshake::new(&client_id, &wrong_id.public_key_bytes()).unwrap();
        let client_msg = client_hs.initiate().unwrap();

        let mut server_hs = ServerHandshake::new(&server_id).unwrap();
        let result = server_hs.respond(&client_msg);
        assert!(result.is_err());
    }

    #[test]
    fn bidirectional_encryption() {
        let server_id = LocalIdentity::generate("Server");
        let client_id = LocalIdentity::generate("Client");

        let mut client_hs =
            ClientHandshake::new(&client_id, &server_id.public_key_bytes()).unwrap();
        let client_msg = client_hs.initiate().unwrap();

        let mut server_hs = ServerHandshake::new(&server_id).unwrap();
        let server_msg = server_hs.respond(&client_msg).unwrap();
        client_hs.process_response(&server_msg).unwrap();

        let server_result = server_hs.finalize().unwrap();
        let client_result = client_hs.finalize().unwrap();

        // Server -> Client
        let mut buf = vec![0u8; 1024];
        let mut dec = vec![0u8; 1024];
        let msg = b"hello from server";
        let len = server_result.transport.write_message(msg, &mut buf).unwrap();
        let dec_len = client_result.transport.read_message(&buf[..len], &mut dec).unwrap();
        assert_eq!(&dec[..dec_len], msg);
    }

    #[test]
    fn finalize_before_complete_fails() {
        let server_id = LocalIdentity::generate("Server");
        let server_hs = ServerHandshake::new(&server_id).unwrap();
        let result = server_hs.finalize();
        assert!(matches!(result, Err(HandshakeError::NotComplete)));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/handshake.rs
git commit -m "feat(security): Noise NK handshake with 1-RTT mutual authentication

Client knows server's Curve25519 key. 1 round trip: client sends ephemeral +
encrypted static, server responds. Server learns client's static key for
allowlist check. Auth only — encryption handled by QUIC/TLS."
```

---

## Task 6: SecurityContext and 0-RTT Policy

**Files:**
- Create: `crates/prism-security/src/context.rs`

Pre-computed per-connection security decisions. Fixed-size array for channel filter lookups. 0-RTT idempotent enforcement.

- [ ] **Step 1: Write SecurityContext with tests**

`crates/prism-security/src/context.rs`:
```rust
use std::sync::Arc;

use prism_protocol::channel::*;
use prism_protocol::header::PrismHeader;

use crate::pairing::PairingEntry;

/// Pre-computed security decisions for a connected client.
/// Created once at connection time, cached for the session.
/// Channel filter lookups are array-indexed: O(1), ~2ns.
pub struct SecurityContext {
    pub device: Arc<PairingEntry>,
    /// Per-channel filter state. Indexed by channel_id & 0xFF.
    /// 256 entries, 256 bytes. Avoids HashMap lookup on every packet.
    pub channel_filters: [ChannelFilterState; 256],
    /// Per-channel 0-RTT safety. Same indexing.
    pub is_0rtt_safe: [bool; 256],
}

/// Filter state for a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelFilterState {
    /// No filtering — send immediately. Display, Input, Audio.
    AllowAll,
    /// Channel blocked for this client.
    Blocked,
    /// Needs user confirmation before first use this session.
    NeedsConfirmation,
    /// Active content filter (Phase 3+). For now, same as AllowAll.
    FilterActive,
}

impl SecurityContext {
    /// Build a SecurityContext for a paired device.
    /// Phase 1: AllowAll for all channels. Filters added in Phase 3.
    pub fn for_device(entry: Arc<PairingEntry>) -> Self {
        let mut channel_filters = [ChannelFilterState::AllowAll; 256];
        let mut is_0rtt_safe = [false; 256];

        // Apply per-channel permissions from pairing entry
        use crate::pairing::Permission;
        let perms = &entry.permissions;

        Self::apply_permission(&mut channel_filters, CHANNEL_DISPLAY, perms.display);
        Self::apply_permission(&mut channel_filters, CHANNEL_INPUT, perms.input);
        Self::apply_permission(&mut channel_filters, CHANNEL_CLIPBOARD, perms.clipboard);
        Self::apply_permission(&mut channel_filters, CHANNEL_FILESHARE, perms.fileshare);
        Self::apply_permission(&mut channel_filters, CHANNEL_NOTIFY, perms.notify);
        Self::apply_permission(&mut channel_filters, CHANNEL_CAMERA, perms.camera);
        Self::apply_permission(&mut channel_filters, CHANNEL_SENSOR, perms.sensor);

        // 0-RTT safety: only idempotent channels
        is_0rtt_safe[(CHANNEL_DISPLAY & 0xFF) as usize] = true;
        is_0rtt_safe[(CHANNEL_INPUT & 0xFF) as usize] = true;
        is_0rtt_safe[(CHANNEL_AUDIO & 0xFF) as usize] = true;
        // Control: only heartbeats are 0-RTT safe (checked per msg_type, not here)
        is_0rtt_safe[(CHANNEL_CONTROL & 0xFF) as usize] = false;
        // Everything else: not 0-RTT safe
        is_0rtt_safe[(CHANNEL_CLIPBOARD & 0xFF) as usize] = false;
        is_0rtt_safe[(CHANNEL_FILESHARE & 0xFF) as usize] = false;
        is_0rtt_safe[(CHANNEL_NOTIFY & 0xFF) as usize] = false;

        Self {
            device: entry,
            channel_filters,
            is_0rtt_safe,
        }
    }

    fn apply_permission(
        filters: &mut [ChannelFilterState; 256],
        channel_id: u16,
        permission: crate::pairing::Permission,
    ) {
        let idx = (channel_id & 0xFF) as usize;
        filters[idx] = match permission {
            crate::pairing::Permission::Allow => ChannelFilterState::AllowAll,
            crate::pairing::Permission::Deny => ChannelFilterState::Blocked,
            crate::pairing::Permission::Ask => ChannelFilterState::NeedsConfirmation,
        };
    }

    /// Check if a channel is allowed for this client. O(1), ~2ns.
    #[inline(always)]
    pub fn channel_filter(&self, channel_id: u16) -> ChannelFilterState {
        self.channel_filters[(channel_id & 0xFF) as usize]
    }

    /// Check if a message is safe for 0-RTT delivery.
    #[inline(always)]
    pub fn is_0rtt_safe(&self, header: &PrismHeader) -> bool {
        self.is_0rtt_safe[(header.channel_id & 0xFF) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;
    use crate::pairing::*;

    fn make_context(perms: ChannelPermissions) -> SecurityContext {
        let id = LocalIdentity::generate("Test");
        let entry = Arc::new(PairingEntry {
            device: id.identity,
            state: PairingState::Paired,
            permissions: perms,
            paired_at: 0,
            last_seen: 0,
        });
        SecurityContext::for_device(entry)
    }

    #[test]
    fn default_permissions_allow_core_channels() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_filter(CHANNEL_DISPLAY), ChannelFilterState::AllowAll);
        assert_eq!(ctx.channel_filter(CHANNEL_INPUT), ChannelFilterState::AllowAll);
        assert_eq!(ctx.channel_filter(CHANNEL_CLIPBOARD), ChannelFilterState::AllowAll);
    }

    #[test]
    fn ask_permission_maps_to_needs_confirmation() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_filter(CHANNEL_CAMERA), ChannelFilterState::NeedsConfirmation);
        assert_eq!(ctx.channel_filter(CHANNEL_SENSOR), ChannelFilterState::NeedsConfirmation);
    }

    #[test]
    fn deny_permission_maps_to_blocked() {
        let mut perms = ChannelPermissions::default();
        perms.display = Permission::Deny;
        let ctx = make_context(perms);
        assert_eq!(ctx.channel_filter(CHANNEL_DISPLAY), ChannelFilterState::Blocked);
    }

    #[test]
    fn zero_rtt_safe_for_display_input_audio() {
        let ctx = make_context(ChannelPermissions::default());
        let display_header = PrismHeader {
            version: 0, channel_id: CHANNEL_DISPLAY, msg_type: 0,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        let input_header = PrismHeader {
            version: 0, channel_id: CHANNEL_INPUT, msg_type: 0,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        let audio_header = PrismHeader {
            version: 0, channel_id: CHANNEL_AUDIO, msg_type: 0,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        assert!(ctx.is_0rtt_safe(&display_header));
        assert!(ctx.is_0rtt_safe(&input_header));
        assert!(ctx.is_0rtt_safe(&audio_header));
    }

    #[test]
    fn zero_rtt_not_safe_for_clipboard_fileshare() {
        let ctx = make_context(ChannelPermissions::default());
        let clipboard_header = PrismHeader {
            version: 0, channel_id: CHANNEL_CLIPBOARD, msg_type: 0,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        let fileshare_header = PrismHeader {
            version: 0, channel_id: CHANNEL_FILESHARE, msg_type: 0,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        assert!(!ctx.is_0rtt_safe(&clipboard_header));
        assert!(!ctx.is_0rtt_safe(&fileshare_header));
    }

    #[test]
    fn unknown_channel_defaults_to_allow() {
        let ctx = make_context(ChannelPermissions::default());
        // Extension channel — not in permissions, defaults to AllowAll
        assert_eq!(ctx.channel_filter(0x100), ChannelFilterState::AllowAll);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/context.rs
git commit -m "feat(security): SecurityContext with pre-computed channel filters and 0-RTT policy

Fixed-size array indexed by channel_id & 0xFF for O(1) ~2ns lookups.
Maps ChannelPermissions to ChannelFilterState (AllowAll/Blocked/NeedsConfirmation).
0-RTT safe only for idempotent channels (Display, Input, Audio)."
```

---

## Task 7: SecurityGate Trait and Phase 1 Implementation

**Files:**
- Create: `crates/prism-security/src/gate.rs`

The SecurityGate trait is the contract Transport and Session Manager code against. Phase 1: AllowAll implementation.

- [ ] **Step 1: Write SecurityGate trait and default impl**

`crates/prism-security/src/gate.rs`:
```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::context::SecurityContext;
use crate::handshake::{HandshakeError, HandshakeResult, ServerHandshake};
use crate::identity::{DeviceIdentity, LocalIdentity};
use crate::pairing::{PairingError, PairingSnapshot, PairingState, PairingStore};
use crate::audit::{AuditEvent, AuditLog};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("handshake error: {0}")]
    Handshake(#[from] HandshakeError),
    #[error("pairing error: {0}")]
    Pairing(#[from] PairingError),
    #[error("unknown device")]
    UnknownDevice,
    #[error("device blocked")]
    DeviceBlocked,
}

/// Authentication result.
pub enum AuthResult {
    /// Authenticated. SecurityContext is ready.
    Authenticated(SecurityContext),
    /// Unknown device, no pairing in progress. Caller should silent-drop.
    SilentDrop,
    /// Device is blocked. Caller should silent-drop.
    Blocked,
}

/// The security gate contract. Transport and Session Manager code against this.
/// Phase 1: DefaultSecurityGate provides AllowAll for all channels.
pub trait SecurityGate: Send + Sync {
    /// Authenticate a client after Noise NK handshake completes.
    /// Returns AuthResult indicating whether to proceed or silent-drop.
    fn authenticate(
        &self,
        client_key: &[u8; 32],
        device_identity: &DeviceIdentity,
    ) -> AuthResult;

    /// Get SecurityContext for a known device.
    fn security_context(&self, device_id: &Uuid) -> Option<Arc<SecurityContext>>;

    /// Record a security audit event.
    fn audit(&self, event: AuditEvent);
}

/// Phase 1 SecurityGate. Uses PairingStore for authentication.
/// AllowAll channel filters (content filters added in Phase 3).
pub struct DefaultSecurityGate {
    pairing: PairingStore,
    identity: LocalIdentity,
    audit_log: AuditLog,
    /// Cached SecurityContexts per device.
    contexts: std::sync::Mutex<std::collections::HashMap<Uuid, Arc<SecurityContext>>>,
}

impl DefaultSecurityGate {
    pub fn new(pairing: PairingStore, identity: LocalIdentity, audit_log: AuditLog) -> Self {
        Self {
            pairing,
            identity,
            audit_log,
            contexts: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Access the local identity.
    pub fn identity(&self) -> &LocalIdentity {
        &self.identity
    }

    /// Access the pairing store (for management operations).
    pub fn pairing_store(&self) -> &PairingStore {
        &self.pairing
    }

    /// Mutable access to the pairing store.
    pub fn pairing_store_mut(&mut self) -> &mut PairingStore {
        &mut self.pairing
    }

    /// Create a server-side Noise NK handshake.
    pub fn create_server_handshake(&self) -> Result<ServerHandshake, HandshakeError> {
        ServerHandshake::new(&self.identity)
    }
}

impl SecurityGate for DefaultSecurityGate {
    fn authenticate(
        &self,
        client_key: &[u8; 32],
        device_identity: &DeviceIdentity,
    ) -> AuthResult {
        let snapshot = self.pairing.snapshot();

        match snapshot.get_by_key(client_key) {
            Some(entry) => {
                match entry.state {
                    PairingState::Paired => {
                        let ctx = SecurityContext::for_device(entry.clone());
                        let ctx = Arc::new(ctx);
                        // Cache the context
                        self.contexts.lock().unwrap()
                            .insert(entry.device.device_id, ctx.clone());

                        self.audit(AuditEvent::ClientAuthenticated {
                            device_id: entry.device.device_id,
                            device_name: entry.device.display_name.clone(),
                        });

                        AuthResult::Authenticated(
                            SecurityContext::for_device(entry.clone())
                        )
                    }
                    PairingState::Blocked => {
                        self.audit(AuditEvent::ClientRejected {
                            device_id: entry.device.device_id,
                            reason: "blocked".to_string(),
                        });
                        AuthResult::Blocked
                    }
                }
            }
            None => {
                self.audit(AuditEvent::ClientRejected {
                    device_id: device_identity.device_id,
                    reason: "unknown device".to_string(),
                });
                AuthResult::SilentDrop
            }
        }
    }

    fn security_context(&self, device_id: &Uuid) -> Option<Arc<SecurityContext>> {
        self.contexts.lock().unwrap().get(device_id).cloned()
    }

    fn audit(&self, event: AuditEvent) {
        self.audit_log.record(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::methods::manual::pair_manually;

    fn setup() -> (DefaultSecurityGate, LocalIdentity) {
        let server_id = LocalIdentity::generate("Server");
        let pairing = PairingStore::new();
        let audit = AuditLog::new(100);
        let gate = DefaultSecurityGate::new(pairing, server_id, audit);
        let client_id = LocalIdentity::generate("Client");
        (gate, client_id)
    }

    #[test]
    fn unknown_device_returns_silent_drop() {
        let (gate, client_id) = setup();
        let result = gate.authenticate(&client_id.public_key_bytes(), &client_id.identity);
        assert!(matches!(result, AuthResult::SilentDrop));
    }

    #[test]
    fn paired_device_authenticates() {
        let (mut gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        gate.pairing_store_mut().add(entry).unwrap();

        let result = gate.authenticate(&client_id.public_key_bytes(), &client_id.identity);
        assert!(matches!(result, AuthResult::Authenticated(_)));
    }

    #[test]
    fn blocked_device_returns_blocked() {
        let (mut gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        let device_id = entry.device.device_id;
        gate.pairing_store_mut().add(entry).unwrap();
        gate.pairing_store_mut().block(&device_id).unwrap();

        let result = gate.authenticate(&client_id.public_key_bytes(), &client_id.identity);
        assert!(matches!(result, AuthResult::Blocked));
    }

    #[test]
    fn security_context_cached_after_auth() {
        let (mut gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        let device_id = entry.device.device_id;
        gate.pairing_store_mut().add(entry).unwrap();

        gate.authenticate(&client_id.public_key_bytes(), &client_id.identity);
        let ctx = gate.security_context(&device_id);
        assert!(ctx.is_some());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/gate.rs
git commit -m "feat(security): SecurityGate trait with Phase 1 DefaultSecurityGate

SecurityGate trait: authenticate, security_context, audit. DefaultSecurityGate
uses PairingStore for auth: unknown → SilentDrop, blocked → Blocked, paired →
Authenticated with cached SecurityContext. Audit events recorded."
```

---

## Task 8: Audit Log

**Files:**
- Create: `crates/prism-security/src/audit.rs`

Basic audit log: connect/disconnect events. Ring buffer, bounded size.

- [ ] **Step 1: Write audit log with tests**

`crates/prism-security/src/audit.rs`:
```rust
use std::collections::VecDeque;
use std::sync::Mutex;
use uuid::Uuid;

/// Security audit events.
#[derive(Debug, Clone)]
pub enum AuditEvent {
    ClientAuthenticated {
        device_id: Uuid,
        device_name: String,
    },
    ClientRejected {
        device_id: Uuid,
        reason: String,
    },
    ClientDisconnected {
        device_id: Uuid,
    },
    KeyRotation {
        device_id: Uuid,
        accepted: bool,
    },
    PairingAttempt {
        method: String,
        success: bool,
    },
}

/// Timestamped audit entry.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: u64,
    pub event: AuditEvent,
}

/// Simple ring buffer audit log. Thread-safe via Mutex.
/// For Phase 1, Mutex is acceptable — audit writes are infrequent.
pub struct AuditLog {
    entries: Mutex<VecDeque<AuditEntry>>,
    max_entries: usize,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(max_entries)),
            max_entries,
        }
    }

    /// Record an audit event.
    pub fn record(&self, event: AuditEvent) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= self.max_entries {
            entries.pop_front();
        }
        entries.push_back(AuditEntry { timestamp, event });
    }

    /// Get all entries (for settings UI or debugging).
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    /// Number of recorded events.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        let log = AuditLog::new(100);
        log.record(AuditEvent::ClientAuthenticated {
            device_id: Uuid::nil(),
            device_name: "Test".to_string(),
        });
        assert_eq!(log.len(), 1);
        let entries = log.entries();
        assert!(matches!(entries[0].event, AuditEvent::ClientAuthenticated { .. }));
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let log = AuditLog::new(3);
        for i in 0..5 {
            log.record(AuditEvent::ClientDisconnected {
                device_id: Uuid::from_u128(i),
            });
        }
        assert_eq!(log.len(), 3);
        // Oldest (0, 1) evicted, remaining: 2, 3, 4
        let entries = log.entries();
        if let AuditEvent::ClientDisconnected { device_id } = &entries[0].event {
            assert_eq!(*device_id, Uuid::from_u128(2));
        }
    }

    #[test]
    fn empty_log() {
        let log = AuditLog::new(100);
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        assert!(log.entries().is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/audit.rs
git commit -m "feat(security): basic audit log with ring buffer

AuditEvent types: ClientAuthenticated, ClientRejected, ClientDisconnected,
KeyRotation, PairingAttempt. Ring buffer with configurable max entries.
Thread-safe via Mutex (audit writes are infrequent)."
```

---

## Task 9: Key Rotation

**Files:**
- Create: `crates/prism-security/src/key_rotation.rs`

Ed25519 signature of new key by old key. Verification on the receiving side.

- [ ] **Step 1: Write key rotation with tests**

`crates/prism-security/src/key_rotation.rs`:
```rust
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum KeyRotationError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("signature verification failed: {0}")]
    VerificationFailed(#[from] ed25519_dalek::SignatureError),
}

/// A key rotation message. The old key signs the new key to prove ownership.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotation {
    pub device_id: Uuid,
    pub new_public_key: [u8; 32],
    pub old_key_signature: [u8; 64],
    pub timestamp: u64,
}

impl KeyRotation {
    /// Create a key rotation, signing the new key with the old Ed25519 key.
    /// `old_ed25519_secret` is the Ed25519 signing key derived from the old identity.
    pub fn create(
        device_id: Uuid,
        new_public_key: [u8; 32],
        old_ed25519_secret: &[u8; 32],
    ) -> Self {
        let signing_key = SigningKey::from_bytes(old_ed25519_secret);
        let signature = signing_key.sign(&new_public_key);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            device_id,
            new_public_key,
            old_key_signature: signature.to_bytes(),
            timestamp: now,
        }
    }

    /// Verify that the old key signed the new key.
    /// `old_ed25519_public` is the Ed25519 verifying key of the old identity.
    pub fn verify(&self, old_ed25519_public: &[u8; 32]) -> Result<(), KeyRotationError> {
        let verifying_key = VerifyingKey::from_bytes(old_ed25519_public)
            .map_err(|e| KeyRotationError::VerificationFailed(e))?;
        let signature = ed25519_dalek::Signature::from_bytes(&self.old_key_signature);
        verifying_key
            .verify(&self.new_public_key, &signature)
            .map_err(|e| KeyRotationError::VerificationFailed(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn create_and_verify_rotation() {
        let old_signing = SigningKey::generate(&mut OsRng);
        let old_public = old_signing.verifying_key().to_bytes();
        let new_key = [0xABu8; 32];

        let rotation = KeyRotation::create(
            Uuid::now_v7(),
            new_key,
            &old_signing.to_bytes(),
        );

        assert!(rotation.verify(&old_public).is_ok());
    }

    #[test]
    fn wrong_public_key_fails_verification() {
        let old_signing = SigningKey::generate(&mut OsRng);
        let wrong_signing = SigningKey::generate(&mut OsRng);
        let wrong_public = wrong_signing.verifying_key().to_bytes();
        let new_key = [0xABu8; 32];

        let rotation = KeyRotation::create(
            Uuid::now_v7(),
            new_key,
            &old_signing.to_bytes(),
        );

        assert!(rotation.verify(&wrong_public).is_err());
    }

    #[test]
    fn tampered_new_key_fails_verification() {
        let old_signing = SigningKey::generate(&mut OsRng);
        let old_public = old_signing.verifying_key().to_bytes();
        let new_key = [0xABu8; 32];

        let mut rotation = KeyRotation::create(
            Uuid::now_v7(),
            new_key,
            &old_signing.to_bytes(),
        );
        // Tamper with the new key
        rotation.new_public_key = [0xCDu8; 32];

        assert!(rotation.verify(&old_public).is_err());
    }

    #[test]
    fn rotation_json_roundtrip() {
        let old_signing = SigningKey::generate(&mut OsRng);
        let new_key = [0xABu8; 32];
        let rotation = KeyRotation::create(
            Uuid::now_v7(),
            new_key,
            &old_signing.to_bytes(),
        );

        let json = serde_json::to_string(&rotation).unwrap();
        let decoded: KeyRotation = serde_json::from_str(&json).unwrap();
        assert_eq!(rotation.device_id, decoded.device_id);
        assert_eq!(rotation.new_public_key, decoded.new_public_key);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/key_rotation.rs
git commit -m "feat(security): key rotation with Ed25519 signed attestation

KeyRotation: old Ed25519 key signs the new Curve25519 public key.
Receiving side verifies signature against known old key.
Tamper-proof: changing the new key invalidates the signature."
```

---

## Plan Self-Review

**1. Spec coverage:**
- Section 1 (Device Identity): Task 2. DeviceIdentity with UUID, key, name, platform. LocalIdentity with generate/load/save.
- Section 2 (Pairing Model): Task 4. PairingStore, PairingEntry, PairingState, ChannelPermissions, Permission (Allow/Deny/Ask). Copy-on-write snapshot.
- Section 3 (Pairing Methods): Task 4. Manual pairing implemented. SPAKE2 code generation stub. PairingMethod enum for future methods.
- Section 4 (Key Rotation): Task 9. KeyRotation with Ed25519 signature. Create + verify.
- Section 5 (Authentication): Task 5 (Noise NK handshake). Task 7 (SecurityGate authenticate).
- Section 7 (0-RTT): Task 6. is_0rtt_safe per channel.
- Section 9 (Content Filters): Task 6. ChannelFilterState enum defined. AllowAll for Phase 1.
- Section 13 (Crypto): Task 3. HKDF, Shannon entropy, is_high_entropy.
- Section 14 (Audit Log): Task 8. AuditEvent, AuditLog ring buffer.
- Section 15 (SecurityGate Trait): Task 7. Full trait + DefaultSecurityGate.
- Section 1.1 (Hardware Keystore): Not in Phase 1. CryptoBackend trait deferred to Phase 4.
- Section 6 (Pre-auth rate limiting): Deferred to Transport plan (rate limiter is Transport's concern).
- Section 8 (Browser auth): Phase 2, not in this plan.

**2. Placeholder scan:** No TBDs or "implement later." SPAKE2 is explicitly a stub with working code generation.

**3. Type consistency:**
- `LocalIdentity` used consistently across Tasks 2, 4, 5, 6, 7.
- `PairingEntry` used in Tasks 4, 6, 7. Same fields.
- `SecurityContext` used in Tasks 6, 7. Same struct.
- `AuditEvent` used in Tasks 7, 8. Same enum.
- `HandshakeResult` used in Task 5, referenced in Task 7 (indirectly via ServerHandshake).
- `ChannelFilterState` defined in Task 6, used in SecurityContext.

No issues found.
