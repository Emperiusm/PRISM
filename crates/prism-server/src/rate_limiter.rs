// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Connection rate limiter: token bucket per source IP.
//
// Prevents connection floods by limiting how many connections a single IP
// can make within a rolling time window. Each IP gets its own token bucket;
// buckets are garbage-collected after 5 minutes of inactivity.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// Token bucket for a single source IP.
struct TokenBucket {
    tokens: f32,
    last_refill: Instant,
    max_tokens: f32,
    refill_rate: f32, // tokens per second
}

/// Per-IP connection rate limiter backed by token buckets.
///
/// Construct with [`ConnectionRateLimiter::new`] (or use the [`Default`]
/// impl for 10 connections per minute), then call [`check`] for every
/// incoming connection attempt.
pub struct ConnectionRateLimiter {
    buckets: HashMap<IpAddr, TokenBucket>,
    max_per_minute: u32,
}

impl ConnectionRateLimiter {
    /// Create a limiter allowing `max_per_minute` connections per IP per minute.
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            buckets: HashMap::new(),
            max_per_minute,
        }
    }

    /// Check whether a connection from `ip` should be allowed.
    ///
    /// Returns `true` (allow) when the bucket has at least one token.
    /// Returns `false` (reject) when the bucket is empty.
    pub fn check(&mut self, ip: IpAddr) -> bool {
        let max_tokens = self.max_per_minute as f32;
        let refill_rate = max_tokens / 60.0;

        let bucket = self.buckets.entry(ip).or_insert_with(|| TokenBucket {
            tokens: max_tokens,
            last_refill: Instant::now(),
            max_tokens,
            refill_rate,
        });

        // Refill tokens proportional to elapsed time since last check.
        let elapsed = bucket.last_refill.elapsed().as_secs_f32();
        bucket.tokens = (bucket.tokens + elapsed * bucket.refill_rate).min(bucket.max_tokens);
        bucket.last_refill = Instant::now();

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Remove entries that have been inactive for more than 5 minutes.
    ///
    /// Call periodically (e.g. once per minute) to prevent unbounded memory growth.
    pub fn gc(&mut self) {
        self.buckets
            .retain(|_, bucket| bucket.last_refill.elapsed() < Duration::from_secs(300));
    }
}

impl Default for ConnectionRateLimiter {
    /// Default: 10 connections per minute per IP.
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn ip(a: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, a))
    }

    #[test]
    fn first_connection_allowed() {
        let mut rl = ConnectionRateLimiter::new(10);
        assert!(rl.check(ip(1)), "first connection must be allowed");
    }

    #[test]
    fn burst_within_limit_allowed() {
        let mut rl = ConnectionRateLimiter::new(10);
        let addr = ip(2);
        // All 10 connections in the initial burst should be allowed.
        for i in 0..10 {
            assert!(rl.check(addr), "connection {} should be allowed", i + 1);
        }
    }

    #[test]
    fn burst_exceeding_limit_rejected() {
        let mut rl = ConnectionRateLimiter::new(10);
        let addr = ip(3);
        // Exhaust the bucket.
        for _ in 0..10 {
            rl.check(addr);
        }
        // The 11th connection must be rejected.
        assert!(!rl.check(addr), "11th connection must be rejected");
    }

    #[test]
    fn refills_over_time() {
        let mut rl = ConnectionRateLimiter::new(60); // 1 token/second
        let addr = ip(4);
        // Exhaust the bucket.
        for _ in 0..60 {
            rl.check(addr);
        }
        assert!(!rl.check(addr), "bucket should be empty");

        // Wait for at least 1 token to refill (~1 second).
        std::thread::sleep(Duration::from_millis(1100));
        assert!(rl.check(addr), "connection should be allowed after refill");
    }

    #[test]
    fn different_ips_independent() {
        let mut rl = ConnectionRateLimiter::new(2);
        let addr_a = ip(10);
        let addr_b = ip(11);

        // Exhaust addr_a.
        rl.check(addr_a);
        rl.check(addr_a);
        assert!(!rl.check(addr_a), "addr_a should be exhausted");

        // addr_b should still have its own full bucket.
        assert!(rl.check(addr_b), "addr_b should be unaffected");
        assert!(
            rl.check(addr_b),
            "addr_b second connection should be allowed"
        );
        assert!(
            !rl.check(addr_b),
            "addr_b should also exhaust independently"
        );
    }
}
