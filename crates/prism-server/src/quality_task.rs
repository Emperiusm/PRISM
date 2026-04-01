use std::sync::Arc;

use arc_swap::ArcSwap;
use bytes::{Bytes, BytesMut};
use prism_protocol::{
    channel::CHANNEL_CONTROL,
    header::{PrismHeader, HEADER_SIZE, PROTOCOL_VERSION},
};
use prism_session::control_msg::PROBE_REQUEST;
use prism_transport::{
    connection::{DelayAsymmetry, TransportMetrics},
    quality::ConnectionQuality,
    quality::prober::ProbePayload,
};

// ── QualityCache ──────────────────────────────────────────────────────────────

/// Lock-free cache of the latest [`ConnectionQuality`] snapshot.
///
/// Readers never block: `load()` is a single atomic pointer swap.
/// Writers race-to-last-write via [`ArcSwap::store`].
pub struct QualityCache {
    inner: ArcSwap<ConnectionQuality>,
}

impl QualityCache {
    /// Create a new cache pre-populated with an optimal quality snapshot.
    pub fn new() -> Self {
        let optimal = ConnectionQuality::compute(
            1_000,          // rtt_us  — 1 ms
            100,            // jitter_us — 0.1 ms
            0.0,            // loss_rate
            1_000_000_000,  // send_bps — 1 Gbps
            1_000_000_000,  // recv_bps — 1 Gbps
            DelayAsymmetry::Symmetric,
        );
        Self {
            inner: ArcSwap::from_pointee(optimal),
        }
    }

    /// Replace the cached quality with `quality`.
    pub fn update(&self, quality: ConnectionQuality) {
        self.inner.store(Arc::new(quality));
    }

    /// Return the current snapshot (cheap Arc clone).
    pub fn load(&self) -> Arc<ConnectionQuality> {
        self.inner.load_full()
    }
}

impl Default for QualityCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── build_probe_datagram ──────────────────────────────────────────────────────

/// Build a PRISM datagram containing a `PROBE_REQUEST` control message.
///
/// Layout: `[PrismHeader (16 B)][ProbePayload (12 B)]` = 28 bytes total.
pub fn build_probe_datagram(payload: &ProbePayload) -> Bytes {
    let probe_bytes = payload.to_bytes();
    let header = PrismHeader {
        version: PROTOCOL_VERSION,
        channel_id: CHANNEL_CONTROL,
        msg_type: PROBE_REQUEST,
        flags: 0,
        sequence: payload.seq,
        timestamp_us: (payload.sender_timestamp_us & 0xFFFF_FFFF) as u32,
        payload_length: probe_bytes.len() as u32,
    };

    let mut buf = BytesMut::with_capacity(HEADER_SIZE + probe_bytes.len());
    header.encode(&mut buf);
    buf.extend_from_slice(&probe_bytes);
    buf.freeze()
}

// ── evaluate_quality ──────────────────────────────────────────────────────────

/// Derive a [`ConnectionQuality`] from raw [`TransportMetrics`].
///
/// Uses `probe_rtt_us` for RTT, `rtt_variance_us` for jitter, and
/// the actual bandwidth fields. Falls back to `rtt_us` when `probe_rtt_us`
/// is zero (QUIC congestion-controller RTT).
pub fn evaluate_quality(metrics: &TransportMetrics) -> ConnectionQuality {
    let rtt_us = if metrics.probe_rtt_us > 0 {
        metrics.probe_rtt_us
    } else {
        metrics.rtt_us
    };

    ConnectionQuality::compute(
        rtt_us,
        metrics.rtt_variance_us,
        metrics.loss_rate,
        metrics.actual_send_bps,
        metrics.actual_recv_bps,
        metrics.delay_asymmetry,
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_protocol::header::HEADER_SIZE;
    use prism_session::control_msg::PROBE_REQUEST;
    use prism_transport::quality::QualityRecommendation;

    // 1. Cache starts at optimal quality.
    #[test]
    fn cache_starts_optimal() {
        let cache = QualityCache::new();
        let q = cache.load();
        assert_eq!(q.recommendation, QualityRecommendation::Optimal);
        assert!(q.score >= 0.9, "expected score ≥ 0.9, got {}", q.score);
    }

    // 2. update() is immediately visible via load().
    #[test]
    fn cache_update_reflects() {
        let cache = QualityCache::new();
        let bad = ConnectionQuality::compute(
            500_000, 100_000, 0.20,
            1_000_000, 1_000_000,
            DelayAsymmetry::Unknown,
        );
        cache.update(bad);
        let q = cache.load();
        assert_eq!(q.recommendation, QualityRecommendation::ConnectionUnusable);
    }

    // 3. build_probe_datagram produces a valid PRISM control datagram.
    #[test]
    fn probe_datagram_valid() {
        use prism_protocol::header::PrismHeader;
        use prism_transport::quality::prober::ProbePayload;

        let payload = ProbePayload { seq: 7, sender_timestamp_us: 123_456_789 };
        let dgram = build_probe_datagram(&payload);

        // Total length = header (16) + probe payload (12)
        assert_eq!(dgram.len(), HEADER_SIZE + 12);

        // Header decodes correctly.
        let hdr = PrismHeader::decode_from_slice(&dgram).expect("decode header");
        assert_eq!(hdr.channel_id, CHANNEL_CONTROL);
        assert_eq!(hdr.msg_type, PROBE_REQUEST);
        assert_eq!(hdr.payload_length, 12);

        // Probe payload roundtrips.
        let probe_back = ProbePayload::from_bytes(&dgram[HEADER_SIZE..]).expect("decode probe");
        assert_eq!(probe_back.seq, 7);
        assert_eq!(probe_back.sender_timestamp_us, 123_456_789);
    }

    // 4. Good metrics → Optimal recommendation.
    #[test]
    fn good_metrics_optimal() {
        let metrics = TransportMetrics {
            rtt_us: 1_000,
            rtt_variance_us: 100,
            loss_rate: 0.0,
            actual_send_bps: 100_000_000,
            actual_recv_bps: 100_000_000,
            delay_asymmetry: DelayAsymmetry::Symmetric,
            probe_rtt_us: 1_000,
            ..TransportMetrics::default()
        };
        let q = evaluate_quality(&metrics);
        assert_eq!(q.recommendation, QualityRecommendation::Optimal);
        assert!(q.score >= 0.9);
    }

    // 5. Bad metrics → score degrades below 0.5.
    #[test]
    fn bad_metrics_degrades() {
        let metrics = TransportMetrics {
            rtt_us: 300_000,
            rtt_variance_us: 80_000,
            loss_rate: 0.15,
            actual_send_bps: 1_000_000,
            actual_recv_bps: 1_000_000,
            delay_asymmetry: DelayAsymmetry::Unknown,
            probe_rtt_us: 300_000,
            ..TransportMetrics::default()
        };
        let q = evaluate_quality(&metrics);
        assert!(q.score < 0.5, "expected score < 0.5, got {}", q.score);
        assert_ne!(q.recommendation, QualityRecommendation::Optimal);
    }
}
