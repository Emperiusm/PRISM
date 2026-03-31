# Plan 2: Security Implementation (Revised)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-security` crate that provides device identity (Curve25519 + Ed25519 + UUIDv7), encrypted pairing store with thread-safe copy-on-write snapshots, Noise NK handshake, SecurityGate trait with AllowAll Phase 1 implementation, SecurityContext with split fast-path/slow-path filter architecture, 0-RTT safety policy, audit log, and Ed25519-signed key rotation.

**Architecture:** `prism-security` depends on `prism-protocol` (channel IDs, header types) and `prism-metrics` (metrics recording). The SecurityGate trait is the contract that Transport and Session Manager code against. Phase 1 implements the full trait with AllowAll channel filters — the `ContentFilter` trait is defined now so Phase 3 can add implementations without changing the interface. The pairing store uses `arc-swap` for lock-free reads and `Mutex<PairingWriter>` for serialized writes. Pairing data is encrypted at rest with AES-256-GCM, key derived via HKDF from the device's Curve25519 secret. `LocalIdentity` stores both Curve25519 (for Noise NK DH) and Ed25519 (for signing — key rotation, TLS cert binding) keypairs.

**Tech Stack:** `snow` (Noise protocol), `x25519-dalek` (Curve25519), `ed25519-dalek` (Ed25519 signing), `uuid` (UUIDv7), `rand` (key generation), `serde`/`serde_json` (serialization), `hex` (key display), `aes-gcm` (pairing store encryption), `hkdf`+`sha2` (key derivation), `arc-swap` (lock-free snapshot swapping)

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
        identity.rs               # DeviceIdentity (Curve25519 + Ed25519), LocalIdentity
        crypto.rs                 # HKDF, AES-GCM encrypt/decrypt, Shannon entropy
        filter.rs                 # ContentFilter trait, FilterResult (Phase 3 boundary)
        pairing/
          mod.rs                  # PairingStore (arc-swap + encrypted file), PairingSnapshot
          methods/
            mod.rs                # PairingMethod enum
            manual.rs             # Manual hex key exchange
            spake2.rs             # Short code generation (stub)
        handshake.rs              # Noise NK handshake (inside QUIC)
        gate.rs                   # SecurityGate trait + DefaultSecurityGate
        context.rs                # SecurityContext, ChannelDecision, 0-RTT policy
        audit.rs                  # AuditEvent, AuditLog ring buffer
        key_rotation.rs           # KeyRotation with Ed25519 signature
```

---

## Task 1: Add prism-security Crate to Workspace

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-security/Cargo.toml`
- Create: `crates/prism-security/src/lib.rs`
- Create: placeholder files for all modules

- [ ] **Step 1: Update workspace root Cargo.toml**

Add `"crates/prism-security"` to members and add new dependencies:

```toml
[workspace]
resolver = "2"
members = [
    "crates/prism-protocol",
    "crates/prism-metrics",
    "crates/prism-security",
]

[workspace.dependencies]
# existing deps stay unchanged, add:
snow = "0.9"
x25519-dalek = { version = "2", features = ["static_secrets"] }
ed25519-dalek = { version = "2", features = ["rand_core"] }
uuid = { version = "1", features = ["v7"] }
rand = "0.8"
hex = "0.4"
aes-gcm = "0.10"
hkdf = "0.12"
sha2 = "0.10"
arc-swap = "1"
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
arc-swap.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
bytes.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 3: Create lib.rs and placeholder files**

`crates/prism-security/src/lib.rs`:
```rust
pub mod identity;
pub mod crypto;
pub mod filter;
pub mod pairing;
pub mod handshake;
pub mod gate;
pub mod context;
pub mod audit;
pub mod key_rotation;
```

Create empty placeholder files for every module listed in the file structure. The `pairing/` directory needs `mod.rs` and `methods/mod.rs`, `methods/manual.rs`, `methods/spake2.rs`.

- [ ] **Step 4: Verify workspace builds**

Run: `cargo build`
Expected: Clean build.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/prism-security/
git commit -m "feat: add prism-security crate to workspace"
```

---

## Task 2: DeviceIdentity with Dual Keypairs (Curve25519 + Ed25519)

**Files:**
- Create: `crates/prism-security/src/identity.rs`

`LocalIdentity` stores both Curve25519 (Noise NK / DH) and Ed25519 (signing — key rotation, TLS cert binding). `DeviceIdentity` is the public shareable part with both public keys.

- [ ] **Step 1: Write identity types with tests**

