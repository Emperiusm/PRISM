// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod bandwidth;
pub mod one_way_delay;
pub mod trend;
pub mod prober;
pub mod mtu;

use crate::connection::DelayAsymmetry;

// ── QualityRecommendation ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum QualityRecommendation {
    Optimal,
    ReduceBitrate { target_bps: u64 },
    ReduceResolution,
    ReduceFramerate,
    EnableFec { ratio: f32 },
    SwitchToStreamOnly,
    PauseNonEssential,
    ConnectionUnusable,
}

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct ProbeQuality {
    pub rtt_us: u64,
    pub jitter_us: u64,
    pub rtt_score: f32,
    pub jitter_score: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct BandwidthQuality {
    pub send_bps: u64,
    pub recv_bps: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct LossQuality {
    pub loss_rate: f32,
    pub loss_score: f32,
}

// ── ConnectionQuality ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    pub score: f32,
    pub recommendation: QualityRecommendation,
    pub probe: ProbeQuality,
    pub bandwidth: BandwidthQuality,
    pub loss: LossQuality,
    pub asymmetry: DelayAsymmetry,
}

impl ConnectionQuality {
    pub fn compute(
        rtt_us: u64,
        jitter_us: u64,
        loss_rate: f32,
        send_bps: u64,
        recv_bps: u64,
        asymmetry: DelayAsymmetry,
    ) -> Self {
        let rtt_ms = rtt_us as f32 / 1_000.0;
        let jitter_ms = jitter_us as f32 / 1_000.0;

        let rtt_score = if rtt_ms <= 5.0 {
            1.0
        } else if rtt_ms <= 20.0 {
            0.8
        } else if rtt_ms <= 50.0 {
            0.6
        } else if rtt_ms <= 100.0 {
            0.3
        } else {
            0.1
        };

        let loss_score = if loss_rate <= 0.001 {
            1.0
        } else if loss_rate <= 0.01 {
            0.7
        } else if loss_rate <= 0.05 {
            0.4
        } else {
            0.1
        };

        let jitter_score = if jitter_ms <= 1.0 {
            1.0
        } else if jitter_ms <= 5.0 {
            0.8
        } else if jitter_ms <= 20.0 {
            0.5
        } else {
            0.2
        };

        // Multiplicative model: any single severely degraded factor dominates the score.
        let raw: f32 = rtt_score * loss_score * jitter_score;
        let score = raw.clamp(0.0, 1.0);

        let recommendation = if score < 0.2 {
            QualityRecommendation::ConnectionUnusable
        } else if (0.02..=0.10).contains(&loss_rate) {
            QualityRecommendation::EnableFec { ratio: loss_rate * 2.0 }
        } else if score < 0.4 {
            QualityRecommendation::PauseNonEssential
        } else if score < 0.6 {
            QualityRecommendation::ReduceFramerate
        } else if score < 0.7 {
            QualityRecommendation::SwitchToStreamOnly
        } else if score < 0.8 {
            QualityRecommendation::ReduceResolution
        } else if score < 0.9 {
            QualityRecommendation::ReduceBitrate { target_bps: send_bps * 3 / 4 }
        } else {
            QualityRecommendation::Optimal
        };

        Self {
            score,
            recommendation,
            probe: ProbeQuality { rtt_us, jitter_us, rtt_score, jitter_score },
            bandwidth: BandwidthQuality { send_bps, recv_bps },
            loss: LossQuality { loss_rate, loss_score },
            asymmetry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_quality_scores_1() {
        let q = ConnectionQuality::compute(1000, 200, 0.0, 100_000_000, 100_000_000, DelayAsymmetry::Symmetric);
        assert!((q.score - 1.0).abs() < f32::EPSILON);
        assert_eq!(q.recommendation, QualityRecommendation::Optimal);
    }

    #[test]
    fn high_rtt_degrades_score() {
        let q = ConnectionQuality::compute(200_000, 5000, 0.0, 50_000_000, 50_000_000, DelayAsymmetry::Symmetric);
        assert!(q.score < 0.5);
    }

    #[test]
    fn high_loss_degrades_score() {
        let q = ConnectionQuality::compute(5000, 500, 0.10, 50_000_000, 50_000_000, DelayAsymmetry::Symmetric);
        assert!(q.score < 0.5);
    }

    #[test]
    fn high_jitter_degrades_score() {
        let q = ConnectionQuality::compute(5000, 50_000, 0.0, 50_000_000, 50_000_000, DelayAsymmetry::Symmetric);
        assert!(q.score < 0.8);
    }

    #[test]
    fn very_bad_quality_recommends_unusable() {
        let q = ConnectionQuality::compute(500_000, 100_000, 0.20, 1_000_000, 1_000_000, DelayAsymmetry::Symmetric);
        assert_eq!(q.recommendation, QualityRecommendation::ConnectionUnusable);
    }

    #[test]
    fn moderate_loss_recommends_fec() {
        let q = ConnectionQuality::compute(10_000, 2000, 0.03, 50_000_000, 50_000_000, DelayAsymmetry::Symmetric);
        assert!(matches!(q.recommendation, QualityRecommendation::EnableFec { .. }));
    }

    #[test]
    fn score_is_bounded_0_to_1() {
        let q = ConnectionQuality::compute(1_000_000, 1_000_000, 1.0, 0, 0, DelayAsymmetry::Unknown);
        assert!(q.score >= 0.0 && q.score <= 1.0);
        let q = ConnectionQuality::compute(100, 10, 0.0, 1_000_000_000, 1_000_000_000, DelayAsymmetry::Symmetric);
        assert!(q.score >= 0.0 && q.score <= 1.0);
    }
}
