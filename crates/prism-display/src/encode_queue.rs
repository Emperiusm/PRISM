use std::collections::VecDeque;

use crate::frame::EncodeJob;

/// A two-tier priority queue for `EncodeJob` dispatch.
///
/// High-priority jobs (e.g. forced IDR frames, foreground display) are always
/// drained before normal-priority jobs.
pub struct EncodeQueue {
    high: VecDeque<EncodeJob>,
    normal: VecDeque<EncodeJob>,
}

impl EncodeQueue {
    pub fn new() -> Self {
        Self {
            high: VecDeque::new(),
            normal: VecDeque::new(),
        }
    }

    /// Push a high-priority encode job.
    pub fn push_high(&mut self, job: EncodeJob) {
        self.high.push_back(job);
    }

    /// Push a normal-priority encode job.
    pub fn push_normal(&mut self, job: EncodeJob) {
        self.normal.push_back(job);
    }

    /// Remove and return the next job, draining the high-priority queue first.
    pub fn steal(&mut self) -> Option<EncodeJob> {
        if let Some(job) = self.high.pop_front() {
            return Some(job);
        }
        self.normal.pop_front()
    }

    /// Total number of pending jobs across both queues.
    pub fn len(&self) -> usize {
        self.high.len() + self.normal.len()
    }

    /// True when both queues are empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for EncodeQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DisplayId, QualityTier, Rect, TextureFormat, SharedTexture};
    use crate::frame::FrameMetadata;
    use crate::classify::RegionType;

    fn dummy_job(seq: u32) -> EncodeJob {
        EncodeJob {
            frame_seq: seq,
            display_id: DisplayId(0),
            region_rect: Rect { x: 0, y: 0, w: 100, h: 100 },
            region_type: RegionType::Video,
            texture: SharedTexture { handle: 0, width: 100, height: 100, format: TextureFormat::Bgra8 },
            target_bitrate: 5_000_000,
            force_keyframe: false,
            quality_tier: QualityTier::Normal,
            expected_regions: 1,
            frame_meta: FrameMetadata {
                display_id: DisplayId(0),
                capture_time_us: 0,
                is_preview: false,
                replaces_seq: None,
                total_regions: 1,
            },
        }
    }

    #[test]
    fn high_priority_drains_first() {
        let mut q = EncodeQueue::new();
        q.push_normal(dummy_job(10));
        q.push_high(dummy_job(1));
        q.push_high(dummy_job(2));
        q.push_normal(dummy_job(20));

        // High-priority jobs arrive first, in insertion order.
        assert_eq!(q.steal().unwrap().frame_seq, 1);
        assert_eq!(q.steal().unwrap().frame_seq, 2);
        // Then normal.
        assert_eq!(q.steal().unwrap().frame_seq, 10);
        assert_eq!(q.steal().unwrap().frame_seq, 20);
    }

    #[test]
    fn empty_returns_none() {
        let mut q = EncodeQueue::new();
        assert!(q.steal().is_none());
    }

    #[test]
    fn len_tracks_both() {
        let mut q = EncodeQueue::new();
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());

        q.push_high(dummy_job(1));
        q.push_normal(dummy_job(2));
        q.push_normal(dummy_job(3));
        assert_eq!(q.len(), 3);
        assert!(!q.is_empty());

        q.steal();
        assert_eq!(q.len(), 2);
        q.steal();
        q.steal();
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
    }
}
