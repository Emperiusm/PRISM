// SPDX-License-Identifier: AGPL-3.0-or-later
//! Clickable button widget with hover/press animation.

use super::{
    EventResponse, GlassQuad, GlowRect, MouseButton, PaintContext, Rect, Size, TextRun, UiAction,
    UiEvent, Widget,
};
use crate::renderer::animation::{Animation, EaseCurve};

pub struct Button {
    label: String,
    action: UiAction,
    rect: Rect,
    hover_anim: Animation,
    hovered: bool,
}

impl Button {
    pub fn new(label: &str, action: UiAction) -> Self {
        Self {
            label: label.to_owned(),
            action,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            hover_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            hovered: false,
        }
    }
}

impl Widget for Button {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 36.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Glass background
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.55, 0.36, 0.96, 0.15],
            border_color: [1.0, 1.0, 1.0, 0.2],
            corner_radius: 6.0,
            noise_intensity: 0.03,
        });

        // Hover glow
        if self.hover_anim.value() > 0.01 {
            ctx.push_glow_rect(GlowRect {
                rect: self.rect,
                color: [0.55, 0.36, 0.96, self.hover_anim.value()],
                spread: 8.0,
                intensity: self.hover_anim.value(),
            });
        }

        // Centered label text
        let text_x = self.rect.x + self.rect.w * 0.5;
        let text_y = self.rect.y + self.rect.h * 0.5;
        ctx.push_text_run(TextRun {
            x: text_x,
            y: text_y,
            text: self.label.clone(),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                let now_hovered = self.rect.contains(*x, *y);
                if now_hovered != self.hovered {
                    self.hovered = now_hovered;
                    self.hover_anim
                        .set_target(if now_hovered { 1.0 } else { 0.0 });
                }
                EventResponse::Ignored
            }
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                if self.rect.contains(*x, *y) {
                    EventResponse::Action(self.action.clone())
                } else {
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.hover_anim.tick(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 200.0, 100.0)
    }

    fn make_button() -> Button {
        Button::new("Connect", UiAction::AddServer)
    }

    #[test]
    fn button_emits_glass_quad_and_text() {
        let mut btn = make_button();
        btn.layout(available());
        let mut ctx = PaintContext::new();
        btn.paint(&mut ctx);
        assert_eq!(ctx.glass_quads.len(), 1);
        assert_eq!(ctx.text_runs.len(), 1);
        assert_eq!(ctx.text_runs[0].text, "Connect");
    }

    #[test]
    fn button_hover_adds_glow() {
        let mut btn = make_button();
        btn.layout(available());

        // Move mouse inside the button
        btn.handle_event(&UiEvent::MouseMove { x: 100.0, y: 18.0 });
        // Animate enough to get visible glow
        btn.animate(200.0);

        let mut ctx = PaintContext::new();
        btn.paint(&mut ctx);
        assert!(
            !ctx.glow_rects.is_empty(),
            "expected a glow rect after hovering"
        );
    }

    #[test]
    fn button_click_returns_action() {
        let mut btn = make_button();
        btn.layout(available());
        let resp = btn.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 18.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Action(_)));
    }

    #[test]
    fn button_click_outside_ignored() {
        let mut btn = make_button();
        btn.layout(available());
        let resp = btn.handle_event(&UiEvent::MouseDown {
            x: 500.0,
            y: 500.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Ignored));
    }
}