`crates/prism-security/src/identity.rs`:
```rust
use ed25519_dalek::SigningKey as Ed25519SigningKey;
use ed25519_dalek::VerifyingKey as Ed25519VerifyingKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid key length: expected 32 bytes, got {0}")]
    InvalidKeyLength(usize),
}

/// Platform type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    MacOS,
    Linux,
    Android,
    #[serde(rename = "ios")]
    IOS,
    Browser,
}

impl Platform {
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        { Platform::Windows }
        #[cfg(target_os = "macos")]
        { Platform::MacOS }
        #[cfg(target_os = "linux")]
        { Platform::Linux }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        { Platform::Linux }
    }
}

/// A device's public identity. Serializable, shareable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub device_id: Uuid,
    pub display_name: String,
    pub platform: Platform,
    /// Curve25519 public key (for Noise NK / DH).
    #[serde(with = "hex_key")]
    pub current_key: [u8; 32],
    /// Ed25519 verifying key (for signature verification — key rotation, TLS cert).
    #[serde(with = "hex_key")]
    pub signing_key: [u8; 32],
    pub created_at: u64,
}

mod hex_key {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(key: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(key))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(key)
    }
}

/// Full local identity with secret keys. Never serialized directly.
pub struct LocalIdentity {
    // Curve25519 (for Noise NK / DH)
    x25519_secret: X25519Secret,
    x25519_public: X25519PublicKey,
    // Ed25519 (for signing — key rotation, TLS cert binding)
    ed25519_signing: Ed25519SigningKey,
    ed25519_verifying: Ed25519VerifyingKey,
    /// Public identity (shareable).
    pub identity: DeviceIdentity,
}

#[derive(Serialize, Deserialize)]
struct StoredLocalIdentity {
    #[serde(with = "hex_key")]
    x25519_secret: [u8; 32],
    #[serde(with = "hex_key")]
    ed25519_secret: [u8; 32],
    identity: DeviceIdentity,
}

impl LocalIdentity {
    /// Generate a new random identity with both keypairs.
    pub fn generate(display_name: &str) -> Self {
        let x25519_secret = X25519Secret::random_from_rng(OsRng);
        let x25519_public = X25519PublicKey::from(&x25519_secret);

        let ed25519_signing = Ed25519SigningKey::generate(&mut OsRng);
        let ed25519_verifying = ed25519_signing.verifying_key();

        let device_id = Uuid::now_v7();
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            x25519_secret,
            x25519_public,
            ed25519_signing,
            ed25519_verifying,
            identity: DeviceIdentity {
                device_id,
                display_name: display_name.to_string(),
                platform: Platform::current(),
                current_key: *x25519_public.as_bytes(),
                signing_key: ed25519_verifying.to_bytes(),
                created_at,
            },
        }
    }

    /// Load from file, or generate and save if it doesn't exist.
    pub fn load_or_generate(
        path: &std::path::Path,
        display_name: &str,
    ) -> Result<Self, IdentityError> {
        if path.exists() {
            let data = std::fs::read_to_string(path)?;
            let stored: StoredLocalIdentity = serde_json::from_str(&data)?;

            let x25519_secret = X25519Secret::from(stored.x25519_secret);
            let x25519_public = X25519PublicKey::from(&x25519_secret);

            let ed25519_signing = Ed25519SigningKey::from_bytes(&stored.ed25519_secret);
            let ed25519_verifying = ed25519_signing.verifying_key();

            Ok(Self {
                x25519_secret,
                x25519_public,
                ed25519_signing,
                ed25519_verifying,
                identity: stored.identity,
            })
        } else {
            let local = Self::generate(display_name);
            local.save(path)?;
            Ok(local)
        }
    }

    /// Save to file.
    pub fn save(&self, path: &std::path::Path) -> Result<(), IdentityError> {
        let stored = StoredLocalIdentity {
            x25519_secret: self.x25519_secret_bytes(),
            ed25519_secret: self.ed25519_signing.to_bytes(),
            identity: self.identity.clone(),
        };
        let json = serde_json::to_string_pretty(&stored)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Curve25519 public key bytes (for Noise NK).
    pub fn x25519_public_bytes(&self) -> [u8; 32] {
        *self.x25519_public.as_bytes()
    }

    /// Curve25519 secret key bytes (for Noise NK handshake).
    pub fn x25519_secret_bytes(&self) -> [u8; 32] {
        self.x25519_secret.to_bytes()
    }

    /// Ed25519 signing key (for key rotation, TLS cert binding).
    pub fn ed25519_signing_key(&self) -> &Ed25519SigningKey {
        &self.ed25519_signing
    }

    /// Ed25519 verifying key bytes.
    pub fn ed25519_verifying_bytes(&self) -> [u8; 32] {
        self.ed25519_verifying.to_bytes()
    }

    /// Device UUID.
    pub fn device_id(&self) -> Uuid {
        self.identity.device_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_valid_identity() {
        let id = LocalIdentity::generate("Test Device");
        assert_ne!(id.x25519_public_bytes(), [0u8; 32]);
        assert_ne!(id.ed25519_verifying_bytes(), [0u8; 32]);
        assert_eq!(id.identity.display_name, "Test Device");
        assert_eq!(id.identity.platform, Platform::current());
        assert_eq!(id.identity.current_key, id.x25519_public_bytes());
        assert_eq!(id.identity.signing_key, id.ed25519_verifying_bytes());
    }

    #[test]
    fn two_identities_are_different() {
        let id1 = LocalIdentity::generate("Device 1");
        let id2 = LocalIdentity::generate("Device 2");
        assert_ne!(id1.x25519_public_bytes(), id2.x25519_public_bytes());
        assert_ne!(id1.ed25519_verifying_bytes(), id2.ed25519_verifying_bytes());
        assert_ne!(id1.device_id(), id2.device_id());
    }

    #[test]
    fn load_or_generate_creates_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");

        let id1 = LocalIdentity::load_or_generate(&path, "Test").unwrap();
        assert!(path.exists());

        let id2 = LocalIdentity::load_or_generate(&path, "Test").unwrap();
        assert_eq!(id1.x25519_public_bytes(), id2.x25519_public_bytes());
        assert_eq!(id1.ed25519_verifying_bytes(), id2.ed25519_verifying_bytes());
        assert_eq!(id1.device_id(), id2.device_id());
    }

    #[test]
    fn x25519_secret_derives_correct_public() {
        let id = LocalIdentity::generate("Test");
        let derived = X25519PublicKey::from(&X25519Secret::from(id.x25519_secret_bytes()));
        assert_eq!(*derived.as_bytes(), id.x25519_public_bytes());
    }

    #[test]
    fn ed25519_signing_key_matches_verifying() {
        let id = LocalIdentity::generate("Test");
        let verifying = id.ed25519_signing_key().verifying_key();
        assert_eq!(verifying.to_bytes(), id.ed25519_verifying_bytes());
    }

    #[test]
    fn device_identity_json_roundtrip() {
        let id = LocalIdentity::generate("Test");
        let json = serde_json::to_string(&id.identity).unwrap();
        let decoded: DeviceIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(id.identity, decoded);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All 6 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/identity.rs
git commit -m "feat(security): DeviceIdentity with dual keypairs (Curve25519 + Ed25519)

LocalIdentity stores both X25519 (Noise NK / DH) and Ed25519 (signing for
key rotation and TLS cert binding). DeviceIdentity includes both public keys.
UUIDv7 device IDs. Platform detection. File persistence."
```

---

## Task 3: Crypto Utilities — HKDF, AES-GCM, Entropy

**Files:**
- Create: `crates/prism-security/src/crypto.rs`

HKDF key derivation, AES-256-GCM encrypt/decrypt (for pairing store), Shannon entropy.

- [ ] **Step 1: Write crypto utilities with tests**

