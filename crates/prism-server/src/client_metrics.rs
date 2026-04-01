// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use uuid::Uuid;

/// Per-client metrics counters.
pub struct ClientMetrics {
    pub client_id: Uuid,
    pub frames_sent: AtomicU32,
    pub frames_dropped: AtomicU32,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub encode_time_total_us: AtomicU64,
    pub input_events: AtomicU32,
    pub probes_sent: AtomicU32,
}

impl ClientMetrics {
    pub fn new(client_id: Uuid) -> Self {
        Self {
            client_id,
            frames_sent: AtomicU32::new(0),
            frames_dropped: AtomicU32::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            encode_time_total_us: AtomicU64::new(0),
            input_events: AtomicU32::new(0),
            probes_sent: AtomicU32::new(0),
        }
    }

    pub fn avg_encode_time_us(&self) -> u64 {
        let frames = self.frames_sent.load(Ordering::Relaxed) as u64;
        if frames > 0 {
            self.encode_time_total_us.load(Ordering::Relaxed) / frames
        } else {
            0
        }
    }

    pub fn avg_frame_size(&self) -> u64 {
        let frames = self.frames_sent.load(Ordering::Relaxed) as u64;
        if frames > 0 {
            self.bytes_sent.load(Ordering::Relaxed) / frames
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_id() -> Uuid {
        Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext))
    }

    #[test]
    fn new_is_zeroed() {
        let m = ClientMetrics::new(test_id());
        assert_eq!(m.frames_sent.load(Ordering::Relaxed), 0);
        assert_eq!(m.frames_dropped.load(Ordering::Relaxed), 0);
        assert_eq!(m.bytes_sent.load(Ordering::Relaxed), 0);
        assert_eq!(m.bytes_received.load(Ordering::Relaxed), 0);
        assert_eq!(m.encode_time_total_us.load(Ordering::Relaxed), 0);
        assert_eq!(m.input_events.load(Ordering::Relaxed), 0);
        assert_eq!(m.probes_sent.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn avg_encode_time() {
        let m = ClientMetrics::new(test_id());
        // No frames yet → 0.
        assert_eq!(m.avg_encode_time_us(), 0);

        m.frames_sent.store(4, Ordering::Relaxed);
        m.encode_time_total_us.store(8_000, Ordering::Relaxed);
        assert_eq!(m.avg_encode_time_us(), 2_000);
    }

    #[test]
    fn avg_frame_size() {
        let m = ClientMetrics::new(test_id());
        // No frames yet → 0.
        assert_eq!(m.avg_frame_size(), 0);

        m.frames_sent.store(10, Ordering::Relaxed);
        m.bytes_sent.store(50_000, Ordering::Relaxed);
        assert_eq!(m.avg_frame_size(), 5_000);
    }

    #[test]
    fn concurrent_safe() {
        let m = Arc::new(ClientMetrics::new(test_id()));
        let mut handles = Vec::new();

        for _ in 0..8 {
            let mc = Arc::clone(&m);
            handles.push(std::thread::spawn(move || {
                for _ in 0..1_000 {
                    mc.frames_sent.fetch_add(1, Ordering::Relaxed);
                    mc.bytes_sent.fetch_add(1_024, Ordering::Relaxed);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(m.frames_sent.load(Ordering::Relaxed), 8_000);
        assert_eq!(m.bytes_sent.load(Ordering::Relaxed), 8_000 * 1_024);
    }
}
