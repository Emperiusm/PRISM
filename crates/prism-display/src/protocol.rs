use std::time::{Duration, Instant};

// ── Display channel message-type constants ────────────────────────────────────

/// Carries the region-map for the current frame.
pub const MSG_REGION_MAP: u8 = 0x01;
/// Carries an encoded video/lossless slice.
pub const MSG_SLICE: u8 = 0x02;
/// Carries a new cursor shape (RGBA pixels).
pub const MSG_CURSOR_SHAPE: u8 = 0x03;
/// Carries the current cursor position.
pub const MSG_CURSOR_POSITION: u8 = 0x04;
/// Requests an IDR (instantaneous decoder refresh) from the encoder.
pub const MSG_IDR_REQUEST: u8 = 0x05;
/// Carries a quality hint from the transport layer.
pub const MSG_QUALITY_HINT: u8 = 0x06;

// ── FrameGapDetector ──────────────────────────────────────────────────────────

/// Detects missing frames in a sequence-numbered stream and throttles IDR requests.
///
/// Sequence numbers are expected to increment by 1 for consecutive frames.  A
/// gap is declared when `seq > last + 1`.
#[derive(Debug)]
pub struct FrameGapDetector {
    last_received_seq: Option<u32>,
    has_gap: bool,
    cooldown: Duration,
    last_idr_request: Option<Instant>,
}

impl FrameGapDetector {
    /// Create a detector with the given IDR-request cooldown period.
    pub fn with_cooldown(cooldown: Duration) -> Self {
        Self {
            last_received_seq: None,
            has_gap: false,
            cooldown,
            last_idr_request: None,
        }
    }

    /// Notify the detector that frame `seq` has been received.
    ///
    /// Sets the gap flag when `seq > last_seq + 1`.  The very first frame never
    /// triggers a gap regardless of its sequence number.
    pub fn receive_seq(&mut self, seq: u32) {
        if let Some(last) = self.last_received_seq {
            if seq > last.wrapping_add(1) {
                self.has_gap = true;
            }
        }
        self.last_received_seq = Some(seq);
    }

    /// Returns `true` if a gap has been detected *and* the cooldown has elapsed
    /// since the last IDR request (or no request has ever been sent).
    ///
    /// When `true` is returned the gap flag is cleared and the request timestamp
    /// is updated.
    pub fn should_request_idr(&mut self) -> bool {
        if !self.has_gap {
            return false;
        }

        let cooldown_elapsed = match self.last_idr_request {
            None => true,
            Some(last) => last.elapsed() >= self.cooldown,
        };

        if cooldown_elapsed {
            self.has_gap = false;
            self.last_idr_request = Some(Instant::now());
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Message type constants ────────────────────────────────────────────────

    #[test]
    fn all_msg_types_distinct() {
        let types = [
            MSG_REGION_MAP,
            MSG_SLICE,
            MSG_CURSOR_SHAPE,
            MSG_CURSOR_POSITION,
            MSG_IDR_REQUEST,
            MSG_QUALITY_HINT,
        ];
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j], "message type {} and {} are equal", i, j);
            }
        }
    }

    // ── FrameGapDetector ─────────────────────────────────────────────────────

    #[test]
    fn no_gap_for_consecutive_frames() {
        let mut det = FrameGapDetector::with_cooldown(Duration::from_millis(100));
        det.receive_seq(10);
        det.receive_seq(11);
        det.receive_seq(12);
        assert!(!det.has_gap);
        assert!(!det.should_request_idr());
    }

    #[test]
    fn detects_gap() {
        let mut det = FrameGapDetector::with_cooldown(Duration::from_millis(100));
        det.receive_seq(5);
        det.receive_seq(8); // gap: 6 and 7 are missing
        assert!(det.has_gap);
        assert!(det.should_request_idr());
    }

    #[test]
    fn cooldown_prevents_immediate_repeat() {
        let mut det = FrameGapDetector::with_cooldown(Duration::from_millis(200));
        det.receive_seq(1);
        det.receive_seq(5); // gap
        // First IDR request fires.
        assert!(det.should_request_idr());
        // Inject another gap immediately.
        det.receive_seq(9);
        // Cooldown not elapsed — should not fire again.
        assert!(!det.should_request_idr());
        // After cooldown, it may fire again.
        std::thread::sleep(Duration::from_millis(210));
        det.receive_seq(15); // another gap
        assert!(det.should_request_idr());
    }

    #[test]
    fn first_frame_no_gap() {
        let mut det = FrameGapDetector::with_cooldown(Duration::from_millis(100));
        // A very high initial seq should not trigger a gap.
        det.receive_seq(1_000_000);
        assert!(!det.has_gap);
        assert!(!det.should_request_idr());
    }
}