`crates/prism-security/src/crypto.rs`:
```rust
use aes_gcm::aead::{Aead, KeyInit, OsRng as AeadOsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
}

/// Derive a 32-byte key from a secret and context using HKDF-SHA256.
pub fn hkdf_derive(secret: &[u8; 32], context: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, secret);
    let mut output = [0u8; 32];
    hk.expand(context.as_bytes(), &mut output)
        .expect("HKDF expand failed");
    output
}

/// Encrypt data with AES-256-GCM. Returns nonce (12 bytes) || ciphertext.
pub fn encrypt_aes_gcm(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::EncryptionFailed)?;

    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt data encrypted with encrypt_aes_gcm. Input: nonce (12 bytes) || ciphertext.
pub fn decrypt_aes_gcm(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < 12 {
        return Err(CryptoError::DecryptionFailed);
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)
}

/// Shannon entropy in bits per byte (0.0 = uniform, 8.0 = maximum random).
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
    fn hkdf_deterministic() {
        let secret = [42u8; 32];
        assert_eq!(
            hkdf_derive(&secret, "pairing-store"),
            hkdf_derive(&secret, "pairing-store")
        );
    }

    #[test]
    fn hkdf_different_contexts() {
        let secret = [42u8; 32];
        assert_ne!(
            hkdf_derive(&secret, "pairing-store"),
            hkdf_derive(&secret, "audit-log")
        );
    }

    #[test]
    fn aes_gcm_roundtrip() {
        let key = [1u8; 32];
        let plaintext = b"hello PRISM";
        let encrypted = encrypt_aes_gcm(&key, plaintext).unwrap();
        let decrypted = decrypt_aes_gcm(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn aes_gcm_wrong_key_fails() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let encrypted = encrypt_aes_gcm(&key1, b"secret").unwrap();
        assert!(decrypt_aes_gcm(&key2, &encrypted).is_err());
    }

    #[test]
    fn aes_gcm_tampered_ciphertext_fails() {
        let key = [1u8; 32];
        let mut encrypted = encrypt_aes_gcm(&key, b"secret").unwrap();
        // Flip a bit in the ciphertext
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0x01;
        assert!(decrypt_aes_gcm(&key, &encrypted).is_err());
    }

    #[test]
    fn aes_gcm_too_short_fails() {
        let key = [1u8; 32];
        assert!(decrypt_aes_gcm(&key, &[0u8; 5]).is_err());
    }

    #[test]
    fn entropy_empty() {
        assert_eq!(shannon_entropy(&[]), 0.0);
    }

    #[test]
    fn entropy_uniform_low() {
        assert!(shannon_entropy(&vec![b'a'; 100]) < 0.01);
    }

    #[test]
    fn entropy_random_high() {
        let data: Vec<u8> = (0..=255).cycle().take(1024).collect();
        assert!(shannon_entropy(&data) > 7.9);
    }

    #[test]
    fn is_high_entropy_detects_secrets() {
        let key: Vec<u8> = (0..32).map(|i| (i * 7 + 13) as u8).collect();
        assert!(is_high_entropy(&key));
    }

    #[test]
    fn is_high_entropy_rejects_normal_text() {
        assert!(!is_high_entropy(b"hello world"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/crypto.rs
git commit -m "feat(security): HKDF, AES-256-GCM encrypt/decrypt, Shannon entropy

HKDF-SHA256 for key derivation. AES-256-GCM for pairing store encryption
(nonce || ciphertext format). Shannon entropy for clipboard content filter.
is_high_entropy threshold check for passwords/tokens."
```

---

## Task 4: ContentFilter Trait and FilterResult

**Files:**
- Create: `crates/prism-security/src/filter.rs`

Defines the content filter boundary for Phase 3. Phase 1 only defines the trait — no implementations.

- [ ] **Step 1: Write filter trait**

`crates/prism-security/src/filter.rs`:
```rust
use bytes::Bytes;

/// Content filter trait. Implemented by clipboard/notification filters in Phase 3.
/// Defined in Phase 1 so the SecurityContext can hold `Arc<dyn ContentFilter>`.
pub trait ContentFilter: Send + Sync {
    /// Inspect data and decide whether to allow, redact, block, or confirm.
    fn filter(&self, data: &[u8]) -> FilterResult;

    /// Human-readable description for settings UI.
    fn description(&self) -> &str;
}

/// Result of a content filter check.
#[derive(Debug, Clone)]
pub enum FilterResult {
    /// Allow the data through unchanged.
    Allow,
    /// Replace the data with a redacted version.
    Redact(Bytes),
    /// Block the data silently.
    Block,
    /// Requires user confirmation before sending. String explains why.
    Confirm(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Test filter that blocks everything with "secret" in it.
    struct TestFilter;

    impl ContentFilter for TestFilter {
        fn filter(&self, data: &[u8]) -> FilterResult {
            if data.windows(6).any(|w| w == b"secret") {
                FilterResult::Block
            } else {
                FilterResult::Allow
            }
        }

        fn description(&self) -> &str {
            "test filter"
        }
    }

    #[test]
    fn content_filter_trait_is_object_safe() {
        let filter: Arc<dyn ContentFilter> = Arc::new(TestFilter);
        assert!(matches!(filter.filter(b"hello"), FilterResult::Allow));
        assert!(matches!(filter.filter(b"my secret"), FilterResult::Block));
        assert_eq!(filter.description(), "test filter");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/filter.rs
git commit -m "feat(security): ContentFilter trait and FilterResult for Phase 3 boundary

Trait defined now so SecurityContext can hold Arc<dyn ContentFilter>.
FilterResult: Allow, Redact, Block, Confirm. Phase 3 adds clipboard
and notification filter implementations."
```

---

## Task 5: Pairing Store with arc-swap, Encryption, and Thread Safety

**Files:**
- Create: `crates/prism-security/src/pairing/mod.rs`
- Create: `crates/prism-security/src/pairing/methods/mod.rs`
- Create: `crates/prism-security/src/pairing/methods/manual.rs`
- Create: `crates/prism-security/src/pairing/methods/spake2.rs`

Thread-safe via `arc-swap` for lock-free reads and `Mutex` for serialized writes. Encrypted file persistence via AES-256-GCM with HKDF-derived key.

- [ ] **Step 1: Write pairing types, store, and methods**

