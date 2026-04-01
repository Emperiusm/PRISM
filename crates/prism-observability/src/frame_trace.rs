// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

/// Per-frame timing trace collected at each pipeline stage (all timestamps in microseconds).
///
/// Server-side fields are always present. Client-side fields (`network_recv_us`,
/// `decode_end_us`, `render_end_us`) are `None` when the server produces a
/// server-only breakdown (e.g. for headless benchmarks or when the client has
/// not yet reported back).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameTrace {
    /// Monotonic sequence number identifying this frame.
    pub frame_seq: u64,
    /// Timestamp when pixel capture began (µs).
    pub capture_start_us: u64,
    /// Timestamp when capture finished (µs).
    pub capture_end_us: u64,
    /// Timestamp when classification / scene-analysis finished (µs).
    pub classify_end_us: u64,
    /// Timestamp when the encoder started (µs).
    pub encode_start_us: u64,
    /// Timestamp when encoding finished (µs).
    pub encode_end_us: u64,
    /// Timestamp when the encoded packet was handed to the network stack (µs).
    pub send_us: u64,

    // --- client-side fields (filled in after client feedback arrives) ---
    /// Timestamp when the first byte of the frame was received by the client (µs).
    pub network_recv_us: Option<u64>,
    /// Timestamp when decoding finished on the client (µs).
    pub decode_end_us: Option<u64>,
    /// Timestamp when the frame was presented on the display (µs).
    pub render_end_us: Option<u64>,
}

/// Per-stage latency deltas derived from a [`FrameTrace`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLatencyBreakdown {
    /// Time spent capturing pixels (µs).
    pub capture_us: u64,
    /// Time spent classifying / analysing the captured frame (µs).
    pub classify_us: u64,
    /// Time spent encoding (µs).
    pub encode_us: u64,
    /// Time from encode-done to hand-off to the network stack (µs).
    pub send_us: u64,
    /// One-way network transit time (µs). `None` if client fields absent.
    pub network_us: Option<u64>,
    /// Client-side decode time (µs). `None` if client fields absent.
    pub decode_us: Option<u64>,
    /// Client-side render / display time (µs). `None` if client fields absent.
    pub render_us: Option<u64>,
    /// End-to-end pipeline latency (µs).
    ///
    /// Uses `render_end_us` when available, otherwise falls back to `send_us`.
    pub total_us: u64,
}

impl FrameTrace {
    /// Compute per-stage latency deltas from this trace.
    pub fn breakdown(&self) -> FrameLatencyBreakdown {
        let capture_us = self.capture_end_us.saturating_sub(self.capture_start_us);
        let classify_us = self.classify_end_us.saturating_sub(self.capture_end_us);
        let encode_us = self.encode_end_us.saturating_sub(self.encode_start_us);
        let send_us = self.send_us.saturating_sub(self.encode_end_us);

        let network_us = self
            .network_recv_us
            .map(|recv| recv.saturating_sub(self.send_us));

        let decode_us = match (self.network_recv_us, self.decode_end_us) {
            (Some(recv), Some(decode)) => Some(decode.saturating_sub(recv)),
            _ => None,
        };

        let render_us = match (self.decode_end_us, self.render_end_us) {
            (Some(decode), Some(render)) => Some(render.saturating_sub(decode)),
            _ => None,
        };

        let pipeline_end = self.render_end_us.unwrap_or(self.send_us);
        let total_us = pipeline_end.saturating_sub(self.capture_start_us);

        FrameLatencyBreakdown {
            capture_us,
            classify_us,
            encode_us,
            send_us,
            network_us,
            decode_us,
            render_us,
            total_us,
        }
    }
}

