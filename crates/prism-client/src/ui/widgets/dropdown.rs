// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drop-down selection widget.

use crate::renderer::animation::{Animation, EaseCurve};
use super::{
    EventResponse, GlassQuad, MouseButton, PaintContext, Rect, Size, TextRun, UiEvent, Widget,
};

pub struct Dropdown {
    options: Vec<String>,
    selected: usize,
    open: bool,
    open_anim: Animation,
    rect: Rect,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        Self {
            options,
            selected,
            open: false,
            open_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_text(&self) -> &str {
        self.options.get(self.selected).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn set_selected(&mut self, index: usize) {
        if index < self.options.len() {
            self.selected = index;
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn item_rect(&self, index: usize) -> Rect {
        Rect::new(
            self.rect.x,
            self.rect.y + self.rect.h + index as f32 * 28.0,
            self.rect.w,
            28.0,
        )
    }
}

impl Widget for Dropdown {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 32.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Closed state header
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.05, 0.0, 0.1, 0.15],
            border_color: [1.0, 1.0, 1.0, 0.2],
            corner_radius: 6.0,
            noise_intensity: 0.0,
        });

        let label = format!("{} \u{25be}", self.selected_text());
        ctx.push_text_run(TextRun {
            x: self.rect.x + 10.0,
            y: self.rect.y + 9.0,
            text: label,
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.9],
            monospace: false,
        });

        // Open dropdown items
        if self.open_anim.value() > 0.01 {
            for (i, option) in self.options.iter().enumerate() {
                let item_r = self.item_rect(i);
                let is_selected = i == self.selected;
                let tint = if is_selected {
                    [0.55, 0.36, 0.96, 0.25]
                } else {
                    [0.05, 0.0, 0.1, 0.15]
                };

                ctx.push_glass_quad(GlassQuad {
                    rect: item_r,
                    blur_rect: item_r,
                    tint,
                    border_color: [1.0, 1.0, 1.0, 0.15],
                    corner_radius: 4.0,
                    noise_intensity: 0.0,
                });

                ctx.push_text_run(TextRun {
                    x: item_r.x + 10.0,
                    y: item_r.y + 7.0,
                    text: option.clone(),
                    font_size: 13.0,
                    color: [1.0, 1.0, 1.0, 0.9],
                    monospace: false,
                });
            }
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                if self.rect.contains(*x, *y) {
                    self.open = !self.open;
                    self.open_anim.set_target(if self.open { 1.0 } else { 0.0 });
                    return EventResponse::Consumed;
                }

                if self.open {
                    for i in 0..self.options.len() {
                        if self.item_rect(i).contains(*x, *y) {
                            self.selected = i;
                            self.open = false;
                            self.open_anim.set_target(0.0);
                            return EventResponse::Consumed;
                        }
                    }
                    // Click outside while open → close
                    self.open = false;
                    self.open_anim.set_target(0.0);
                    return EventResponse::Consumed;
                }

                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.open_anim.tick(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dropdown() -> Dropdown {
        Dropdown::new(vec!["Option A".into(), "Option B".into(), "Option C".into()], 0)
    }

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 200.0, 200.0)
    }

    #[test]
    fn default_selection() {
        let dd = make_dropdown();
        assert_eq!(dd.selected_index(), 0);
        assert_eq!(dd.selected_text(), "Option A");
    }

    #[test]
    fn click_opens_and_closes() {
        let mut dd = make_dropdown();
        dd.layout(available());

        // Click header to open
        dd.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 16.0,
            button: MouseButton::Left,
        });
        assert!(dd.is_open(), "dropdown should be open after click");

        // Click header again to close
        dd.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 16.0,
            button: MouseButton::Left,
        });
        assert!(!dd.is_open(), "dropdown should be closed after second click");
    }

    #[test]
    fn select_option() {
        let mut dd = make_dropdown();
        dd.layout(available());

        // Open
        dd.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 16.0,
            button: MouseButton::Left,
        });
        assert!(dd.is_open());

        // item_rect(1) = y: rect.y + rect.h + 1*28 = 0 + 32 + 28 = 60, h=28 → center y=74
        dd.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 74.0,
            button: MouseButton::Left,
        });

        assert_eq!(dd.selected_index(), 1, "expected Option B selected");
        assert!(!dd.is_open(), "dropdown should close after selection");
    }
}
