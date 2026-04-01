// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use prism_observability::{FrameLatencyBreakdown, FrameTrace, FrameTracer};

/// Records per-frame timing for the capture→encode→send pipeline.
pub struct PipelineTracer {
    tracer: FrameTracer,
    traces: Vec<FrameLatencyBreakdown>,
    max_traces: usize,
}

impl PipelineTracer {
    pub fn new() -> Self {
        Self {
            tracer: FrameTracer::new(),
            traces: Vec::new(),
            max_traces: 1000,
        }
    }

    /// Record a complete frame's pipeline timing.
    pub fn record_frame(&mut self, frame_seq: u64, capture_us: u64, encode_us: u64, send_us: u64) {
        let total = capture_us + encode_us + send_us;
        if self.tracer.should_trace(total) {
            let trace = FrameTrace {
                frame_seq,
                capture_start_us: 0,
                capture_end_us: capture_us,
                classify_end_us: capture_us,
                encode_start_us: capture_us,
                encode_end_us: capture_us + encode_us,
                send_us: capture_us + encode_us + send_us,
                network_recv_us: None,
                decode_end_us: None,
                render_end_us: None,
            };
            let breakdown = trace.breakdown();
            if self.traces.len() >= self.max_traces {
                self.traces.remove(0);
            }
            self.traces.push(breakdown);
        }
    }

    /// Get recent traces for analysis.
    pub fn recent_traces(&self) -> &[FrameLatencyBreakdown] {
        &self.traces
    }

    pub fn trace_count(&self) -> usize {
        self.traces.len()
    }

    /// Update the slow-frame threshold from histogram p95.
    pub fn update_threshold(&mut self, p95_us: u64) {
        self.tracer.update_threshold(p95_us);
    }
}

impl Default for PipelineTracer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A total latency guaranteed to trigger slow-frame tracing (threshold = 20 ms).
    const SLOW_US: u64 = 30_000;
    /// A total latency that falls below the default threshold.
    const FAST_US: u64 = 1_000;

    #[test]
    fn record_frame_adds_trace() {
        let mut pt = PipelineTracer::new();
        // A slow frame (30 ms total) should always be captured.
        pt.record_frame(1, SLOW_US, 0, 0);
        assert_eq!(pt.trace_count(), 1);
        let bd = &pt.recent_traces()[0];
        assert_eq!(bd.capture_us, SLOW_US);
        assert_eq!(bd.encode_us, 0);
    }

    #[test]
    fn slow_frames_always_traced() {
        let mut pt = PipelineTracer::new();
        // Drive 5 slow frames; all should be recorded (budget = 10/s).
        for i in 0..5u64 {
            pt.record_frame(i, SLOW_US, 0, 0);
        }
        assert_eq!(pt.trace_count(), 5);
    }

    #[test]
    fn max_traces_respected() {
        let mut pt = PipelineTracer {
            tracer: {
                // Use a zero threshold so every frame counts as slow and gets traced.
                let mut t = FrameTracer::new();
                t.update_threshold(0);
                // Also raise the per-second budget so we can store many traces.
                t.max_traces_per_second = u32::MAX;
                t
            },
            traces: Vec::new(),
            max_traces: 10,
        };

        // Record 20 frames; only the last 10 should be kept.
        for i in 0..20u64 {
            pt.record_frame(i, FAST_US, 0, 0);
        }

        assert_eq!(pt.trace_count(), 10, "ring buffer should cap at max_traces");
    }
}
