// SPDX-License-Identifier: AGPL-3.0-or-later
//! On/off toggle switch widget.

use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::{EventResponse, MouseButton, PaintContext, Rect, Size, UiEvent, Widget};

const TRACK_W: f32 = 44.0;
const TRACK_H: f32 = 22.0;
const THUMB_SIZE: f32 = 16.0;
const THUMB_PAD: f32 = 3.0;

pub struct Toggle {
    on: bool,
    rect: Rect,
    slide_anim: Animation,
}

impl Toggle {
    pub fn new(on: bool) -> Self {
        let mut anim = Animation::new(EaseCurve::EaseOut, 150.0);
        anim.set_target(if on { 1.0 } else { 0.0 });
        for _ in 0..20 {
            anim.tick(20.0);
        }
        Self {
            on,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            slide_anim: anim,
        }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        self.slide_anim.set_target(if on { 1.0 } else { 0.0 });
    }

    fn track_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + self.rect.w - TRACK_W,
            self.rect.y + (self.rect.h - TRACK_H) * 0.5,
            TRACK_W,
            TRACK_H,
        )
    }
}

impl Widget for Toggle {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, available.w, TRACK_H);
        Size {
            w: available.w,
            h: TRACK_H,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let track = self.track_rect();
        ctx.push_glass_quad(theme::toggle_track(track, self.on));

        let t = self.slide_anim.value();
        let thumb_x = track.x + THUMB_PAD + t * (TRACK_W - THUMB_SIZE - THUMB_PAD * 2.0);
        let thumb_y = track.y + (TRACK_H - THUMB_SIZE) * 0.5;
        let thumb_rect = Rect::new(thumb_x, thumb_y, THUMB_SIZE, THUMB_SIZE);
        ctx.push_glass_quad(theme::toggle_thumb(thumb_rect, self.on));
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                let track = self.track_rect();
                if track.contains(*x, *y) {
                    self.on = !self.on;
                    self.slide_anim.set_target(if self.on { 1.0 } else { 0.0 });
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.slide_anim.tick(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 200.0, 40.0)
    }

    #[test]
    fn toggle_starts_off() {
        let t = Toggle::new(false);
        assert!(!t.is_on());
    }

    #[test]
    fn toggle_starts_on() {
        let t = Toggle::new(true);
        assert!(t.is_on());
    }

    #[test]
    fn click_toggles_state() {
        let mut t = Toggle::new(false);
        t.layout(available());

        let track = t.track_rect();
        let resp = t.handle_event(&UiEvent::MouseDown {
            x: track.x + 10.0,
            y: track.y + 5.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Consumed));
        assert!(t.is_on());
    }

    #[test]
    fn click_outside_ignored() {
        let mut t = Toggle::new(false);
        t.layout(available());

        let resp = t.handle_event(&UiEvent::MouseDown {
            x: 0.0,
            y: 0.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Ignored));
        assert!(!t.is_on());
    }

    #[test]
    fn double_click_returns_to_off() {
        let mut t = Toggle::new(false);
        t.layout(available());

        let track = t.track_rect();
        t.handle_event(&UiEvent::MouseDown {
            x: track.x + 10.0,
            y: track.y + 5.0,
            button: MouseButton::Left,
        });
        assert!(t.is_on());

        t.handle_event(&UiEvent::MouseDown {
            x: track.x + 10.0,
            y: track.y + 5.0,
            button: MouseButton::Left,
        });
        assert!(!t.is_on());
    }

    #[test]
    fn set_on_programmatic() {
        let mut t = Toggle::new(false);
        t.set_on(true);
        assert!(t.is_on());
        t.set_on(false);
        assert!(!t.is_on());
    }
}
