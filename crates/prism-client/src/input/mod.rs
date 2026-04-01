// SPDX-License-Identifier: AGPL-3.0-or-later
//! Input routing — overlay vs remote forwarding, double-tap detection, drag.

pub mod double_tap;
pub mod drag;

use crate::ui::widgets::UiEvent;

// ---------------------------------------------------------------------------
// InputTarget
// ---------------------------------------------------------------------------

/// Determines which recipient gets a routed input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    Remote,
    Overlay,
}

// ---------------------------------------------------------------------------
// InputCoalescer
// ---------------------------------------------------------------------------

/// Reduces high-frequency mouse-move and scroll floods to a single event per
/// frame before they are forwarded.
pub struct InputCoalescer {
    pending_mouse: Option<(f32, f32)>,
    pending_scroll: (f32, f32),
}

impl InputCoalescer {
    pub fn new() -> Self {
        Self {
            pending_mouse: None,
            pending_scroll: (0.0, 0.0),
        }
    }

    /// Replace any pending mouse position with the latest one.
    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.pending_mouse = Some((x, y));
    }

    /// Accumulate scroll deltas.
    pub fn scroll(&mut self, dx: f32, dy: f32) {
        self.pending_scroll.0 += dx;
        self.pending_scroll.1 += dy;
    }

    /// Push coalesced events into `out` and reset internal state.
    pub fn drain(&mut self, out: &mut Vec<UiEvent>) {
        if let Some((x, y)) = self.pending_mouse.take() {
            out.push(UiEvent::MouseMove { x, y });
        }
        let (dx, dy) = self.pending_scroll;
        if dx != 0.0 || dy != 0.0 {
            out.push(UiEvent::Scroll { dx, dy });
            self.pending_scroll = (0.0, 0.0);
        }
    }
}

impl Default for InputCoalescer {
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

    #[test]
    fn coalesce_multiple_mouse_moves() {
        let mut c = InputCoalescer::new();
        c.mouse_move(1.0, 2.0);
        c.mouse_move(3.0, 4.0);
        c.mouse_move(5.0, 6.0);

        let mut out = Vec::new();
        c.drain(&mut out);

        assert_eq!(out.len(), 1, "should coalesce to a single MouseMove");
        match out[0] {
            UiEvent::MouseMove { x, y } => {
                assert_eq!(x, 5.0);
                assert_eq!(y, 6.0);
            }
            _ => panic!("expected MouseMove"),
        }
    }

    #[test]
    fn coalesce_scroll_accumulates() {
        let mut c = InputCoalescer::new();
        c.scroll(1.0, 2.0);
        c.scroll(3.0, 4.0);

        let mut out = Vec::new();
        c.drain(&mut out);

        assert_eq!(out.len(), 1, "should coalesce to a single Scroll");
        match out[0] {
            UiEvent::Scroll { dx, dy } => {
                assert_eq!(dx, 4.0);
                assert_eq!(dy, 6.0);
            }
            _ => panic!("expected Scroll"),
        }
    }
}
