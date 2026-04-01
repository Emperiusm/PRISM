// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drag handler — tracks mouse-drag state for movable overlay panels.

use crate::ui::widgets::Rect;

// ---------------------------------------------------------------------------
// DragState
// ---------------------------------------------------------------------------

/// Tracks a mouse drag operation on a floating panel.
pub struct DragState {
    dragging: bool,
    offset_x: f32,
    offset_y: f32,
}

impl DragState {
    pub fn new() -> Self {
        Self {
            dragging: false,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    /// Begin a drag.  `offset_x/y` are mouse position relative to the panel
    /// origin so the panel does not jump when the drag starts.
    pub fn start(&mut self, mouse_x: f32, mouse_y: f32, panel_rect: &Rect) {
        self.dragging = true;
        self.offset_x = mouse_x - panel_rect.x;
        self.offset_y = mouse_y - panel_rect.y;
    }

    /// Move the panel to follow the mouse.  No-op when not dragging.
    pub fn update(&mut self, mouse_x: f32, mouse_y: f32, panel_rect: &mut Rect) {
        if self.dragging {
            panel_rect.x = mouse_x - self.offset_x;
            panel_rect.y = mouse_y - self.offset_y;
        }
    }

    /// End the drag.
    pub fn stop(&mut self) {
        self.dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
}

impl Default for DragState {
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
    fn drag_moves_panel() {
        let mut drag = DragState::new();
        let mut rect = Rect::new(100.0, 100.0, 200.0, 150.0);

        // Start drag at (120, 110) — offset should be (20, 10)
        drag.start(120.0, 110.0, &rect);
        assert!(drag.is_dragging());

        // Update to mouse position (220, 210) → panel should move to (200, 200)
        drag.update(220.0, 210.0, &mut rect);
        assert_eq!(rect.x, 200.0);
        assert_eq!(rect.y, 200.0);

        drag.stop();
        assert!(!drag.is_dragging());
    }
}
