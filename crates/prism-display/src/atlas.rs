// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Key type
// ---------------------------------------------------------------------------

/// Identifies a unique region within a display channel.
///
/// `(channel, region_index)` — both values are compact indices suitable for
/// use as `HashMap` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionKey(pub u16, pub u8);

// ---------------------------------------------------------------------------
// Decision type
// ---------------------------------------------------------------------------

/// What the atlas tracker recommends for this region this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticDecision {
    /// Region hash is the same and a cached copy exists — skip sending.
    Unchanged,
    /// Region is newly stable; encode once and push into the remote cache.
    SendAndCache,
    /// Region changed or hasn't stabilised yet — encode normally.
    EncodeNormally,
}

// ---------------------------------------------------------------------------
// Tracker
// ---------------------------------------------------------------------------

/// Tracks which display regions are "static" (content unchanged for several
/// consecutive frames) so the encode pipeline can skip re-encoding them.
///
/// When a region's content hash is stable for `cache_threshold` consecutive
/// frames the tracker emits `SendAndCache` once, then `Unchanged` for all
/// subsequent frames until the content changes.  If the cache is full the
/// least-recently-seen entry is evicted (LRU by `frame_counter` value).
pub struct StaticAtlasTracker {
    /// Last known content hash per region.
    region_hashes: HashMap<RegionKey, u64>,
    /// How many consecutive frames this region has had the same hash.
    static_frame_count: HashMap<RegionKey, u32>,
    /// Regions whose content has been pushed to the remote cache.
    /// Maps key → the hash value that was cached.
    cached_regions: HashMap<RegionKey, u64>,
    /// Frame counter at which each region was last seen (for LRU eviction).
    last_seen: HashMap<RegionKey, u64>,
    /// How many stable frames are required before caching.
    cache_threshold: u32,
    /// Maximum number of simultaneously cached regions.
    max_regions: usize,
    /// Monotonically increasing frame counter (incremented on each `check`
    /// that might change state).
    frame_counter: u64,
}

impl StaticAtlasTracker {
    /// Create a tracker with the given stability threshold and a default cap
    /// of 256 cached regions.
    pub fn new(cache_threshold: u32) -> Self {
        Self::with_max_regions(cache_threshold, 256)
    }

    /// Create a tracker with explicit cache-size cap.
    pub fn with_max_regions(cache_threshold: u32, max_regions: usize) -> Self {
        Self {
            region_hashes: HashMap::new(),
            static_frame_count: HashMap::new(),
            cached_regions: HashMap::new(),
            last_seen: HashMap::new(),
            cache_threshold,
            max_regions,
            frame_counter: 0,
        }
    }

    /// Decide what to do with `key` whose current content hash is
    /// `content_hash`.
    pub fn check(&mut self, key: RegionKey, content_hash: u64) -> StaticDecision {
        self.frame_counter += 1;
        let fc = self.frame_counter;

        // Has the content changed since the last time we saw this key?
        let prev_hash = self.region_hashes.get(&key).copied();
        if prev_hash != Some(content_hash) {
            // Content changed (or first time seeing this key): reset everything.
            self.region_hashes.insert(key, content_hash);
            self.static_frame_count.insert(key, 1);
            self.cached_regions.remove(&key);
            self.last_seen.insert(key, fc);
            return StaticDecision::EncodeNormally;
        }

        // Hash is the same as before.

        // If it's already in the remote cache, nothing to send.
        if self.cached_regions.contains_key(&key) {
            self.last_seen.insert(key, fc);
            return StaticDecision::Unchanged;
        }

        // Increment the run-length counter.
        let count = self.static_frame_count.entry(key).or_insert(0);
        *count += 1;
        self.last_seen.insert(key, fc);

        if *count >= self.cache_threshold {
            // Evict LRU if we're at capacity.
            if self.cached_regions.len() >= self.max_regions {
                self.evict_lru();
            }
            self.cached_regions.insert(key, content_hash);
            return StaticDecision::SendAndCache;
        }

        StaticDecision::EncodeNormally
    }

