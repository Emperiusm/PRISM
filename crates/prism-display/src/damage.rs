//! Damage rect tracking: merging and codec alignment utilities.
//!
//! Two concerns are handled here:
//!
//! 1. **Damage merging** — collapse many small dirty rectangles into a smaller
//!    set of bounding rects, controlled by a proximity threshold.
//! 2. **Macroblock / superblock snapping** — expand a rect outward to an
//!    alignment boundary so encoders receive codec-aligned input regions.

use crate::types::Rect;

// ── Proximity helper ──────────────────────────────────────────────────────────

/// Returns `true` when the gap between `a` and `b` on both axes is
/// ≤ `threshold` pixels (i.e. they are close enough to be worth merging).
///
/// Overlapping rects trivially satisfy the condition.
pub fn rects_within_threshold(a: &Rect, b: &Rect, threshold: i32) -> bool {
    let ax2 = a.x + a.w as i32;
    let bx2 = b.x + b.w as i32;
    let ay2 = a.y + a.h as i32;
    let by2 = b.y + b.h as i32;

    // Horizontal gap: positive when they don't overlap in x.
    let gap_x = (a.x.max(b.x) - ax2.min(bx2)).max(0);
    // Vertical gap: positive when they don't overlap in y.
    let gap_y = (a.y.max(b.y) - ay2.min(by2)).max(0);

    gap_x <= threshold && gap_y <= threshold
}

// ── merge_damage_rects ────────────────────────────────────────────────────────

/// Merge an arbitrary set of dirty rectangles into a smaller set of bounding
/// rects.
///
/// Two rects are merged when the gap between them (on both axes) is ≤
/// `threshold` pixels.  A `threshold` of 0 merges only overlapping / touching
/// rects; larger values trade a larger merged area for fewer rects.
///
/// The algorithm makes **two passes**:
/// 1. Iterate the input; for each rect try to extend an existing output rect.
///    If no candidate is found, start a new output rect.
/// 2. Merge any output rects that now overlap or fall within threshold of each
///    other (since pass 1 can produce adjacent outputs).
pub fn merge_damage_rects(rects: &[Rect], threshold: i32) -> Vec<Rect> {
    if rects.is_empty() {
        return Vec::new();
    }

    // ── Pass 1: fold each input rect into an existing output rect or start a
    //            new one.
    let mut out: Vec<Rect> = Vec::new();
    for &r in rects {
        let mut merged_into: Option<usize> = None;
        for (i, o) in out.iter().enumerate() {
            if rects_within_threshold(o, &r, threshold) {
                merged_into = Some(i);
                break;
            }
        }
        match merged_into {
            Some(i) => out[i] = out[i].merge(&r),
            None => out.push(r),
        }
    }

    // ── Pass 2: merge any output rects that are now within threshold of each
    //            other (pass 1 can leave adjacent outputs).
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < out.len() {
            let mut j = i + 1;
            let mut absorbed = false;
            while j < out.len() {
                if rects_within_threshold(&out[i], &out[j], threshold) {
                    let merged = out[i].merge(&out[j]);
                    out[i] = merged;
                    out.swap_remove(j);
                    changed = true;
                    absorbed = true;
                    // Don't advance j — the element at j is now a different rect.
                } else {
                    j += 1;
                }
            }
            if absorbed {
                // Retry i against the remaining rects.
                continue;
            }
            i += 1;
        }
    }

    out
}

// ── macroblock_snap ───────────────────────────────────────────────────────────

/// Snap `rect` **outward** to `alignment`-pixel boundaries.
///
/// `alignment` must be a power of two (e.g. 16 for H.264/H.265 macroblocks,
/// 64 for AV1 superblocks).
///
/// The returned rect is guaranteed to:
/// * Have `x`, `y`, `x + w`, and `y + h` all divisible by `alignment`.
/// * Contain every pixel of the original `rect` (i.e. it can only grow, never
///   shrink).
pub fn macroblock_snap(rect: Rect, alignment: i32) -> Rect {
    debug_assert!(alignment > 0 && (alignment & (alignment - 1)) == 0,
        "alignment must be a positive power of two");

    let mask = !(alignment - 1);

    // Snap top-left *down* (floor).
    let x = rect.x & mask;
    let y = rect.y & mask;

    // Snap bottom-right *up* (ceiling).
    let x2_raw = rect.x + rect.w as i32;
    let y2_raw = rect.y + rect.h as i32;
    let x2 = (x2_raw + alignment - 1) & mask;
    let y2 = (y2_raw + alignment - 1) & mask;

    Rect {
        x,
        y,
        w: (x2 - x) as u32,
        h: (y2 - y) as u32,
    }
}

