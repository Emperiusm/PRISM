// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use prism_protocol::clipboard::{ClipboardEchoGuard, fast_hash};

// ── ClipboardSyncState ────────────────────────────────────────────────────────

/// Per-session clipboard synchronisation state.
///
/// Combines echo suppression (avoid bouncing our own sends back),
/// hash-based deduplication (skip re-sending unchanged content),
/// and counters for observability.
pub struct ClipboardSyncState {
    echo_guard: ClipboardEchoGuard,
    last_text_hash: AtomicU64,
    messages_sent: AtomicU32,
    messages_received: AtomicU32,
}

impl ClipboardSyncState {
    /// Create a fresh state with zero counters and no remembered hash.
    pub fn new() -> Self {
        Self {
            echo_guard: ClipboardEchoGuard::new(),
            last_text_hash: AtomicU64::new(0),
            messages_sent: AtomicU32::new(0),
            messages_received: AtomicU32::new(0),
        }
    }

    // ── Send-path decisions ───────────────────────────────────────────────────

    /// Decide whether `text` should be forwarded to the remote peer.
    ///
    /// Returns `false` when:
    /// - `text` is empty,
    /// - the echo guard identifies it as content we sent ourselves, or
    /// - it hashes to the same value as the last text we sent (dedup).
    ///
    /// When `true` is returned the hash is stored as the new `last_text_hash`
    /// so that a duplicate call for the same content is suppressed immediately.
    /// This makes `should_send_text` a check-and-mark operation.
    pub fn should_send_text(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        let hash = fast_hash(text.as_bytes());

        // Echo suppression: did we send this exact content recently?
        if !self.echo_guard.should_send(hash) {
            return false;
        }

        // Dedup: skip if identical to the last outbound payload.
        if self.last_text_hash.load(Ordering::Relaxed) == hash {
            return false;
        }

        // Mark this hash as the last-sent so a subsequent identical call is
        // suppressed (check-and-mark semantics).
        self.last_text_hash.store(hash, Ordering::Relaxed);

        true
    }

    /// Record that we are about to send `data` to the remote peer.
    ///
    /// Stores the hash in both the echo guard (so an echoed-back copy is
    /// suppressed) and `last_text_hash` (for dedup).
    pub fn remember_set(&self, data: &[u8]) {
        let hash = fast_hash(data);
        self.echo_guard.remember(hash);
        self.last_text_hash.store(hash, Ordering::Relaxed);
    }

    // ── Counters ──────────────────────────────────────────────────────────────

    /// Increment the outbound message counter by one.
    pub fn record_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the inbound message counter by one.
    pub fn record_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Return the total number of messages sent so far.
    pub fn messages_sent(&self) -> u32 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Return the total number of messages received so far.
    pub fn messages_received(&self) -> u32 {
        self.messages_received.load(Ordering::Relaxed)
    }
}

impl Default for ClipboardSyncState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1. New text that has never been sent should be forwarded.
    #[test]
    fn detects_new_text() {
        let state = ClipboardSyncState::new();
        assert!(state.should_send_text("hello from clipboard"));
    }

    // 2. Sending the same text twice is suppressed on the second call.
    #[test]
    fn suppresses_same_text() {
        let state = ClipboardSyncState::new();
        let text = "clipboard content";

        // Simulate the first successful send.
        assert!(state.should_send_text(text));
        state.remember_set(text.as_bytes());

        // Identical content must be suppressed.
        assert!(!state.should_send_text(text));
    }

    // 3. Changed text is detected after a previous send.
    #[test]
    fn detects_changed_text() {
        let state = ClipboardSyncState::new();
        let old = "old clipboard text";
        let new = "new clipboard text";

        state.remember_set(old.as_bytes());

        // New content must pass through.
        assert!(state.should_send_text(new));
    }

    // 4. Content we sent (echo guard) is suppressed when it bounces back.
    #[test]
    fn suppresses_echo() {
        let state = ClipboardSyncState::new();
        let text = "echoed text";

        // Simulate sending; echo guard records the hash.
        state.remember_set(text.as_bytes());

        // The same content arriving from the peer must be suppressed.
        assert!(!state.should_send_text(text));
    }

    // 5. Empty strings are never forwarded.
    #[test]
    fn empty_not_sent() {
        let state = ClipboardSyncState::new();
        assert!(!state.should_send_text(""));
    }

    // 6. Sent/received counters are independently tracked.
    #[test]
    fn stats_tracking() {
        let state = ClipboardSyncState::new();

        assert_eq!(state.messages_sent(), 0);
        assert_eq!(state.messages_received(), 0);

        state.record_sent();
        state.record_sent();
        state.record_sent();
        state.record_received();

        assert_eq!(state.messages_sent(), 3);
        assert_eq!(state.messages_received(), 1);
    }
}
