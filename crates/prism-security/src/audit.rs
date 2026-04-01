// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

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
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= self.max_entries { entries.pop_front(); }
        entries.push_back(AuditEntry { timestamp, event });
    }

    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    pub fn len(&self) -> usize { self.entries.lock().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.entries.lock().unwrap().is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        let log = AuditLog::new(100);
        log.record(AuditEvent::ClientAuthenticated {
            device_id: Uuid::nil(), device_name: "Test".to_string(),
        });
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let log = AuditLog::new(3);
        for i in 0..5 {
            log.record(AuditEvent::ClientDisconnected { device_id: Uuid::from_u128(i) });
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
