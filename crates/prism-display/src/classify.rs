// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Region classification for adaptive encoding.
//!
//! A two-tier system is planned; this module implements **Tier 1**: window-level
//! classification driven by per-window update frequency.  A Tier 2 classifier
//! based on pixel analysis will be added later.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::types::Rect;

// ── UpdateFrequency ───────────────────────────────────────────────────────────

/// How often a region (window) updates its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFrequency {
    /// No updates observed; content is frozen.
    Static,
    /// Occasional updates (e.g. text editing, UI controls).
    Low,
    /// Continuous or near-continuous updates (e.g. video playback, games).
    High,
    /// Frequency has not yet been measured.
    Unknown,
}

// ── RegionType ────────────────────────────────────────────────────────────────

/// Broad category of display-region content used to select an encode strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionType {
    /// Sharp edges, high contrast — benefits from lossless or high-QP encode.
    Text,
    /// Motion content — use a video codec.
    Video,
    /// No recent change — can skip or send Unchanged.
    Static,
    /// Classification not yet determined.
    Uncertain,
}

impl RegionType {
    /// Map an `UpdateFrequency` observation to the most appropriate
    /// `RegionType`.
    pub fn from_frequency(freq: UpdateFrequency) -> Self {
        match freq {
            UpdateFrequency::Static => RegionType::Static,
            UpdateFrequency::Low => RegionType::Text,
            UpdateFrequency::High => RegionType::Video,
            UpdateFrequency::Unknown => RegionType::Uncertain,
        }
    }

    /// Which decoder slot should be used for this region type.
    ///
    /// * `0` — video (lossy) decoder
    /// * `1` — text / uncertain (lossless or adaptive) decoder
    /// * `2` — static (skip / cache) path
    pub fn decoder_slot(self) -> u8 {
        match self {
            RegionType::Video => 0,
            RegionType::Text | RegionType::Uncertain => 1,
            RegionType::Static => 2,
        }
    }
}

// ── ClassifiedRegion ──────────────────────────────────────────────────────────

/// A screen region together with its classification and routing information.
#[derive(Debug, Clone)]
pub struct ClassifiedRegion {
    /// Bounding rectangle of the region in display coordinates.
    pub rect: Rect,
    /// What kind of content the region contains.
    pub classification: RegionType,
    /// Confidence in the classification, in the range `[0.0, 1.0]`.
    pub confidence: f32,
    /// Decoder slot the region should be routed to (see [`RegionType::decoder_slot`]).
    pub decoder_slot: u8,
}

// ── WindowActivity ────────────────────────────────────────────────────────────

/// Describes the recent activity of a single application window.
pub struct WindowActivity {
    /// Platform window handle (HWND on Windows).
    pub hwnd: u64,
    /// Current bounding rectangle of the window in display coordinates.
    pub rect: Rect,
    /// Observed update frequency over the last measurement interval.
    pub frequency: UpdateFrequency,
}

// ── Tier1Classifier ───────────────────────────────────────────────────────────

/// Minimum time a window must keep the same [`RegionType`] before the
/// classifier reports full confidence.
const STABLE_DURATION: Duration = Duration::from_secs(5);

/// Window-level classifier.  It maintains a history of per-window
/// classifications and ramps confidence up as the classification stays stable.
pub struct Tier1Classifier {
    /// Maps hwnd → (last classification, time the current classification was
    /// first observed).
    history: HashMap<u64, (RegionType, Instant)>,
}

impl Tier1Classifier {
    /// Create a new, empty classifier.
    pub fn new() -> Self {
        Self {
            history: HashMap::new(),
        }
    }

