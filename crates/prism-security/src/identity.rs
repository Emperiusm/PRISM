// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

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
        {
            Platform::Windows
        }
        #[cfg(target_os = "macos")]
        {
            Platform::MacOS
        }
        #[cfg(target_os = "linux")]
        {
            Platform::Linux
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Platform::Linux
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub device_id: Uuid,
    pub display_name: String,
    pub platform: Platform,
    #[serde(with = "hex_key")]
    pub current_key: [u8; 32],
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

pub struct LocalIdentity {
    x25519_secret: X25519Secret,
    x25519_public: X25519PublicKey,
    ed25519_signing: Ed25519SigningKey,
    ed25519_verifying: Ed25519VerifyingKey,
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
        // Restrict file permissions — secret keys should only be readable by owner
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn x25519_public_bytes(&self) -> [u8; 32] {
        *self.x25519_public.as_bytes()
    }
    pub fn x25519_secret_bytes(&self) -> [u8; 32] {
        self.x25519_secret.to_bytes()
    }
    pub fn ed25519_signing_key(&self) -> &Ed25519SigningKey {
        &self.ed25519_signing
    }
    pub fn ed25519_verifying_bytes(&self) -> [u8; 32] {
        self.ed25519_verifying.to_bytes()
    }
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
        assert_eq!(id.identity.current_key, id.x25519_public_bytes());
        assert_eq!(id.identity.signing_key, id.ed25519_verifying_bytes());
    }

    #[test]
    fn two_identities_are_different() {
        let id1 = LocalIdentity::generate("Device 1");
        let id2 = LocalIdentity::generate("Device 2");
        assert_ne!(id1.x25519_public_bytes(), id2.x25519_public_bytes());
        assert_ne!(id1.ed25519_verifying_bytes(), id2.ed25519_verifying_bytes());
    }

    #[test]
    fn load_or_generate_creates_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");
        let id1 = LocalIdentity::load_or_generate(&path, "Test").unwrap();
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
