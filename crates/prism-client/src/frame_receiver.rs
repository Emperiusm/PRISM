// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Frame reception utilities: statistics tracking and datagram parsing.
//!
//! [`FrameStats`] accumulates per-frame metrics (frame count, bytes, timing,
//! sequence gaps) and exposes derived statistics such as average FPS and average
//! frame size.
//!
//! [`parse_display_datagram`] decodes the [`PrismHeader`] from a raw datagram
//! [`Bytes`] and returns `(sequence, channel_id)` when the channel is
//! `CHANNEL_DISPLAY`, or `None` for any other channel or on parse failure.

use std::time::Instant;

use bytes::Bytes;

use prism_protocol::channel::CHANNEL_DISPLAY;
use prism_protocol::header::{HEADER_SIZE, PrismHeader};

// ── FrameStats ────────────────────────────────────────────────────────────────

/// Per-channel frame reception statistics.
#[derive(Debug)]
pub struct FrameStats {
    /// Total number of frames recorded.
    pub frames_received: u64,
    /// Total bytes across all recorded frames (payload bytes only).
    pub bytes_received: u64,
    /// Instant of the first `record` call, or `None` if no frames yet.
    pub first_frame_time: Option<Instant>,
    /// Instant of the most recent `record` call, or `None` if no frames yet.
    pub last_frame_time: Option<Instant>,
    /// Sequence number of the most recently seen frame, or `None` if no frames yet.
    pub last_seq: Option<u32>,
    /// Number of detected sequence gaps (missing frames between arrivals).
    pub gaps: u64,
}

impl FrameStats {
    /// Create a new zeroed statistics accumulator.
    pub fn new() -> Self {
        Self {
            frames_received: 0,
            bytes_received: 0,
            first_frame_time: None,
            last_frame_time: None,
            last_seq: None,
            gaps: 0,
        }
    }

    /// Record a received frame with the given sequence number and payload byte count.
    ///
    /// A gap is detected when `seq > last_seq + 1` (i.e., at least one frame
    /// was skipped). Sequence wrap-around is not handled; this is appropriate
    /// for the short-lived counters expected in a PRISM session.
    pub fn record(&mut self, seq: u32, bytes: usize) {
        let now = Instant::now();

        if self.first_frame_time.is_none() {
            self.first_frame_time = Some(now);
        }
        self.last_frame_time = Some(now);

        // Gap detection
        if let Some(last) = self.last_seq
            && seq > last.wrapping_add(1)
        {
            // Number of missing frames is (seq - last - 1)
            self.gaps += (seq - last - 1) as u64;
        }
        self.last_seq = Some(seq);

        self.frames_received += 1;
        self.bytes_received += bytes as u64;
    }

    /// Average frames per second since the first frame, or `0.0` if fewer than
    /// two frames have been received (no elapsed time to measure).
    pub fn avg_fps(&self) -> f64 {
        match (self.first_frame_time, self.last_frame_time) {
            (Some(first), Some(last)) if last > first => {
                let elapsed = last.duration_since(first).as_secs_f64();
                if elapsed > 0.0 {
                    self.frames_received as f64 / elapsed
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    /// Average frame size in bytes, or `0.0` if no frames have been received.
    pub fn avg_frame_size(&self) -> f64 {
        if self.frames_received == 0 {
            0.0
        } else {
            self.bytes_received as f64 / self.frames_received as f64
        }
    }
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new()
    }
}

// ── parse_display_datagram ────────────────────────────────────────────────────

/// Parse the [`PrismHeader`] from a raw datagram.
///
/// Returns `Some((sequence, channel_id))` when the header is valid and the
/// channel is [`CHANNEL_DISPLAY`].  Returns `None` on any parse error or when
/// the channel does not match.
pub fn parse_display_datagram(data: &Bytes) -> Option<(u32, u16)> {
    if data.len() < HEADER_SIZE {
        return None;
    }
    let header = PrismHeader::decode_from_slice(&data[..HEADER_SIZE]).ok()?;
    if header.channel_id == CHANNEL_DISPLAY {
        Some((header.sequence, header.channel_id))
    } else {
        None
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use prism_protocol::channel::{CHANNEL_DISPLAY, CHANNEL_INPUT};
    use prism_protocol::header::{HEADER_SIZE, PROTOCOL_VERSION, PrismHeader};

    // ── FrameStats tests ─────────────────────────────────────────────────────

    #[test]
    fn empty_stats() {
        let stats = FrameStats::new();
        assert_eq!(stats.frames_received, 0);
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.gaps, 0);
        assert!(stats.first_frame_time.is_none());
        assert!(stats.last_frame_time.is_none());
        assert!(stats.last_seq.is_none());
        assert_eq!(stats.avg_fps(), 0.0);
        assert_eq!(stats.avg_frame_size(), 0.0);
    }

    #[test]
    fn record_increments() {
        let mut stats = FrameStats::new();
        stats.record(0, 1000);
        stats.record(1, 2000);

        assert_eq!(stats.frames_received, 2);
        assert_eq!(stats.bytes_received, 3000);
        assert_eq!(stats.avg_frame_size(), 1500.0);
    }

    #[test]
    fn gap_detection() {
        let mut stats = FrameStats::new();
        stats.record(0, 100);
        // Skip sequences 1, 2, 3 — three missing frames
        stats.record(4, 100);

        assert_eq!(stats.gaps, 3);
        assert_eq!(stats.frames_received, 2);
    }

    #[test]
    fn no_gaps_sequential() {
        let mut stats = FrameStats::new();
        for seq in 0..10u32 {
            stats.record(seq, 512);
        }
        assert_eq!(stats.gaps, 0);
        assert_eq!(stats.frames_received, 10);
    }

    // ── parse_display_datagram tests ─────────────────────────────────────────

    fn make_header_bytes(channel_id: u16, sequence: u32) -> Bytes {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id,
            msg_type: 0x02,
            flags: 0,
            sequence,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        buf.freeze()
    }

    #[test]
    fn parse_valid_display() {
        let data = make_header_bytes(CHANNEL_DISPLAY, 42);
        let result = parse_display_datagram(&data);
        assert_eq!(result, Some((42, CHANNEL_DISPLAY)));
    }

    #[test]
    fn parse_non_display_returns_none() {
        let data = make_header_bytes(CHANNEL_INPUT, 7);
        let result = parse_display_datagram(&data);
        assert!(result.is_none(), "non-display channel must return None");
    }
}
