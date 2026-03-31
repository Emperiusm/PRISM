use std::collections::VecDeque;
use std::time::Instant;

// ── AudioFrame ───────────────────────────────────────────────────────────────

/// A single decoded PCM audio frame ready for playback.
#[derive(Debug)]
pub struct AudioFrame {
    /// Presentation timestamp in microseconds (from the sender's clock).
    pub timestamp_us: u64,
    /// Interleaved PCM samples.
    pub pcm_samples: Vec<f32>,
    /// Wall-clock time at which this frame arrived.
    pub received_at: Instant,
}

// ── AdaptiveJitterBuffer ─────────────────────────────────────────────────────

/// Buffers incoming audio frames and adapts its hold-depth to network jitter.
///
/// Target depth is clamped to [1, 4] frames (20 – 80 ms at 20 ms/frame).
/// Jitter is tracked with an EMA (α = 0.1) and the target depth is
/// widened when jitter is high and narrowed when jitter is low.
pub struct AdaptiveJitterBuffer {
    frames: VecDeque<AudioFrame>,
    frame_duration_ms: u64,
    /// EMA of inter-arrival jitter in milliseconds.
    jitter_ms: f32,
    /// Last frame arrival time (for computing inter-arrival delta).
    last_arrival: Option<Instant>,
    /// Current target buffer depth in frames.
    target_depth: usize,
}

const MIN_DEPTH: usize = 1;
const MAX_DEPTH: usize = 4;
/// EMA smoothing factors.
const EMA_SLOW: f32 = 0.9; // weight on old value
const EMA_FAST: f32 = 0.1; // weight on new sample

impl AdaptiveJitterBuffer {
    /// Create a new buffer.
    ///
    /// `frame_duration_ms` is the nominal duration of one frame (e.g. 20 ms).
    pub fn new(frame_duration_ms: u64) -> Self {
        Self {
            frames: VecDeque::new(),
            frame_duration_ms,
            jitter_ms: 0.0,
            last_arrival: None,
            target_depth: MIN_DEPTH,
        }
    }

    /// Push a new frame and update the jitter / target-depth model.
    pub fn push(&mut self, frame: AudioFrame) {
        let now = frame.received_at;

        // Update inter-arrival jitter EMA.
        if let Some(prev) = self.last_arrival {
            let actual_gap_ms = now.duration_since(prev).as_millis() as f32;
            let ideal_gap_ms = self.frame_duration_ms as f32;
            let deviation = (actual_gap_ms - ideal_gap_ms).abs();
            self.jitter_ms = EMA_SLOW * self.jitter_ms + EMA_FAST * deviation;
        }
        self.last_arrival = Some(now);

        // Adapt target depth based on current jitter relative to frame duration.
        let frame_dur = self.frame_duration_ms as f32;
        if self.jitter_ms > frame_dur * 0.5 {
            // High jitter → grow buffer (up to MAX).
            self.target_depth = (self.target_depth + 1).min(MAX_DEPTH);
        } else if self.jitter_ms < frame_dur * 0.1 && self.target_depth > MIN_DEPTH {
            // Low jitter → shrink buffer (down to MIN).
            self.target_depth -= 1;
        }

        self.frames.push_back(frame);
    }

    /// Pop the oldest frame if the buffer has filled to at least `target_depth`.
    pub fn pop(&mut self) -> Option<AudioFrame> {
        if self.frames.len() >= self.target_depth {
            self.frames.pop_front()
        } else {
            None
        }
    }

    /// Current number of frames held in the buffer.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Current target depth (in frames).
    pub fn target_depth(&self) -> usize {
        self.target_depth
    }

    /// Current EMA jitter estimate in milliseconds.
    pub fn jitter_ms(&self) -> f32 {
        self.jitter_ms
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(timestamp_us: u64) -> AudioFrame {
        AudioFrame {
            timestamp_us,
            pcm_samples: vec![0.0; 960],
            received_at: Instant::now(),
        }
    }

    #[test]
    fn empty_pops_none() {
        let mut buf = AdaptiveJitterBuffer::new(20);
        assert!(buf.pop().is_none());
    }

    #[test]
    fn single_frame_pops_at_depth_1() {
        let mut buf = AdaptiveJitterBuffer::new(20);
        // target_depth starts at 1, so one frame is enough to pop.
        buf.push(make_frame(1000));
        assert_eq!(buf.target_depth(), 1);
        let frame = buf.pop();
        assert!(frame.is_some());
        assert_eq!(frame.unwrap().timestamp_us, 1000);
    }

    #[test]
    fn fifo_order() {
        let mut buf = AdaptiveJitterBuffer::new(20);
        // Push three frames; target stays at 1 (no jitter in tests using Instant::now).
        for ts in [100u64, 200, 300] {
            buf.push(make_frame(ts));
        }
        // Pop all three and verify ordering.
        let ts0 = buf.pop().expect("first").timestamp_us;
        let ts1 = buf.pop().expect("second").timestamp_us;
        let ts2 = buf.pop().expect("third").timestamp_us;
        assert!(ts0 < ts1 && ts1 < ts2);
    }

    #[test]
    fn target_starts_at_1() {
        let buf = AdaptiveJitterBuffer::new(20);
        assert_eq!(buf.target_depth(), MIN_DEPTH);
    }

    #[test]
    fn target_capped_at_max() {
        let mut buf = AdaptiveJitterBuffer::new(20);
        // Force target_depth directly beyond MAX to verify cap.
        // We do this by artificially setting target_depth then calling push
        // again to trigger the grow path.
        buf.target_depth = MAX_DEPTH;
        // Simulate high-jitter push: set jitter above threshold.
        buf.jitter_ms = 100.0; // well above 0.5 * 20ms = 10ms
        buf.push(make_frame(0));
        assert_eq!(buf.target_depth(), MAX_DEPTH);
    }

    #[test]
    fn len_tracks_buffer() {
        let mut buf = AdaptiveJitterBuffer::new(20);
        assert_eq!(buf.len(), 0);
        buf.push(make_frame(0));
        assert_eq!(buf.len(), 1);
        buf.push(make_frame(1));
        assert_eq!(buf.len(), 2);
        buf.pop();
        assert_eq!(buf.len(), 1);
    }
}
