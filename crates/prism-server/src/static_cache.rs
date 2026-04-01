// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use prism_display::atlas::{RegionKey, StaticDecision};
use serde::{Deserialize, Serialize};

/// Cache instruction sent to client alongside frame data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CacheInstruction {
    /// Client should cache this region's data at the given key.
    Cache { key: (u16, u8), hash: u64 },
    /// Client should use its cached version (no new data sent).
    UseCached { key: (u16, u8) },
    /// Encode normally (no caching).
    None,
}

impl CacheInstruction {
    /// Convert a StaticDecision to a CacheInstruction.
    pub fn from_decision(decision: StaticDecision, key: RegionKey, hash: u64) -> Self {
        match decision {
            StaticDecision::SendAndCache => CacheInstruction::Cache {
                key: (key.0, key.1),
                hash,
            },
            StaticDecision::Unchanged => CacheInstruction::UseCached {
                key: (key.0, key.1),
            },
            StaticDecision::EncodeNormally => CacheInstruction::None,
        }
    }

    pub fn is_cached(&self) -> bool {
        matches!(self, CacheInstruction::UseCached { .. })
    }

    pub fn is_cache_store(&self) -> bool {
        matches!(self, CacheInstruction::Cache { .. })
    }
}

/// Estimate bandwidth savings from caching.
pub struct CacheSavingsTracker {
    frames_cached: u32,
    frames_total: u32,
    bytes_saved: u64,
}

impl CacheSavingsTracker {
    pub fn new() -> Self {
        Self {
            frames_cached: 0,
            frames_total: 0,
            bytes_saved: 0,
        }
    }

    pub fn record(&mut self, instruction: &CacheInstruction, region_bytes: u64) {
        self.frames_total += 1;
        if instruction.is_cached() {
            self.frames_cached += 1;
            self.bytes_saved += region_bytes;
        }
    }

    pub fn cache_hit_rate(&self) -> f32 {
        if self.frames_total > 0 {
            self.frames_cached as f32 / self.frames_total as f32
        } else {
            0.0
        }
    }

    pub fn bytes_saved(&self) -> u64 {
        self.bytes_saved
    }
}

impl Default for CacheSavingsTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_send_and_cache() {
        let key = RegionKey(3, 7);
        let instr = CacheInstruction::from_decision(StaticDecision::SendAndCache, key, 0xDEAD);
        assert_eq!(
            instr,
            CacheInstruction::Cache {
                key: (3, 7),
                hash: 0xDEAD
            }
        );
        assert!(instr.is_cache_store());
        assert!(!instr.is_cached());
    }

    #[test]
    fn from_unchanged() {
        let key = RegionKey(1, 2);
        let instr = CacheInstruction::from_decision(StaticDecision::Unchanged, key, 0);
        assert_eq!(instr, CacheInstruction::UseCached { key: (1, 2) });
        assert!(instr.is_cached());
        assert!(!instr.is_cache_store());
    }

    #[test]
    fn from_encode_normally() {
        let key = RegionKey(0, 0);
        let instr = CacheInstruction::from_decision(StaticDecision::EncodeNormally, key, 0);
        assert_eq!(instr, CacheInstruction::None);
        assert!(!instr.is_cached());
        assert!(!instr.is_cache_store());
    }

    #[test]
    fn cache_savings_tracking() {
        let mut tracker = CacheSavingsTracker::new();
        let cached_instr = CacheInstruction::UseCached { key: (0, 0) };
        let normal_instr = CacheInstruction::None;

        for _ in 0..6 {
            tracker.record(&cached_instr, 1000);
        }
        for _ in 0..4 {
            tracker.record(&normal_instr, 0);
        }

        let rate = tracker.cache_hit_rate();
        assert!(
            (rate - 0.6).abs() < f32::EPSILON,
            "expected 0.6, got {rate}"
        );
        assert_eq!(tracker.bytes_saved(), 6 * 1000);
    }

    #[test]
    fn cache_instruction_json_roundtrip() {
        let instructions = vec![
            CacheInstruction::Cache {
                key: (5, 3),
                hash: 0xCAFE_BABE,
            },
            CacheInstruction::UseCached { key: (2, 1) },
            CacheInstruction::None,
        ];

        for instr in &instructions {
            let json = serde_json::to_string(instr).expect("serialization failed");
            let recovered: CacheInstruction =
                serde_json::from_str(&json).expect("deserialization failed");
            assert_eq!(&recovered, instr, "roundtrip failed for {instr:?}");
        }
    }
}