/// Convenience wrapper: snap to 16-pixel macroblocks (H.264 / H.265).
#[inline]
pub fn macroblock_snap_16(rect: Rect) -> Rect {
    macroblock_snap(rect, 16)
}

/// Convenience wrapper: snap to 64-pixel superblocks (AV1).
#[inline]
pub fn superblock_snap_64(rect: Rect) -> Rect {
    macroblock_snap(rect, 64)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1 ── Empty input returns empty output.
    #[test]
    fn no_rects_returns_empty() {
        assert!(merge_damage_rects(&[], 0).is_empty());
    }

    // 2 ── A single rect is returned unchanged.
    #[test]
    fn single_rect_unchanged() {
        let r = Rect { x: 10, y: 20, w: 50, h: 30 };
        let out = merge_damage_rects(&[r], 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], r);
    }

    // 3 ── Two overlapping rects collapse to one.
    #[test]
    fn adjacent_rects_merge() {
        let a = Rect { x: 0, y: 0, w: 100, h: 100 };
        let b = Rect { x: 50, y: 50, w: 100, h: 100 };
        let out = merge_damage_rects(&[a, b], 0);
        assert_eq!(out.len(), 1);
        let m = out[0];
        assert_eq!(m, Rect { x: 0, y: 0, w: 150, h: 150 });
    }

    // 4 ── Two rects far apart remain separate.
    #[test]
    fn distant_rects_stay_separate() {
        let a = Rect { x: 0, y: 0, w: 10, h: 10 };
        let b = Rect { x: 500, y: 500, w: 10, h: 10 };
        let out = merge_damage_rects(&[a, b], 4);
        assert_eq!(out.len(), 2);
    }

    // 5 ── macroblock_snap_16 expands to 16-pixel boundaries and covers original.
    #[test]
    fn macroblock_snap_aligns_to_16() {
        let r = Rect { x: 3, y: 5, w: 45, h: 33 };
        let s = macroblock_snap_16(r);

        assert_eq!(s.x % 16, 0, "x not aligned: {}", s.x);
        assert_eq!(s.y % 16, 0, "y not aligned: {}", s.y);
        assert_eq!((s.x + s.w as i32) % 16, 0, "right not aligned");
        assert_eq!((s.y + s.h as i32) % 16, 0, "bottom not aligned");

        // Snapped rect must contain the original.
        assert!(s.x <= r.x);
        assert!(s.y <= r.y);
        assert!(s.x + s.w as i32 >= r.x + r.w as i32);
        assert!(s.y + s.h as i32 >= r.y + r.h as i32);
    }

    // 6 ── A rect already on 16-pixel boundaries is unchanged.
    //     (1920×1088 — the H.264-padded height used in practice.)
    #[test]
    fn macroblock_snap_already_aligned() {
        let r = Rect { x: 0, y: 0, w: 1920, h: 1088 };
        assert_eq!(macroblock_snap_16(r), r);
    }

    // 7 ── superblock_snap_64 produces 64-pixel-aligned boundaries.
    #[test]
    fn superblock_snap_64_for_av1() {
        let r = Rect { x: 1, y: 1, w: 127, h: 65 };
        let s = superblock_snap_64(r);

        assert_eq!(s.x % 64, 0, "x not aligned: {}", s.x);
        assert_eq!(s.y % 64, 0, "y not aligned: {}", s.y);
        assert_eq!((s.x + s.w as i32) % 64, 0, "right not aligned");
        assert_eq!((s.y + s.h as i32) % 64, 0, "bottom not aligned");

        assert!(s.x <= r.x);
        assert!(s.y <= r.y);
        assert!(s.x + s.w as i32 >= r.x + r.w as i32);
        assert!(s.y + s.h as i32 >= r.y + r.h as i32);
    }

    // 8 ── Many small adjacent rects collapse to far fewer outputs.
    #[test]
    fn many_small_rects_merge_to_few() {
        // 20 adjacent 10×10 rects in a horizontal strip.
        let rects: Vec<Rect> = (0..20)
            .map(|i| Rect { x: i * 10, y: 0, w: 10, h: 10 })
            .collect();
        let out = merge_damage_rects(&rects, 0);
        assert!(out.len() < 5, "expected fewer than 5 merged rects, got {}", out.len());
    }
}
