//! NVENC encode session lifecycle management.

use prism_display::EncodedSlice;

// ── NvencSessionState ─────────────────────────────────────────────────────────

/// Lifecycle state of an NVENC encode session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvencSessionState {
    /// Session has not been created yet.
    Uninitialized,
    /// Session is created and configured; ready to receive frames.
    Ready,
    /// Actively encoding frames.
    Encoding,
    /// Draining remaining frames from the encoder pipeline.
    Flushing,
    /// An unrecoverable error occurred; session must be destroyed.
    Error,
}

// ── EncodeResult ──────────────────────────────────────────────────────────────

/// Result returned by an encode call on an NVENC session.
#[derive(Debug)]
pub enum EncodeResult {
    /// One or more encoded slices are available.
    Encoded {
        /// Ordered list of bitstream slices for this frame.
        slices: Vec<EncodedSlice>,
        /// True when this is an IDR / keyframe.
        is_keyframe: bool,
    },
    /// The encoder buffered the input but has no output to offer yet (B-frame
    /// lookahead or pipeline fill).
    NeedsMoreInput,
    /// The encoder has been fully drained; no more output will follow.
    Flushed,
}

// ── NvencStats ────────────────────────────────────────────────────────────────

/// Running statistics for an NVENC encode session.
///
/// All counters are monotonically increasing across the session lifetime.
#[derive(Debug, Clone, Default)]
pub struct NvencStats {
    /// Total number of frames encoded (including keyframes).
    pub frames_encoded: u64,
    /// Number of IDR / keyframes encoded.
    pub keyframes_encoded: u64,
    /// Cumulative encoded byte count across all frames.
    pub total_bytes_out: u64,
    /// Exponential running average encode latency in microseconds.
    pub avg_encode_time_us: u64,
    /// Encode latency of the most recently completed frame (microseconds).
    pub last_encode_time_us: u64,
}

impl NvencStats {
    /// Record one completed encoded frame.
    ///
    /// Updates all counters and computes a running arithmetic average for
    /// `avg_encode_time_us` over all frames seen so far.
    pub fn record_frame(&mut self, bytes: usize, is_keyframe: bool, encode_time_us: u64) {
        self.frames_encoded += 1;
        if is_keyframe {
            self.keyframes_encoded += 1;
        }
        self.total_bytes_out += bytes as u64;
        self.last_encode_time_us = encode_time_us;

        // Running arithmetic mean: avg_n = avg_{n-1} + (x_n - avg_{n-1}) / n
        let n = self.frames_encoded;
        let delta = encode_time_us as i64 - self.avg_encode_time_us as i64;
        self.avg_encode_time_us =
            (self.avg_encode_time_us as i64 + delta / n as i64) as u64;
    }

    /// Rough average bitrate estimate in bits per second.
    ///
    /// Computed as `total_bytes * 8 / frames_encoded`.  Returns 0 when no
    /// frames have been encoded yet.
    pub fn avg_bitrate_bps(&self) -> u64 {
        if self.frames_encoded == 0 {
            return 0;
        }
        self.total_bytes_out * 8 / self.frames_encoded
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_states_neq() {
        assert_ne!(NvencSessionState::Uninitialized, NvencSessionState::Ready);
        assert_ne!(NvencSessionState::Encoding, NvencSessionState::Flushing);
        assert_ne!(NvencSessionState::Error, NvencSessionState::Ready);
    }

    #[test]
    fn stats_default_zero() {
        let s = NvencStats::default();
        assert_eq!(s.frames_encoded, 0);
        assert_eq!(s.keyframes_encoded, 0);
        assert_eq!(s.total_bytes_out, 0);
        assert_eq!(s.avg_encode_time_us, 0);
        assert_eq!(s.last_encode_time_us, 0);
    }

    #[test]
    fn stats_record_frame() {
        let mut s = NvencStats::default();
        s.record_frame(1000, true,  100);
        s.record_frame(2000, false, 200);
        s.record_frame(3000, false, 300);

        assert_eq!(s.frames_encoded,   3);
        assert_eq!(s.keyframes_encoded, 1);
        assert_eq!(s.total_bytes_out,  6000);
        assert_eq!(s.last_encode_time_us, 300);
    }

    #[test]
    fn stats_avg_encode_time() {
        let mut s = NvencStats::default();
        s.record_frame(500, false, 100);
        s.record_frame(500, false, 300);
        // Average of 100 and 300 is 200.
        assert_eq!(s.avg_encode_time_us, 200);
    }
}
