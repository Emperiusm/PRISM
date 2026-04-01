// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

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

pub enum AuthResult {
    Authenticated(Arc<SecurityContext>),
    SilentDrop,
    Blocked,
}

pub trait SecurityGate: Send + Sync {
    fn authenticate(&self, client_key: &[u8; 32], device_identity: &DeviceIdentity) -> AuthResult;
    fn security_context(&self, device_id: &Uuid) -> Option<Arc<SecurityContext>>;
    fn audit(&self, event: AuditEvent);
}

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
    fn authenticate(&self, client_key: &[u8; 32], device_identity: &DeviceIdentity) -> AuthResult {
        let snapshot = self.pairing.snapshot();
        match snapshot.by_key.get(client_key) {
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
                PairingState::Pending => {
                    self.audit(AuditEvent::ClientRejected {
                        device_id: entry.device.device_id,
                        reason: "pending".to_string(),
                    });
                    AuthResult::SilentDrop
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
        gate.pairing_store().block(device_id).unwrap();
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
    fn authenticated_returns_same_arc_as_cached() {
        let (gate, client_id) = setup();
        let entry = pair_manually(client_id.identity.clone());
        let device_id = entry.device.device_id;
        gate.pairing_store().add(entry).unwrap();
        if let AuthResult::Authenticated(ctx) =
            gate.authenticate(&client_id.x25519_public_bytes(), &client_id.identity)
        {
            let cached = gate.security_context(&device_id).unwrap();
            assert!(Arc::ptr_eq(&ctx, &cached));
        } else {
            panic!("expected Authenticated");
        }
    }
}
