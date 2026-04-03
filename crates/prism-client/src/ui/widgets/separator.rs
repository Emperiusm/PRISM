// SPDX-License-Identifier: AGPL-3.0-or-later
//! Horizontal or vertical separator line widget.

use super::{EventResponse, GlassQuad, PaintContext, Rect, Size, UiEvent, Widget};

pub struct Separator {
    rect: Rect,
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

impl Separator {
    pub fn new() -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl Widget for Separator {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, available.w, 2.0);
        Size {
            w: available.w,
            h: 2.0,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // 1px light line
        let top_rect = Rect::new(self.rect.x, self.rect.y, self.rect.w, 1.0);
        ctx.push_glass_quad(GlassQuad {
            rect: top_rect,
            blur_rect: top_rect,
            tint: [1.0, 1.0, 1.0, 0.08],
            border_color: [0.0, 0.0, 0.0, 0.0],
            corner_radius: 0.0,
            noise_intensity: 0.0,
            ..Default::default()
        });

        // 1px dark shadow line
        let bottom_rect = Rect::new(self.rect.x, self.rect.y + 1.0, self.rect.w, 1.0);
        ctx.push_glass_quad(GlassQuad {
            rect: bottom_rect,
            blur_rect: bottom_rect,
            tint: [0.0, 0.0, 0.0, 0.15],
            border_color: [0.0, 0.0, 0.0, 0.0],
            corner_radius: 0.0,
            noise_intensity: 0.0,
            ..Default::default()
        });
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 300.0, 100.0)
    }

    #[test]
    fn separator_height_is_2() {
        let mut sep = Separator::new();
        let size = sep.layout(available());
        assert!((size.h - 2.0).abs() < 0.01, "h was {}", size.h);
    }

    #[test]
    fn separator_emits_two_quads() {
        let mut sep = Separator::new();
        sep.layout(available());
        let mut ctx = PaintContext::new();
        sep.paint(&mut ctx);
        assert_eq!(ctx.glass_quads.len(), 2);
    }
}
