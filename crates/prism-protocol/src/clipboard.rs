// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

// ── ClipboardFormat ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardFormat {
    Text,
    Html,
    Image,
}

// ── ClipboardMessage ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardMessage {
    pub format: ClipboardFormat,
    pub data: Vec<u8>,
    pub content_hash: u64,
}

impl ClipboardMessage {
    /// Convenience constructor for plain-text clipboard content.
    pub fn text(s: &str) -> Self {
        let data = s.as_bytes().to_vec();
        let content_hash = fast_hash(&data);
        Self {
            format: ClipboardFormat::Text,
            data,
            content_hash,
        }
    }

    /// Extract UTF-8 text if the format is `Text`.
    pub fn text_content(&self) -> Option<String> {
        if self.format == ClipboardFormat::Text {
            String::from_utf8(self.data.clone()).ok()
        } else {
            None
        }
    }

    /// Serialize to JSON bytes.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("ClipboardMessage serialization is infallible")
    }

    /// Deserialize from JSON bytes; returns `None` on any error.
    pub fn from_json(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

// ── fast_hash ─────────────────────────────────────────────────────────────────

/// FNV-1a 64-bit hash — cheap, deterministic, suitable for echo suppression.
pub fn fast_hash(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;

    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ── ClipboardEchoGuard ────────────────────────────────────────────────────────

/// Remembers the last hash we *sent* so we can suppress echoed-back updates.
pub struct ClipboardEchoGuard {
    last_sent: AtomicU64,
}

impl ClipboardEchoGuard {
    pub fn new() -> Self {
        Self {
            last_sent: AtomicU64::new(0),
        }
    }

    /// Record a hash we are about to send.
    pub fn remember(&self, hash: u64) {
        self.last_sent.store(hash, Ordering::Relaxed);
    }

    /// Returns `true` if this hash is *different* from the last sent hash,
    /// i.e. the frame should be forwarded (not suppressed as an echo).
    pub fn should_send(&self, hash: u64) -> bool {
        self.last_sent.load(Ordering::Relaxed) != hash
    }
}

impl Default for ClipboardEchoGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roundtrip() {
        let msg = ClipboardMessage::text("hello, world");
        assert_eq!(msg.format, ClipboardFormat::Text);
        assert_eq!(msg.text_content(), Some("hello, world".to_string()));

        let json = msg.to_json();
        let decoded = ClipboardMessage::from_json(&json).expect("decode failed");
        assert_eq!(decoded.text_content(), Some("hello, world".to_string()));
        assert_eq!(decoded.content_hash, msg.content_hash);
    }

    #[test]
    fn fast_hash_deterministic() {
        let data = b"clipboard payload";
        assert_eq!(fast_hash(data), fast_hash(data));
        // Different inputs must not collide.
        assert_ne!(fast_hash(data), fast_hash(b"different payload"));
    }

    #[test]
    fn fast_hash_empty() {
        // Empty slice should not panic and should return the FNV offset.
        let h = fast_hash(&[]);
        assert_eq!(h, 14_695_981_039_346_656_037u64);
    }

    #[test]
    fn echo_guard_suppresses() {
        let guard = ClipboardEchoGuard::new();
        let hash = fast_hash(b"some text");
        guard.remember(hash);
        // Same hash → echo → should NOT send.
        assert!(!guard.should_send(hash));
    }

    #[test]
    fn echo_guard_allows_after_change() {
        let guard = ClipboardEchoGuard::new();
        let hash_a = fast_hash(b"old text");
        let hash_b = fast_hash(b"new text");
        guard.remember(hash_a);
        // Different hash → new content → should send.
        assert!(guard.should_send(hash_b));
    }

    #[test]
    fn format_serialize() {
        let formats = [ClipboardFormat::Text, ClipboardFormat::Html, ClipboardFormat::Image];
        for fmt in &formats {
            let json = serde_json::to_string(fmt).expect("serialize");
            let back: ClipboardFormat = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*fmt, back);
        }
    }
}