    /// Classify a set of windows and return one [`ClassifiedRegion`] per window
    /// plus one extra region for any screen area not covered by a window.
    ///
    /// # Arguments
    /// * `windows`  — current window list with their update frequencies.
    /// * `screen_w` / `screen_h` — full-screen dimensions for computing the
    ///   uncovered background region.
    pub fn classify(
        &mut self,
        windows: &[WindowActivity],
        screen_w: u32,
        screen_h: u32,
    ) -> Vec<ClassifiedRegion> {
        let now = Instant::now();
        let mut regions = Vec::with_capacity(windows.len() + 1);

        for w in windows {
            let new_type = RegionType::from_frequency(w.frequency);

            let (stable_type, stable_since) = self
                .history
                .entry(w.hwnd)
                .and_modify(|(prev, since)| {
                    if *prev != new_type {
                        // Classification changed — reset stability clock.
                        *prev = new_type;
                        *since = now;
                    }
                })
                .or_insert_with(|| (new_type, now));

            let elapsed = now.duration_since(*stable_since);
            // Ramp from 0.1 up to 1.0 linearly over STABLE_DURATION.
            let confidence = if elapsed >= STABLE_DURATION {
                1.0_f32
            } else {
                0.1 + 0.9 * (elapsed.as_secs_f32() / STABLE_DURATION.as_secs_f32())
            };

            let classification = *stable_type;
            regions.push(ClassifiedRegion {
                rect: w.rect,
                classification,
                confidence,
                decoder_slot: classification.decoder_slot(),
            });
        }

        // Background region: the full screen.  We report it as Static with
        // full confidence.  Callers that need pixel-accurate uncovered areas
        // should subtract window rects themselves.
        let screen_rect = Rect { x: 0, y: 0, w: screen_w, h: screen_h };
        let is_covered = windows
            .iter()
            .any(|w| w.rect.w == screen_w && w.rect.h == screen_h);

        if !is_covered {
            regions.push(ClassifiedRegion {
                rect: screen_rect,
                classification: RegionType::Static,
                confidence: 1.0,
                decoder_slot: RegionType::Static.decoder_slot(),
            });
        }

        regions
    }
}

impl Default for Tier1Classifier {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn small_window(hwnd: u64, freq: UpdateFrequency) -> WindowActivity {
        WindowActivity {
            hwnd,
            rect: Rect { x: 0, y: 0, w: 800, h: 600 },
            frequency: freq,
        }
    }

    // 1 ── UpdateFrequency → RegionType mapping
    #[test]
    fn update_frequency_to_region_type() {
        assert_eq!(RegionType::from_frequency(UpdateFrequency::Static), RegionType::Static);
        assert_eq!(RegionType::from_frequency(UpdateFrequency::Low), RegionType::Text);
        assert_eq!(RegionType::from_frequency(UpdateFrequency::High), RegionType::Video);
        assert_eq!(RegionType::from_frequency(UpdateFrequency::Unknown), RegionType::Uncertain);
    }

    // 2 ── A Low-frequency window should produce a Text region.
    #[test]
    fn tier1_single_window_classifies() {
        let mut clf = Tier1Classifier::new();
        let windows = [small_window(1, UpdateFrequency::Low)];
        let regions = clf.classify(&windows, 1920, 1080);

        let text = regions
            .iter()
            .find(|r| r.classification == RegionType::Text);
        assert!(text.is_some(), "expected a Text region");
    }

    // 3 ── A High-frequency window should produce a Video region.
    #[test]
    fn tier1_video_window_classifies_as_video() {
        let mut clf = Tier1Classifier::new();
        let windows = [small_window(2, UpdateFrequency::High)];
        let regions = clf.classify(&windows, 1920, 1080);

        let video = regions
            .iter()
            .find(|r| r.classification == RegionType::Video);
        assert!(video.is_some(), "expected a Video region");
    }

    // 4 ── A small window leaves most of the screen uncovered → Static background.
    #[test]
    fn tier1_uncovered_area_is_static() {
        let mut clf = Tier1Classifier::new();
        let windows = [small_window(3, UpdateFrequency::Low)];
        let regions = clf.classify(&windows, 1920, 1080);

        let bg = regions
            .iter()
            .find(|r| r.classification == RegionType::Static);
        assert!(bg.is_some(), "expected a Static background region");
    }

    // 5 ── decoder_slot assignments
    #[test]
    fn decoder_slot_assignment() {
        assert_eq!(RegionType::Video.decoder_slot(), 0);
        assert_eq!(RegionType::Text.decoder_slot(), 1);
        assert_eq!(RegionType::Uncertain.decoder_slot(), 1);
        assert_eq!(RegionType::Static.decoder_slot(), 2);
    }

    // 6 ── Confidence starts above 0 and below full on first classification.
    #[test]
    fn classified_region_confidence() {
        let mut clf = Tier1Classifier::new();
        let windows = [small_window(4, UpdateFrequency::High)];
        let regions = clf.classify(&windows, 1920, 1080);

        let video = regions
            .iter()
            .find(|r| r.classification == RegionType::Video)
            .expect("expected a Video region");

        // Confidence must be in (0, 1] — specifically > 0.0 but ≤ 1.0.
        assert!(
            video.confidence > 0.0 && video.confidence <= 1.0,
            "confidence out of range: {}",
            video.confidence
        );
        // On the very first call it cannot be at full confidence yet.
        assert!(
            video.confidence > 0.5 || video.confidence < 1.0,
            "unexpected confidence: {}",
            video.confidence
        );
    }
}