`crates/prism-security/src/pairing/mod.rs`:
```rust
pub mod methods;

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::{decrypt_aes_gcm, encrypt_aes_gcm, hkdf_derive, CryptoError};
use crate::identity::DeviceIdentity;

#[derive(Debug, Error)]
pub enum PairingError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("device not found: {0}")]
    DeviceNotFound(Uuid),
    #[error("device already paired: {0}")]
    AlreadyPaired(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairingState {
    Paired,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingEntry {
    pub device: DeviceIdentity,
    pub state: PairingState,
    pub permissions: ChannelPermissions,
    pub paired_at: u64,
    pub last_seen: u64,
}

/// Read-optimized snapshot. Lock-free via Arc.
#[derive(Debug, Clone, Default)]
pub struct PairingSnapshot {
    pub by_key: HashMap<[u8; 32], Arc<PairingEntry>>,
    pub by_device_id: HashMap<Uuid, Arc<PairingEntry>>,
    pub generation: u64,
}

impl PairingSnapshot {
    pub fn get_by_key(&self, key: &[u8; 32]) -> Option<&Arc<PairingEntry>> {
        self.by_key.get(key)
    }

    pub fn get_by_device_id(&self, id: &Uuid) -> Option<&Arc<PairingEntry>> {
        self.by_device_id.get(id)
    }

    pub fn is_authorized(&self, key: &[u8; 32]) -> bool {
        self.by_key
            .get(key)
            .is_some_and(|e| e.state == PairingState::Paired)
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// Thread-safe pairing store. Lock-free reads via arc-swap.
/// Write operations take a Mutex, rebuild snapshot, and swap atomically.
/// All methods take &self (no &mut self needed).
pub struct PairingStore {
    current: ArcSwap<PairingSnapshot>,
    writer: Mutex<()>,
    path: Option<PathBuf>,
    encryption_key: Option<[u8; 32]>,
}

impl PairingStore {
    /// In-memory only (no persistence).
    pub fn new() -> Self {
        Self {
            current: ArcSwap::from_pointee(PairingSnapshot::default()),
            writer: Mutex::new(()),
            path: None,
            encryption_key: None,
        }
    }

    /// File-backed with encryption. Key derived from device secret.
    pub fn with_encrypted_file(
        path: &std::path::Path,
        device_secret: &[u8; 32],
    ) -> Result<Self, PairingError> {
        let encryption_key = hkdf_derive(device_secret, "pairing-store");
        let store = Self {
            current: ArcSwap::from_pointee(PairingSnapshot::default()),
            writer: Mutex::new(()),
            path: Some(path.to_path_buf()),
            encryption_key: Some(encryption_key),
        };
        if path.exists() {
            store.load()?;
        }
        Ok(store)
    }

    /// Lock-free snapshot read. ~5ns (atomic load + Arc clone).
    pub fn snapshot(&self) -> Arc<PairingSnapshot> {
        self.current.load_full()
    }

    /// Add a paired device. Thread-safe.
    pub fn add(&self, entry: PairingEntry) -> Result<(), PairingError> {
        let _lock = self.writer.lock().unwrap();
        let current = self.current.load_full();
        if current.by_device_id.contains_key(&entry.device.device_id) {
            return Err(PairingError::AlreadyPaired(entry.device.device_id));
        }
        let entry = Arc::new(entry);
        let mut new_snap = (*current).clone();
        new_snap
            .by_key
            .insert(entry.device.current_key, entry.clone());
        new_snap.by_device_id.insert(entry.device.device_id, entry);
        new_snap.generation += 1;
        self.current.store(Arc::new(new_snap));
        self.persist()?;
        Ok(())
    }

    /// Remove a device by UUID. Thread-safe.
    pub fn remove(&self, device_id: &Uuid) -> Result<(), PairingError> {
        let _lock = self.writer.lock().unwrap();
        let current = self.current.load_full();
        let entry = current
            .by_device_id
            .get(device_id)
            .ok_or(PairingError::DeviceNotFound(*device_id))?;
        let key = entry.device.current_key;
        let mut new_snap = (*current).clone();
        new_snap.by_key.remove(&key);
        new_snap.by_device_id.remove(device_id);
        new_snap.generation += 1;
        self.current.store(Arc::new(new_snap));
        self.persist()?;
        Ok(())
    }

    /// Block a device. Thread-safe.
    pub fn block(&self, device_id: &Uuid) -> Result<(), PairingError> {
        let _lock = self.writer.lock().unwrap();
        let current = self.current.load_full();
        let entry = current
            .by_device_id
            .get(device_id)
            .ok_or(PairingError::DeviceNotFound(*device_id))?;
        let mut updated = (**entry).clone();
        updated.state = PairingState::Blocked;
        let updated = Arc::new(updated);
        let mut new_snap = (*current).clone();
        new_snap
            .by_key
            .insert(updated.device.current_key, updated.clone());
        new_snap.by_device_id.insert(*device_id, updated);
        new_snap.generation += 1;
        self.current.store(Arc::new(new_snap));
        self.persist()?;
        Ok(())
    }

    /// Update a device's key (key rotation). Thread-safe.
    pub fn update_key(
        &self,
        device_id: &Uuid,
        new_key: [u8; 32],
    ) -> Result<(), PairingError> {
        let _lock = self.writer.lock().unwrap();
        let current = self.current.load_full();
        let entry = current
            .by_device_id
            .get(device_id)
            .ok_or(PairingError::DeviceNotFound(*device_id))?;
        let old_key = entry.device.current_key;
        let mut updated = (**entry).clone();
        updated.device.current_key = new_key;
        let updated = Arc::new(updated);
        let mut new_snap = (*current).clone();
        new_snap.by_key.remove(&old_key);
        new_snap.by_key.insert(new_key, updated.clone());
        new_snap.by_device_id.insert(*device_id, updated);
        new_snap.generation += 1;
        self.current.store(Arc::new(new_snap));
        self.persist()?;
        Ok(())
    }

    fn persist(&self) -> Result<(), PairingError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let current = self.current.load_full();
        let entries: Vec<&PairingEntry> =
            current.by_device_id.values().map(|e| e.as_ref()).collect();
        let plaintext = serde_json::to_vec_pretty(&entries)?;

        let data = if let Some(key) = &self.encryption_key {
            encrypt_aes_gcm(key, &plaintext)?
        } else {
            plaintext
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, data)?;
        Ok(())
    }

    fn load(&self) -> Result<(), PairingError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let data = std::fs::read(path)?;

        let plaintext = if let Some(key) = &self.encryption_key {
            decrypt_aes_gcm(key, &data)?
        } else {
            data
        };

        let entries: Vec<PairingEntry> = serde_json::from_slice(&plaintext)?;
        let mut snap = PairingSnapshot::default();
        for entry in entries {
            let entry = Arc::new(entry);
            snap.by_key
                .insert(entry.device.current_key, entry.clone());
            snap.by_device_id.insert(entry.device.device_id, entry);
        }
        self.current.store(Arc::new(snap));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;

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
        let store = PairingStore::new();
        let entry = make_entry("Device A");
        let key = entry.device.current_key;
        store.add(entry).unwrap();
        assert!(store.snapshot().is_authorized(&key));
    }

    #[test]
    fn add_and_lookup_by_device_id() {
        let store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        store.add(entry).unwrap();
        let found = store.snapshot().get_by_device_id(&device_id).unwrap();
        assert_eq!(found.device.display_name, "Device A");
    }

    #[test]
    fn duplicate_add_rejected() {
        let store = PairingStore::new();
        let entry = make_entry("Device A");
        let entry2 = entry.clone();
        store.add(entry).unwrap();
        assert!(matches!(store.add(entry2), Err(PairingError::AlreadyPaired(_))));
    }

    #[test]
    fn remove_device() {
        let store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        let key = entry.device.current_key;
        store.add(entry).unwrap();
        store.remove(&device_id).unwrap();
        assert!(!store.snapshot().is_authorized(&key));
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn block_device() {
        let store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        let key = entry.device.current_key;
        store.add(entry).unwrap();
        store.block(&device_id).unwrap();
        assert!(!store.snapshot().is_authorized(&key));
        let found = store.snapshot().get_by_key(&key).unwrap();
        assert_eq!(found.state, PairingState::Blocked);
    }

    #[test]
    fn update_key() {
        let store = PairingStore::new();
        let entry = make_entry("Device A");
        let device_id = entry.device.device_id;
        let old_key = entry.device.current_key;
        store.add(entry).unwrap();
        let new_key = [0xABu8; 32];
        store.update_key(&device_id, new_key).unwrap();
        assert!(!store.snapshot().is_authorized(&old_key));
        assert!(store.snapshot().is_authorized(&new_key));
    }

    #[test]
    fn snapshot_independence() {
        let store = PairingStore::new();
        let entry = make_entry("Device A");
        store.add(entry).unwrap();
        let snap_before = store.snapshot();
        store.add(make_entry("Device B")).unwrap();
        assert_eq!(snap_before.len(), 1);
        assert_eq!(store.snapshot().len(), 2);
    }

    #[test]
    fn encrypted_file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pairings.enc");
        let secret = [42u8; 32];

        {
            let store = PairingStore::with_encrypted_file(&path, &secret).unwrap();
            store.add(make_entry("Device A")).unwrap();
            store.add(make_entry("Device B")).unwrap();
        }

        {
            let store = PairingStore::with_encrypted_file(&path, &secret).unwrap();
            assert_eq!(store.snapshot().len(), 2);
        }

        // Wrong key can't decrypt
        {
            let wrong_secret = [99u8; 32];
            let result = PairingStore::with_encrypted_file(&path, &wrong_secret);
            assert!(result.is_err());
        }
    }

    #[test]
    fn thread_safe_concurrent_adds() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(PairingStore::new());
        let mut handles = Vec::new();

        for i in 0..4 {
            let store = store.clone();
            handles.push(thread::spawn(move || {
                let entry = make_entry(&format!("Device {i}"));
                store.add(entry).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(store.snapshot().len(), 4);
    }

    #[test]
    fn default_permissions() {
        let perms = ChannelPermissions::default();
        assert_eq!(perms.display, Permission::Allow);
        assert_eq!(perms.camera, Permission::Ask);
        assert_eq!(perms.filesystem_browse, Permission::Ask);
    }
}
```

