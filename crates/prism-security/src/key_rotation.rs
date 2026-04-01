// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use ed25519_dalek::{Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::identity::LocalIdentity;

#[derive(Debug, Error)]
pub enum KeyRotationError {
    #[error("signature verification failed: {0}")]
    VerificationFailed(#[from] ed25519_dalek::SignatureError),
}

mod hex_sig {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(sig: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&hex::encode(sig))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where D: Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom(format!("expected 64 bytes, got {}", bytes.len())));
        }
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&bytes);
        Ok(sig)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotation {
    pub device_id: Uuid,
    pub new_public_key: [u8; 32],
    #[serde(with = "hex_sig")]
    pub old_key_signature: [u8; 64],
    pub timestamp: u64,
}

impl KeyRotation {
    pub fn create(identity: &LocalIdentity, new_public_key: [u8; 32]) -> Self {
        let signature = identity.ed25519_signing_key().sign(&new_public_key);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        Self {
            device_id: identity.device_id(),
            new_public_key,
            old_key_signature: signature.to_bytes(),
            timestamp: now,
        }
    }

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
        let rotation = KeyRotation::create(&id, [0xABu8; 32]);
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