    /// Remove the cached region with the smallest `last_seen` timestamp.
    fn evict_lru(&mut self) {
        if let Some((&oldest_key, _)) = self
            .last_seen
            .iter()
            .filter(|(k, _)| self.cached_regions.contains_key(k))
            .min_by_key(|&(_, &ts)| ts)
        {
            self.cached_regions.remove(&oldest_key);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_region_is_not_cached() {
        let mut tracker = StaticAtlasTracker::new(3);
        let key = RegionKey(0, 0);
        let decision = tracker.check(key, 0xABCD);
        assert_eq!(decision, StaticDecision::EncodeNormally);
    }

    #[test]
    fn region_becomes_cached_after_threshold() {
        let mut tracker = StaticAtlasTracker::new(3);
        let key = RegionKey(1, 0);
        let hash = 0x1234_5678;

        // First call: hash is new → EncodeNormally (count = 1)
        assert_eq!(tracker.check(key, hash), StaticDecision::EncodeNormally);
        // Second: count = 2, still below threshold
        assert_eq!(tracker.check(key, hash), StaticDecision::EncodeNormally);
        // Third: count = 3, hits threshold → SendAndCache
        assert_eq!(tracker.check(key, hash), StaticDecision::SendAndCache);
        // Fourth: already cached → Unchanged
        assert_eq!(tracker.check(key, hash), StaticDecision::Unchanged);
    }

    #[test]
    fn hash_change_invalidates_cache() {
        let mut tracker = StaticAtlasTracker::new(2);
        let key = RegionKey(0, 1);
        let hash_a = 0xAAAA;
        let hash_b = 0xBBBB;

        // Get it cached (threshold=2).
        tracker.check(key, hash_a); // count=1, EncodeNormally
        tracker.check(key, hash_a); // count=2, SendAndCache

        // Change content.
        let decision = tracker.check(key, hash_b);
        assert_eq!(
            decision,
            StaticDecision::EncodeNormally,
            "changed hash must invalidate cache"
        );
    }

    #[test]
    fn different_regions_tracked_independently() {
        let mut tracker = StaticAtlasTracker::new(2);
        let key_a = RegionKey(0, 0);
        let key_b = RegionKey(0, 1);

        // Cache key_a (2 identical frames).
        tracker.check(key_a, 0x11);
        tracker.check(key_a, 0x11); // SendAndCache

        // key_b has never been seen; first check → EncodeNormally.
        assert_eq!(tracker.check(key_b, 0x22), StaticDecision::EncodeNormally);
        // key_a should still be Unchanged.
        assert_eq!(tracker.check(key_a, 0x11), StaticDecision::Unchanged);
    }

    #[test]
    fn lru_eviction() {
        // max_regions=2, threshold=2 so each region needs 2 stable-hash calls
        // before caching: call1=new hash→EncodeNormally, call2=same→SendAndCache.
        let mut tracker = StaticAtlasTracker::with_max_regions(2, 2);
        let key_a = RegionKey(0, 0);
        let key_b = RegionKey(0, 1);
        let key_c = RegionKey(0, 2);

        // Stabilise and cache A.
        assert_eq!(tracker.check(key_a, 0xAA), StaticDecision::EncodeNormally); // count=1
        assert_eq!(tracker.check(key_a, 0xAA), StaticDecision::SendAndCache); // count=2>=2

        // Stabilise and cache B.
        assert_eq!(tracker.check(key_b, 0xBB), StaticDecision::EncodeNormally);
        assert_eq!(tracker.check(key_b, 0xBB), StaticDecision::SendAndCache);

        // Refresh A's last_seen so it's newer than B.
        assert_eq!(tracker.check(key_a, 0xAA), StaticDecision::Unchanged);

        // Stabilise C; adding it must evict B (B's last_seen is oldest).
        assert_eq!(tracker.check(key_c, 0xCC), StaticDecision::EncodeNormally);
        assert_eq!(tracker.check(key_c, 0xCC), StaticDecision::SendAndCache);

        // A is still cached (was refreshed after B).
        assert_eq!(tracker.check(key_a, 0xAA), StaticDecision::Unchanged);
        // C was just cached.
        assert_eq!(tracker.check(key_c, 0xCC), StaticDecision::Unchanged);
        // B was evicted; its count is still >= threshold so the next
        // same-hash call immediately yields SendAndCache (no re-stabilisation
        // needed since eviction only removes from cached_regions).
        assert_eq!(
            tracker.check(key_b, 0xBB),
            StaticDecision::SendAndCache,
            "evicted region with stable count should be SendAndCache again"
        );
    }
}