- [ ] **Step 2: Write pairing methods**

`crates/prism-security/src/pairing/methods/mod.rs`:
```rust
pub mod manual;
pub mod spake2;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PairingMethod {
    Manual,
    Tailscale,
    ShortCode,
    QrCode,
    Coordination,
}
```

`crates/prism-security/src/pairing/methods/manual.rs`:
```rust
use crate::identity::DeviceIdentity;
use crate::pairing::{ChannelPermissions, PairingEntry, PairingState};

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
        let remote = LocalIdentity::generate("Remote");
        let entry = pair_manually(remote.identity);
        assert_eq!(entry.state, PairingState::Paired);
        assert_eq!(entry.device.display_name, "Remote");
    }
}
```

`crates/prism-security/src/pairing/methods/spake2.rs`:
```rust
pub fn generate_code() -> String {
    use rand::Rng;
    let code: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{:06}", code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_6_digits() {
        let code = generate_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-security/src/pairing/
git commit -m "feat(security): thread-safe encrypted pairing store with arc-swap

PairingStore: lock-free reads via ArcSwap, Mutex-serialized writes.
All methods take &self (no &mut self). Encrypted file persistence via
AES-256-GCM with HKDF-derived key. Copy-on-write snapshots.
Manual pairing + SPAKE2 code gen stub."
```

---

## Task 6: Noise NK Handshake

**Files:**
- Create: `crates/prism-security/src/handshake.rs`

Same as original plan — no changes needed here.

- [ ] **Step 1: Write handshake with tests**

`crates/prism-security/src/handshake.rs`:
```rust
use snow::{Builder, HandshakeState, TransportState};
use thiserror::Error;

use crate::identity::LocalIdentity;

const NOISE_PATTERN: &str = "Noise_NK_25519_ChaChaPoly_SHA256";

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("noise protocol error: {0}")]
    Noise(#[from] snow::Error),
    #[error("handshake not complete")]
    NotComplete,
}

pub struct HandshakeResult {
    pub transport: TransportState,
    pub remote_static: Option<[u8; 32]>,
}

pub struct ServerHandshake {
    state: HandshakeState,
}

impl ServerHandshake {
    pub fn new(identity: &LocalIdentity) -> Result<Self, HandshakeError> {
        let state = Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&identity.x25519_secret_bytes())
            .build_responder()?;
        Ok(Self { state })
    }

    pub fn respond(&mut self, client_msg: &[u8]) -> Result<Vec<u8>, HandshakeError> {
        let mut read_buf = vec![0u8; 65535];
        self.state.read_message(client_msg, &mut read_buf)?;
        let mut response = vec![0u8; 65535];
        let len = self.state.write_message(&[], &mut response)?;
        response.truncate(len);
        Ok(response)
    }

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

pub struct ClientHandshake {
    state: HandshakeState,
}

impl ClientHandshake {
    pub fn new(
        identity: &LocalIdentity,
        server_public_key: &[u8; 32],
    ) -> Result<Self, HandshakeError> {
        let state = Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&identity.x25519_secret_bytes())
            .remote_public_key(server_public_key)
            .build_initiator()?;
        Ok(Self { state })
    }

    pub fn initiate(&mut self) -> Result<Vec<u8>, HandshakeError> {
        let mut msg = vec![0u8; 65535];
        let len = self.state.write_message(&[], &mut msg)?;
        msg.truncate(len);
        Ok(msg)
    }

    pub fn process_response(&mut self, server_msg: &[u8]) -> Result<(), HandshakeError> {
        let mut read_buf = vec![0u8; 65535];
        self.state.read_message(server_msg, &mut read_buf)?;
        Ok(())
    }

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
            ClientHandshake::new(&client_id, &server_id.x25519_public_bytes()).unwrap();
        let client_msg = client_hs.initiate().unwrap();

        let mut server_hs = ServerHandshake::new(&server_id).unwrap();
        let server_msg = server_hs.respond(&client_msg).unwrap();
        client_hs.process_response(&server_msg).unwrap();

        let server_result = server_hs.finalize().unwrap();
        let client_result = client_hs.finalize().unwrap();

        assert_eq!(
            server_result.remote_static.unwrap(),
            client_id.x25519_public_bytes()
        );

        // Verify encryption works
        let mut enc_buf = vec![0u8; 1024];
        let mut dec_buf = vec![0u8; 1024];
        let len = client_result
            .transport
            .write_message(b"hello", &mut enc_buf)
            .unwrap();
        let dec_len = server_result
            .transport
            .read_message(&enc_buf[..len], &mut dec_buf)
            .unwrap();
        assert_eq!(&dec_buf[..dec_len], b"hello");
    }

    #[test]
    fn wrong_server_key_fails() {
        let server_id = LocalIdentity::generate("Server");
        let client_id = LocalIdentity::generate("Client");
        let wrong_id = LocalIdentity::generate("Wrong");

        let mut client_hs =
            ClientHandshake::new(&client_id, &wrong_id.x25519_public_bytes()).unwrap();
        let client_msg = client_hs.initiate().unwrap();

        let mut server_hs = ServerHandshake::new(&server_id).unwrap();
        assert!(server_hs.respond(&client_msg).is_err());
    }

    #[test]
    fn finalize_before_complete_fails() {
        let id = LocalIdentity::generate("Server");
        let hs = ServerHandshake::new(&id).unwrap();
        assert!(matches!(hs.finalize(), Err(HandshakeError::NotComplete)));
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

Uses x25519 secret bytes from LocalIdentity (dual-keypair aware).
1 round trip: client sends ephemeral + encrypted static, server responds.
Server learns client's Curve25519 key for pairing lookup."
```

