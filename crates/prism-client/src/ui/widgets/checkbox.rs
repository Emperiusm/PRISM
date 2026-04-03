// SPDX-License-Identifier: AGPL-3.0-or-later
//! Toggle checkbox widget.

use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiEvent, Widget,
};

pub struct Checkbox {
    label: String,
    checked: bool,
    fill_anim: Animation,
    rect: Rect,
}

impl Checkbox {
    pub fn new(label: &str, checked: bool) -> Self {
        let mut fill_anim = Animation::new(EaseCurve::EaseOut, 150.0);
        if checked {
            fill_anim.set_target(1.0);
            // Snap immediately by ticking past full duration
            fill_anim.tick(150.0);
        }
        Self {
            label: label.to_owned(),
            checked,
            fill_anim,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
        self.fill_anim.set_target(if checked { 1.0 } else { 0.0 });
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }
}

impl Widget for Checkbox {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, available.w, 24.0);
        Size {
            w: available.w,
            h: 24.0,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        const BOX_SIZE: f32 = 16.0;
        let box_rect = Rect::new(
            self.rect.x,
            self.rect.y + (self.rect.h - BOX_SIZE) * 0.5,
            BOX_SIZE,
            BOX_SIZE,
        );

        // Box background
        ctx.push_glass_quad(theme::glass_quad(
            box_rect,
            if self.checked {
                [0.22, 0.31, 0.41, 0.94]
            } else {
                [0.15, 0.19, 0.25, 0.90]
            },
            if self.checked {
                [theme::ACCENT[0], theme::ACCENT[1], theme::ACCENT[2], 0.36]
            } else {
                [1.0, 1.0, 1.0, 0.14]
            },
            5.0,
        ));

        if self.fill_anim.value() > 0.01 {
            ctx.push_text_run(TextRun {
                x: box_rect.x + 4.5,
                y: box_rect.y + 1.0,
                text: "x".into(),
                font_size: 12.0,
                color: theme::accent(0.60 + self.fill_anim.value() * 0.30),
                ..Default::default()
            });
        }

        // Label text
        ctx.push_text_run(TextRun {
            x: self.rect.x + BOX_SIZE + 8.0,
            y: self.rect.y + 4.0,
            text: self.label.clone(),
            font_size: 13.0,
            color: theme::TEXT_SECONDARY,
            ..Default::default()
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                if self.rect.contains(*x, *y) {
                    self.checked = !self.checked;
                    self.fill_anim
                        .set_target(if self.checked { 1.0 } else { 0.0 });
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.fill_anim.tick(dt_ms);
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

    #[test]
    fn checkbox_toggles_on_click() {
        let mut cb = Checkbox::new("Enable", false);
        cb.layout(available());

        // First click — should become checked
        let resp1 = cb.handle_event(&UiEvent::MouseDown {
            x: 10.0,
            y: 12.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp1, EventResponse::Consumed));
        assert!(cb.is_checked(), "should be checked after first click");

        // Second click — should become unchecked
        let resp2 = cb.handle_event(&UiEvent::MouseDown {
            x: 10.0,
            y: 12.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp2, EventResponse::Consumed));
        assert!(!cb.is_checked(), "should be unchecked after second click");
    }

    #[test]
    fn checkbox_click_outside_ignored() {
        let mut cb = Checkbox::new("Enable", false);
        cb.layout(available());
        let resp = cb.handle_event(&UiEvent::MouseDown {
            x: 500.0,
            y: 500.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Ignored));
        assert!(!cb.is_checked());
    }
}