/// Adaptive frame tracer: decides which frames are worth capturing full
/// [`FrameTrace`] data for, balancing overhead against observability.
///
/// Two capture strategies are combined:
/// - **Uniform sampling** – every `uniform_rate`-th frame is always traced.
/// - **Slow-frame capture** – any frame whose end-to-end latency exceeds
///   `slow_frame_threshold_us` is traced regardless of the uniform counter.
///
/// A per-second budget (`max_traces_per_second`) caps total captures so that
/// a pathological stream of slow frames doesn't flood the trace store.
#[derive(Debug, Clone)]
pub struct FrameTracer {
    /// Trace 1-in-N frames uniformly. Default: 60 (one per second at 60 fps).
    pub uniform_rate: u64,
    /// Internal counter for uniform sampling; wraps at `uniform_rate`.
    uniform_counter: u64,
    /// Frames slower than this are always traced (subject to budget). Default: 20 ms.
    pub slow_frame_threshold_us: u64,
    /// How many traces have been emitted in the current one-second window.
    traces_this_second: u32,
    /// Hard cap on traces per second. Default: 10.
    pub max_traces_per_second: u32,
}

impl Default for FrameTracer {
    fn default() -> Self {
        Self {
            uniform_rate: 60,
            uniform_counter: 0,
            slow_frame_threshold_us: 20_000,
            traces_this_second: 0,
            max_traces_per_second: 10,
        }
    }
}

impl FrameTracer {
    /// Create a new `FrameTracer` with default parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Decide whether the next frame should be fully traced.
    ///
    /// `last_frame_total_us` is the end-to-end latency of the frame that just
    /// completed (from [`FrameLatencyBreakdown::total_us`] or equivalent).
    ///
    /// Must be called once per frame in presentation order.
    pub fn should_trace(&mut self, last_frame_total_us: u64) -> bool {
        // Always increment the uniform counter so the cadence is independent
        // of whether we are over budget.
        self.uniform_counter += 1;
        let uniform_tick = self.uniform_counter >= self.uniform_rate;
        if uniform_tick {
            self.uniform_counter = 0;
        }

        // Budget check: if we've already hit the cap this second, suppress.
        if self.traces_this_second >= self.max_traces_per_second {
            return false;
        }

        // Slow frames are always traced (within budget).
        if last_frame_total_us >= self.slow_frame_threshold_us {
            self.traces_this_second += 1;
            return true;
        }

        // Uniform sampling.
        if uniform_tick {
            self.traces_this_second += 1;
            return true;
        }

        false
    }

    /// Update the slow-frame threshold to a new p95 latency value.
    ///
    /// Callers should pass the rolling p95 of recent `total_us` values so the
    /// threshold adapts as network conditions change.
    pub fn update_threshold(&mut self, p95_us: u64) {
        self.slow_frame_threshold_us = p95_us;
    }