---

## Task 7: SecurityContext with Split Fast-Path / Slow-Path

**Files:**
- Create: `crates/prism-security/src/context.rs`

`ChannelDecision` (Copy-able, array-indexed) for fast path. `active_filters: HashMap` for slow path (Phase 3 content filters). The array lookup is O(1) ~2ns; the HashMap is only consulted for `CheckFilter` channels.

- [ ] **Step 1: Write SecurityContext with tests**

`crates/prism-security/src/context.rs`:
```rust
use std::collections::HashMap;
use std::sync::Arc;

use prism_protocol::channel::*;
use prism_protocol::header::PrismHeader;

use crate::filter::ContentFilter;
use crate::pairing::{PairingEntry, Permission};

/// Fast-path channel decision. Copy-able, stored in fixed-size array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDecision {
    /// No filtering — send immediately.
    AllowAll,
    /// Channel blocked for this client.
    Blocked,
    /// Needs user confirmation before first use this session.
    NeedsConfirmation,
    /// Look up in active_filters HashMap (slow path, Phase 3+).
    CheckFilter,
}

/// Pre-computed per-connection security decisions.
pub struct SecurityContext {
    pub device: Arc<PairingEntry>,
    /// Fast-path: array indexed by channel_id & 0xFF. O(1), ~2ns.
    pub channel_decisions: [ChannelDecision; 256],
    /// Slow-path: content filters for channels that need inspection (Phase 3+).
    pub active_filters: HashMap<u16, Arc<dyn ContentFilter>>,
    /// Per-channel 0-RTT safety.
    pub is_0rtt_safe: [bool; 256],
}

impl SecurityContext {
    /// Build for Phase 1: AllowAll for allowed channels, no active filters.
    pub fn for_device(entry: Arc<PairingEntry>) -> Self {
        let mut channel_decisions = [ChannelDecision::AllowAll; 256];
        let mut is_0rtt_safe = [false; 256];

        let perms = &entry.permissions;
        Self::apply_permission(&mut channel_decisions, CHANNEL_DISPLAY, perms.display);
        Self::apply_permission(&mut channel_decisions, CHANNEL_INPUT, perms.input);
        Self::apply_permission(&mut channel_decisions, CHANNEL_CLIPBOARD, perms.clipboard);
        Self::apply_permission(&mut channel_decisions, CHANNEL_FILESHARE, perms.fileshare);
        Self::apply_permission(&mut channel_decisions, CHANNEL_NOTIFY, perms.notify);
        Self::apply_permission(&mut channel_decisions, CHANNEL_CAMERA, perms.camera);
        Self::apply_permission(&mut channel_decisions, CHANNEL_SENSOR, perms.sensor);

        is_0rtt_safe[(CHANNEL_DISPLAY & 0xFF) as usize] = true;
        is_0rtt_safe[(CHANNEL_INPUT & 0xFF) as usize] = true;
        is_0rtt_safe[(CHANNEL_AUDIO & 0xFF) as usize] = true;

        Self {
            device: entry,
            channel_decisions,
            active_filters: HashMap::new(),
            is_0rtt_safe,
        }
    }

    fn apply_permission(
        decisions: &mut [ChannelDecision; 256],
        channel_id: u16,
        permission: Permission,
    ) {
        decisions[(channel_id & 0xFF) as usize] = match permission {
            Permission::Allow => ChannelDecision::AllowAll,
            Permission::Deny => ChannelDecision::Blocked,
            Permission::Ask => ChannelDecision::NeedsConfirmation,
        };
    }

    /// Fast-path channel decision. O(1), ~2ns.
    #[inline(always)]
    pub fn channel_decision(&self, channel_id: u16) -> ChannelDecision {
        self.channel_decisions[(channel_id & 0xFF) as usize]
    }

    /// Get content filter for a channel (slow path, Phase 3+).
    pub fn content_filter(&self, channel_id: u16) -> Option<&Arc<dyn ContentFilter>> {
        self.active_filters.get(&channel_id)
    }

    /// Check 0-RTT safety.
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
    fn default_allows_core_channels() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_decision(CHANNEL_DISPLAY), ChannelDecision::AllowAll);
        assert_eq!(ctx.channel_decision(CHANNEL_INPUT), ChannelDecision::AllowAll);
        assert_eq!(ctx.channel_decision(CHANNEL_CLIPBOARD), ChannelDecision::AllowAll);
    }

    #[test]
    fn ask_maps_to_needs_confirmation() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_decision(CHANNEL_CAMERA), ChannelDecision::NeedsConfirmation);
        assert_eq!(ctx.channel_decision(CHANNEL_SENSOR), ChannelDecision::NeedsConfirmation);
    }

    #[test]
    fn deny_maps_to_blocked() {
        let mut perms = ChannelPermissions::default();
        perms.display = Permission::Deny;
        let ctx = make_context(perms);
        assert_eq!(ctx.channel_decision(CHANNEL_DISPLAY), ChannelDecision::Blocked);
    }

    #[test]
    fn zero_rtt_safe_channels() {
        let ctx = make_context(ChannelPermissions::default());
        let h = |ch| PrismHeader {
            version: 0, channel_id: ch, msg_type: 0,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        assert!(ctx.is_0rtt_safe(&h(CHANNEL_DISPLAY)));
        assert!(ctx.is_0rtt_safe(&h(CHANNEL_INPUT)));
        assert!(ctx.is_0rtt_safe(&h(CHANNEL_AUDIO)));
        assert!(!ctx.is_0rtt_safe(&h(CHANNEL_CLIPBOARD)));
        assert!(!ctx.is_0rtt_safe(&h(CHANNEL_FILESHARE)));
    }

    #[test]
    fn no_active_filters_in_phase_1() {
        let ctx = make_context(ChannelPermissions::default());
        assert!(ctx.active_filters.is_empty());
        assert!(ctx.content_filter(CHANNEL_CLIPBOARD).is_none());
    }

    #[test]
    fn unknown_channel_defaults_to_allow() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_decision(0x100), ChannelDecision::AllowAll);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/context.rs
git commit -m "feat(security): SecurityContext with split fast-path/slow-path filters

ChannelDecision array (Copy, O(1) ~2ns) for fast path. HashMap<u16,
Arc<dyn ContentFilter>> for slow path (Phase 3 content filters).
Fast path: AllowAll/Blocked/NeedsConfirmation/CheckFilter.
Slow path only consulted for CheckFilter channels. 0-RTT safe for
Display/Input/Audio only."
```

