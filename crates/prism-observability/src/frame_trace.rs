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

#[cfg(test)]
mod tests {
    use super::*;

    fn server_only_trace() -> FrameTrace {
        FrameTrace {
            frame_seq: 1,
            capture_start_us: 1_000,
            capture_end_us: 1_500,   // capture: 500
            classify_end_us: 2_000,  // classify: 500
            encode_start_us: 2_000,
            encode_end_us: 4_000,    // encode: 2 000
            send_us: 4_100,          // send: 100
            network_recv_us: None,
            decode_end_us: None,
            render_end_us: None,
        }
    }

    fn full_pipeline_trace() -> FrameTrace {
        FrameTrace {
            frame_seq: 2,
            capture_start_us: 0,
            capture_end_us: 300,       // capture: 300
            classify_end_us: 600,      // classify: 300
            encode_start_us: 600,
            encode_end_us: 2_600,      // encode: 2 000
            send_us: 2_700,            // send: 100
            network_recv_us: Some(5_700),  // network: 3 000
            decode_end_us: Some(7_700),    // decode: 2 000
            render_end_us: Some(8_200),    // render: 500
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
}