    /// Reset the per-second budget counter.
    ///
    /// Should be called once per wall-clock second (or equivalent tick).
    pub fn reset_second(&mut self) {
        self.traces_this_second = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_only_trace() -> FrameTrace {
        FrameTrace {
            frame_seq: 1,
            capture_start_us: 1_000,
            capture_end_us: 1_500,  // capture: 500
            classify_end_us: 2_000, // classify: 500
            encode_start_us: 2_000,
            encode_end_us: 4_000, // encode: 2 000
            send_us: 4_100,       // send: 100
            network_recv_us: None,
            decode_end_us: None,
            render_end_us: None,
        }
    }

    fn full_pipeline_trace() -> FrameTrace {
        FrameTrace {
            frame_seq: 2,
            capture_start_us: 0,
            capture_end_us: 300,  // capture: 300
            classify_end_us: 600, // classify: 300
            encode_start_us: 600,
            encode_end_us: 2_600,         // encode: 2 000
            send_us: 2_700,               // send: 100
            network_recv_us: Some(5_700), // network: 3 000
            decode_end_us: Some(7_700),   // decode: 2 000
            render_end_us: Some(8_200),   // render: 500
        }
    }

    #[test]
    fn server_only_breakdown() {
        let trace = server_only_trace();
        let bd = trace.breakdown();

        assert_eq!(bd.capture_us, 500);
        assert_eq!(bd.classify_us, 500);
        assert_eq!(bd.encode_us, 2_000);
        assert_eq!(bd.send_us, 100);
        assert_eq!(bd.network_us, None);
        assert_eq!(bd.decode_us, None);
        assert_eq!(bd.render_us, None);
        // total = send_us - capture_start_us = 4_100 - 1_000
        assert_eq!(bd.total_us, 3_100);
    }

    #[test]
    fn full_pipeline_breakdown() {
        let trace = full_pipeline_trace();
        let bd = trace.breakdown();

        assert_eq!(bd.capture_us, 300);
        assert_eq!(bd.classify_us, 300);
        assert_eq!(bd.encode_us, 2_000);
        assert_eq!(bd.send_us, 100);
        assert_eq!(bd.network_us, Some(3_000));
        assert_eq!(bd.decode_us, Some(2_000));
        assert_eq!(bd.render_us, Some(500));
        // total = render_end_us - capture_start_us = 8_200 - 0
        assert_eq!(bd.total_us, 8_200);
    }

    #[test]
    fn saturating_sub_guards_against_clock_skew() {
        // If client clock is behind server clock the deltas clamp to 0 rather than wrapping.
        let trace = FrameTrace {
            frame_seq: 99,
            capture_start_us: 1_000,
            capture_end_us: 1_000, // zero-duration capture
            classify_end_us: 999,  // slightly behind — should clamp
            encode_start_us: 999,
            encode_end_us: 999,
            send_us: 999,
            network_recv_us: Some(500), // before send — clamp to 0
            decode_end_us: Some(600),
            render_end_us: Some(700),
        };

        let bd = trace.breakdown();
        assert_eq!(bd.capture_us, 0);
        assert_eq!(bd.classify_us, 0);
        assert_eq!(bd.network_us, Some(0));
    }

    // --- FrameTracer tests ---

    #[test]
    fn uniform_sampling_2_traces_in_120_frames() {
        // uniform_rate = 60 → frames 60 and 120 are sampled (2 in 120).
        let mut tracer = FrameTracer::new();
        // Use a latency well below the slow-frame threshold so only uniform ticks fire.
        let fast = 1_000u64; // 1 ms
        let mut count = 0u32;
        for _ in 0..120 {
            if tracer.should_trace(fast) {
                count += 1;
            }
        }
        assert_eq!(count, 2, "expected exactly 2 uniform traces in 120 frames");
    }

    #[test]
    fn slow_frames_always_traced() {
        let mut tracer = FrameTracer::new(); // threshold = 20 ms
        let slow = 30_000u64; // 30 ms → above threshold

        // Drive 5 consecutive slow frames; all should be traced (budget = 10).
        let mut count = 0u32;
        for _ in 0..5 {
            if tracer.should_trace(slow) {
                count += 1;
            }
        }
        assert_eq!(count, 5);
    }

    #[test]
    fn budget_caps_at_max_traces_per_second() {
        let mut tracer = FrameTracer::new(); // max = 10
        let slow = 50_000u64; // always above threshold

        let mut count = 0u32;
        for _ in 0..30 {
            if tracer.should_trace(slow) {
                count += 1;
            }
        }
        assert_eq!(count, 10, "budget must cap traces at max_traces_per_second");
    }

    #[test]
    fn budget_resets_after_reset_second() {
        let mut tracer = FrameTracer::new(); // max = 10
        let slow = 50_000u64;

        // Exhaust budget.
        for _ in 0..30 {
            tracer.should_trace(slow);
        }
        assert_eq!(tracer.traces_this_second, 10);

        // Reset and verify new traces are allowed.
        tracer.reset_second();
        assert_eq!(tracer.traces_this_second, 0);

        let mut count = 0u32;
        for _ in 0..5 {
            if tracer.should_trace(slow) {
                count += 1;
            }
        }
        assert_eq!(count, 5);
    }

    #[test]
    fn update_threshold_changes_slow_frame_detection() {
        let mut tracer = FrameTracer::new(); // default threshold = 20 ms
        // Frame at 15 ms — not slow by default.
        assert!(!tracer.should_trace(15_000));

        // Lower threshold to 10 ms; now 15 ms is slow.
        tracer.update_threshold(10_000);
        // Recheck: budget still open.
        tracer.reset_second();
        assert!(tracer.should_trace(15_000));
    }
}