---

## Task 8: Audit Log

**Files:**
- Create: `crates/prism-security/src/audit.rs`

Same as original plan — unchanged.

- [ ] **Step 1: Write audit log with tests**

`crates/prism-security/src/audit.rs`:
```rust
use std::collections::VecDeque;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum AuditEvent {
    ClientAuthenticated { device_id: Uuid, device_name: String },
    ClientRejected { device_id: Uuid, reason: String },
    ClientDisconnected { device_id: Uuid },
    KeyRotation { device_id: Uuid, accepted: bool },
    PairingAttempt { method: String, success: bool },
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: u64,
    pub event: AuditEvent,
}

pub struct AuditLog {
    entries: Mutex<VecDeque<AuditEntry>>,
    max_entries: usize,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(max_entries.min(1024))),
            max_entries,
        }
    }

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

    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

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
        let entries = log.entries();
        if let AuditEvent::ClientDisconnected { device_id } = &entries[0].event {
            assert_eq!(*device_id, Uuid::from_u128(2));
        }
    }

    #[test]
    fn empty_log() {
        let log = AuditLog::new(100);
        assert!(log.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/audit.rs
git commit -m "feat(security): audit log with ring buffer

AuditEvent: Authenticated, Rejected, Disconnected, KeyRotation, PairingAttempt.
Ring buffer, configurable max entries. Thread-safe via Mutex."
```

---

## Task 9: SecurityGate Trait and DefaultSecurityGate

**Files:**
- Create: `crates/prism-security/src/gate.rs`

Fixed: `AuthResult::Authenticated` holds `Arc<SecurityContext>`. No double creation.

- [ ] **Step 1: Write SecurityGate with tests**

`crates/prism-security/src/gate.rs`:
```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use thiserror::Error;
use uuid::Uuid;

use crate::audit::{AuditEvent, AuditLog};
use crate::context::SecurityContext;
use crate::handshake::{HandshakeError, ServerHandshake};
use crate::identity::{DeviceIdentity, LocalIdentity};
use crate::pairing::{PairingState, PairingStore};

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("handshake error: {0}")]
    Handshake(#[from] HandshakeError),
    #[error("unknown device")]
    UnknownDevice,
    #[error("device blocked")]
    DeviceBlocked,
}

/// Authentication result. Authenticated holds Arc<SecurityContext>.
pub enum AuthResult {
    Authenticated(Arc<SecurityContext>),
    SilentDrop,
    Blocked,
}

/// The security gate contract.
pub trait SecurityGate: Send + Sync {
    fn authenticate(
        &self,
        client_key: &[u8; 32],
        device_identity: &DeviceIdentity,
    ) -> AuthResult;

    fn security_context(&self, device_id: &Uuid) -> Option<Arc<SecurityContext>>;

    fn audit(&self, event: AuditEvent);
}

/// Phase 1 implementation.
pub struct DefaultSecurityGate {
    pairing: PairingStore,
    identity: LocalIdentity,
    audit_log: AuditLog,
    contexts: Mutex<HashMap<Uuid, Arc<SecurityContext>>>,
}

impl DefaultSecurityGate {
    pub fn new(pairing: PairingStore, identity: LocalIdentity, audit_log: AuditLog) -> Self {
        Self {
            pairing,
            identity,
            audit_log,
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub fn identity(&self) -> &LocalIdentity {
        &self.identity
    }

    pub fn pairing_store(&self) -> &PairingStore {
        &self.pairing
    }

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
            Some(entry) => match entry.state {
                PairingState::Paired => {
                    let ctx = Arc::new(SecurityContext::for_device(entry.clone()));
                    self.contexts
                        .lock()
                        .unwrap()
                        .insert(entry.device.device_id, ctx.clone());

                    self.audit(AuditEvent::ClientAuthenticated {
                        device_id: entry.device.device_id,
                        device_name: entry.device.display_name.clone(),
                    });

                    AuthResult::Authenticated(ctx)
                }
                PairingState::Blocked => {
                    self.audit(AuditEvent::ClientRejected {
                        device_id: entry.device.device_id,
                        reason: "blocked".to_string(),
                    });
                    AuthResult::Blocked
                }
            },
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
    fn unknown_device_silent_drop() {
        let (gate, client_id) = setup();
        let result = gate.authenticate(&client_id.x25519_public_bytes(), &client_id.identity);
        assert!(matches!(result, AuthResult::SilentDrop));
    }

    #[test]
    fn paired_device_authenticates() {
        let (gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        gate.pairing_store().add(entry).unwrap();

        let result = gate.authenticate(&client_id.x25519_public_bytes(), &client_id.identity);
        assert!(matches!(result, AuthResult::Authenticated(_)));
    }

    #[test]
    fn blocked_device_returns_blocked() {
        let (gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        let device_id = entry.device.device_id;
        gate.pairing_store().add(entry).unwrap();
        gate.pairing_store().block(&device_id).unwrap();

        let result = gate.authenticate(&client_id.x25519_public_bytes(), &client_id.identity);
        assert!(matches!(result, AuthResult::Blocked));
    }

    #[test]
    fn context_cached_after_auth() {
        let (gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        let device_id = entry.device.device_id;
        gate.pairing_store().add(entry).unwrap();

        gate.authenticate(&client_id.x25519_public_bytes(), &client_id.identity);
        assert!(gate.security_context(&device_id).is_some());
    }

    #[test]
    fn authenticated_returns_arc_security_context() {
        let (gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        let device_id = entry.device.device_id;
        gate.pairing_store().add(entry).unwrap();

        if let AuthResult::Authenticated(ctx) = gate.authenticate(
            &client_id.x25519_public_bytes(),
            &client_id.identity,
        ) {
            // The returned Arc is the same as the cached one
            let cached = gate.security_context(&device_id).unwrap();
            assert!(Arc::ptr_eq(&ctx, &cached));
        } else {
            panic!("expected Authenticated");
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-security`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-security/src/gate.rs
git commit -m "feat(security): SecurityGate trait with DefaultSecurityGate

