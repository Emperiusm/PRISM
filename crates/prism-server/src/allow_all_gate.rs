// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Development-mode security gate that authenticates all clients.
//!
//! `AllowAllGate` implements [`SecurityGate`] without any pairing or key
//! verification checks.  Every `authenticate()` call succeeds and returns a
//! [`SecurityContext`] where every channel decision is
//! [`ChannelDecision::AllowAll`].
//!
//! **Never use in production.**

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use prism_security::audit::AuditEvent;
use prism_security::context::SecurityContext;
use prism_security::gate::{AuthResult, SecurityGate};
use prism_security::identity::DeviceIdentity;
use prism_security::pairing::{ChannelPermissions, PairingEntry, PairingState, Permission};
use uuid::Uuid;

// ── AllowAllGate ──────────────────────────────────────────────────────────────

/// Dev-mode [`SecurityGate`] that authenticates every client unconditionally.
pub struct AllowAllGate {
    /// Running count of calls to `authenticate()`.
    auth_count: AtomicU32,
}

impl AllowAllGate {
    pub fn new() -> Self {
        Self { auth_count: AtomicU32::new(0) }
    }

    /// Number of successful authentications since creation.
    pub fn auth_count(&self) -> u32 {
        self.auth_count.load(Ordering::SeqCst)
    }
}

impl Default for AllowAllGate {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityGate for AllowAllGate {
    fn authenticate(
        &self,
        _client_key: &[u8; 32],
        device_identity: &DeviceIdentity,
    ) -> AuthResult {
        self.auth_count.fetch_add(1, Ordering::SeqCst);

        // Build an all-allow ChannelPermissions.
        let permissions = ChannelPermissions {
            display: Permission::Allow,
            input: Permission::Allow,
            clipboard: Permission::Allow,
            fileshare: Permission::Allow,
            notify: Permission::Allow,
            camera: Permission::Allow,
            sensor: Permission::Allow,
            filesystem_browse: Permission::Allow,
        };

        let entry = Arc::new(PairingEntry {
            device: device_identity.clone(),
            state: PairingState::Paired,
            permissions,
            paired_at: 0,
            last_seen: 0,
        });

        let ctx = Arc::new(SecurityContext::for_device(entry));
        AuthResult::Authenticated(ctx)
    }

    fn security_context(&self, _device_id: &Uuid) -> Option<Arc<SecurityContext>> {
        // AllowAllGate does not cache contexts — callers that need a context
        // should keep the Arc returned from `authenticate()`.
        None
    }

    fn audit(&self, _event: AuditEvent) {
        // Intentional no-op: dev-mode gate discards all audit events.
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_security::context::ChannelDecision;
    use prism_security::gate::AuthResult;
    use prism_protocol::channel::{
        CHANNEL_CAMERA, CHANNEL_CLIPBOARD, CHANNEL_DISPLAY, CHANNEL_INPUT, CHANNEL_SENSOR,
    };

    fn make_identity() -> DeviceIdentity {
        prism_security::identity::LocalIdentity::generate("Test Client").identity
    }

    fn dummy_key() -> [u8; 32] {
        [0u8; 32]
    }

    #[test]
    fn authenticates_any_client() {
        let gate = AllowAllGate::new();
        let identity = make_identity();
        let result = gate.authenticate(&dummy_key(), &identity);
        assert!(
            matches!(result, AuthResult::Authenticated(_)),
            "AllowAllGate must always return Authenticated"
        );
    }

    #[test]
    fn counts_authentications() {
        let gate = AllowAllGate::new();
        let identity = make_identity();
        gate.authenticate(&dummy_key(), &identity);
        gate.authenticate(&dummy_key(), &identity);
        assert_eq!(gate.auth_count(), 2);
    }

    #[test]
    fn context_has_allow_all_decisions() {
        let gate = AllowAllGate::new();
        let identity = make_identity();
        let ctx = match gate.authenticate(&dummy_key(), &identity) {
            AuthResult::Authenticated(ctx) => ctx,
            other => panic!("expected Authenticated, got {:?}", std::mem::discriminant(&other)),
        };

        // All well-known channels must be AllowAll.
        for &channel in &[
            CHANNEL_DISPLAY,
            CHANNEL_INPUT,
            CHANNEL_CLIPBOARD,
            CHANNEL_CAMERA,
            CHANNEL_SENSOR,
        ] {
            assert_eq!(
                ctx.channel_decision(channel),
                ChannelDecision::AllowAll,
                "channel 0x{channel:04X} must be AllowAll"
            );
        }
    }

    #[test]
    fn audit_is_noop() {
        let gate = AllowAllGate::new();
        // Must not panic for any variant.
        gate.audit(AuditEvent::ClientAuthenticated {
            device_id: Uuid::nil(),
            device_name: "test".to_string(),
        });
        gate.audit(AuditEvent::ClientRejected {
            device_id: Uuid::nil(),
            reason: "none".to_string(),
        });
        gate.audit(AuditEvent::ClientDisconnected { device_id: Uuid::nil() });
        // No assertion needed — just must not panic.
    }
}
