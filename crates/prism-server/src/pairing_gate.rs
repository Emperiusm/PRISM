//! Trust-on-first-use security gate wrapper.
//!
//! [`TofuGate`] wraps any [`SecurityGate`] and tracks the number of
//! [`AuthResult::SilentDrop`] responses it observes.  Each drop represents an
//! unknown device that could be a TOFU pairing candidate; the actual pairing
//! logic lives in `main.rs` — this wrapper simply surfaces the count and logs a
//! warning so the operator knows an auto-pair attempt occurred.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use prism_security::audit::AuditEvent;
use prism_security::context::SecurityContext;
use prism_security::gate::{AuthResult, SecurityGate};
use prism_security::identity::DeviceIdentity;
use uuid::Uuid;

// ── TofuGate ──────────────────────────────────────────────────────────────────

/// Wraps a [`SecurityGate`] and counts auto-pair attempts (SilentDrop results).
///
/// All method calls are delegated to the inner gate unchanged.  The only
/// additional behaviour is: when `authenticate()` returns `SilentDrop`, the
/// `auto_paired` counter is incremented and a warning is printed to stderr.
pub struct TofuGate<G: SecurityGate> {
    inner: G,
    auto_paired: AtomicU32,
}

impl<G: SecurityGate> TofuGate<G> {
    /// Wrap `inner` in a TOFU counter gate.
    pub fn new(inner: G) -> Self {
        Self { inner, auto_paired: AtomicU32::new(0) }
    }

    /// Number of `SilentDrop` results observed since creation.
    pub fn auto_paired_count(&self) -> u32 {
        self.auto_paired.load(Ordering::SeqCst)
    }
}

impl<G: SecurityGate> SecurityGate for TofuGate<G> {
    fn authenticate(&self, client_key: &[u8; 32], device_identity: &DeviceIdentity) -> AuthResult {
        let result = self.inner.authenticate(client_key, device_identity);
        match result {
            AuthResult::SilentDrop => {
                self.auto_paired.fetch_add(1, Ordering::SeqCst);
                eprintln!(
                    "[TofuGate] WARN: unknown device \"{}\" ({}) — SilentDrop (TOFU candidate)",
                    device_identity.display_name, device_identity.device_id,
                );
                AuthResult::SilentDrop
            }
            other => other,
        }
    }

    fn security_context(&self, device_id: &Uuid) -> Option<Arc<SecurityContext>> {
        self.inner.security_context(device_id)
    }

    fn audit(&self, event: AuditEvent) {
        self.inner.audit(event);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allow_all_gate::AllowAllGate;

    // ── Test helper: RejectAllGate ────────────────────────────────────────────

    /// A [`SecurityGate`] that always returns `SilentDrop`.  Used only in tests
    /// to exercise `TofuGate`'s counter without needing a real pairing store.
    struct RejectAllGate;

    impl SecurityGate for RejectAllGate {
        fn authenticate(&self, _: &[u8; 32], _: &DeviceIdentity) -> AuthResult {
            AuthResult::SilentDrop
        }

        fn security_context(&self, _: &Uuid) -> Option<Arc<SecurityContext>> {
            None
        }

        fn audit(&self, _: AuditEvent) {}
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_identity() -> DeviceIdentity {
        prism_security::identity::LocalIdentity::generate("Test Client").identity
    }

    fn dummy_key() -> [u8; 32] {
        [0u8; 32]
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// When the inner gate always returns `Authenticated`, the TOFU counter
    /// stays at zero and the result passes through unchanged.
    #[test]
    fn wraps_allow_all() {
        let gate = TofuGate::new(AllowAllGate::new());
        let identity = make_identity();
        let result = gate.authenticate(&dummy_key(), &identity);
        assert!(
            matches!(result, AuthResult::Authenticated(_)),
            "TofuGate must pass Authenticated through unchanged"
        );
        assert_eq!(gate.auto_paired_count(), 0, "no SilentDrop — count must stay 0");
    }

    /// When the inner gate returns `SilentDrop`, the TOFU counter is incremented
    /// and the `SilentDrop` is returned to the caller.
    #[test]
    fn counts_silent_drop() {
        let gate = TofuGate::new(RejectAllGate);
        let identity = make_identity();
        let result = gate.authenticate(&dummy_key(), &identity);
        assert!(
            matches!(result, AuthResult::SilentDrop),
            "TofuGate must preserve SilentDrop"
        );
        assert_eq!(gate.auto_paired_count(), 1, "one SilentDrop must increment counter to 1");

        // A second call increments to 2.
        gate.authenticate(&dummy_key(), &identity);
        assert_eq!(gate.auto_paired_count(), 2);
    }

    /// Calling `audit()` on the gate must not panic regardless of event variant.
    #[test]
    fn delegates_audit() {
        let gate = TofuGate::new(AllowAllGate::new());
        gate.audit(AuditEvent::ClientAuthenticated {
            device_id: Uuid::nil(),
            device_name: "test".to_string(),
        });
        gate.audit(AuditEvent::ClientRejected {
            device_id: Uuid::nil(),
            reason: "blocked".to_string(),
        });
        gate.audit(AuditEvent::ClientDisconnected { device_id: Uuid::nil() });
        // No assertion needed — must not panic.
    }
}