AuthResult::Authenticated(Arc<SecurityContext>) — single creation, cached
and returned as the same Arc. SecurityGate trait: authenticate,
security_context, audit. PairingStore accessed via &self (thread-safe)."
```

---

## Task 10: Key Rotation with Ed25519

**Files:**
- Create: `crates/prism-security/src/key_rotation.rs`

Uses `LocalIdentity.ed25519_signing_key()` for signing. Verification against `DeviceIdentity.signing_key`.

- [ ] **Step 1: Write key rotation with tests**

`crates/prism-security/src/key_rotation.rs`:
```rust
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::identity::LocalIdentity;

#[derive(Debug, Error)]
pub enum KeyRotationError {
    #[error("signature verification failed: {0}")]
    VerificationFailed(#[from] ed25519_dalek::SignatureError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotation {
    pub device_id: Uuid,
    pub new_public_key: [u8; 32],
    pub old_key_signature: [u8; 64],
    pub timestamp: u64,
}

impl KeyRotation {
    /// Create a rotation, signing the new key with the device's Ed25519 key.
    pub fn create(identity: &LocalIdentity, new_public_key: [u8; 32]) -> Self {
        let signature = identity.ed25519_signing_key().sign(&new_public_key);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            device_id: identity.device_id(),
            new_public_key,
            old_key_signature: signature.to_bytes(),
            timestamp: now,
        }
    }

    /// Verify the rotation against the device's known Ed25519 verifying key.
    pub fn verify(&self, ed25519_verifying_bytes: &[u8; 32]) -> Result<(), KeyRotationError> {
        let verifying_key = VerifyingKey::from_bytes(ed25519_verifying_bytes)?;
        let signature = ed25519_dalek::Signature::from_bytes(&self.old_key_signature);
        verifying_key.verify(&self.new_public_key, &signature)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_verify() {
        let id = LocalIdentity::generate("Test");
        let new_key = [0xABu8; 32];
        let rotation = KeyRotation::create(&id, new_key);
        assert!(rotation.verify(&id.ed25519_verifying_bytes()).is_ok());
    }

    #[test]
    fn wrong_verifying_key_fails() {
        let id = LocalIdentity::generate("Test");
        let other = LocalIdentity::generate("Other");
        let rotation = KeyRotation::create(&id, [0xABu8; 32]);
        assert!(rotation.verify(&other.ed25519_verifying_bytes()).is_err());
    }

    #[test]
    fn tampered_new_key_fails() {
        let id = LocalIdentity::generate("Test");
        let mut rotation = KeyRotation::create(&id, [0xABu8; 32]);
        rotation.new_public_key = [0xCDu8; 32];
        assert!(rotation.verify(&id.ed25519_verifying_bytes()).is_err());
    }

    #[test]
    fn json_roundtrip() {
        let id = LocalIdentity::generate("Test");
        let rotation = KeyRotation::create(&id, [0xABu8; 32]);
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

Uses LocalIdentity.ed25519_signing_key() for signing. Verification
against DeviceIdentity.signing_key (Ed25519 verifying key bytes).
Tamper-proof: changing the new key invalidates the signature."
```

---

## Plan Self-Review

**1. Spec coverage:**
- Section 1 (Identity): Task 2 — dual keypairs (Curve25519 + Ed25519), UUIDv7, platform.
- Section 2 (Pairing): Task 5 — PairingStore with arc-swap, encrypted file, thread-safe &self methods.
- Section 3 (Methods): Task 5 — manual + SPAKE2 stub. PairingMethod enum.
- Section 4 (Key Rotation): Task 10 — Ed25519 signature using LocalIdentity's signing key.
- Section 5 (Auth): Task 6 (Noise NK). Task 9 (SecurityGate).
- Section 7 (0-RTT): Task 7 — is_0rtt_safe per channel in SecurityContext.
- Section 9 (Filters): Task 4 — ContentFilter trait + FilterResult defined. Task 7 — ChannelDecision::CheckFilter + active_filters HashMap.
- Section 13 (Crypto): Task 3 — HKDF, AES-GCM, entropy.
- Section 14 (Audit): Task 8 — AuditEvent + ring buffer.
- Section 15 (SecurityGate): Task 9 — full trait + DefaultSecurityGate.
- Section 2.2 (Encrypted store): Task 3 (AES-GCM) + Task 5 (encrypted persistence).
- Thread safety: Task 5 (arc-swap PairingStore). Task 9 (Mutex contexts cache).
- AuthResult bug: Task 9 — Arc<SecurityContext>, verified ptr_eq in test.

**2. Placeholder scan:** No TBDs. SPAKE2 is explicitly a stub with working code generation.

**3. Type consistency:**
- `LocalIdentity` uses `x25519_secret_bytes()` in Tasks 2, 6. `ed25519_signing_key()` in Tasks 2, 10.
- `PairingStore` methods take `&self` in Tasks 5, 9. Thread-safe.
- `SecurityContext` uses `ChannelDecision` (not `ChannelFilterState`) in Task 7. `for_device()` returns owned value, wrapped in `Arc` by Task 9's gate.
- `AuthResult::Authenticated(Arc<SecurityContext>)` in Task 9. Test verifies `Arc::ptr_eq`.
- `AuditEvent` variants match between Tasks 8 and 9.
- `ContentFilter` trait in Task 4. `active_filters: HashMap<u16, Arc<dyn ContentFilter>>` in Task 7.

No issues found.
