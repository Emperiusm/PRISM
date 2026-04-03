// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drop-down selection widget.

use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::{
    ColorMode, EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiEvent, Widget,
};

pub struct Dropdown {
    options: Vec<String>,
    selected: usize,
    open: bool,
    open_anim: Animation,
    rect: Rect,
    color_mode: ColorMode,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        Self {
            options,
            selected,
            open: false,
            open_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            color_mode: ColorMode::Dark,
        }
    }

    pub fn with_color_mode(mut self, mode: ColorMode) -> Self {
        self.color_mode = mode;
        self
    }

    pub fn set_color_mode(&mut self, mode: ColorMode) {
        self.color_mode = mode;
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_text(&self) -> &str {
        self.options
            .get(self.selected)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn set_selected(&mut self, index: usize) {
        if index < self.options.len() {
            self.selected = index;
        }
    }

    pub fn set_selected_by_text(&mut self, text: &str) -> bool {
        if let Some(index) = self.options.iter().position(|option| option == text) {
            self.selected = index;
            true
        } else {
            false
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
        let h = 40.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Closed state header
        match self.color_mode {
            ColorMode::Light => {
                ctx.push_glass_quad(theme::launcher_control_surface(self.rect, self.open));
            }
            ColorMode::Dark => {
                ctx.push_glass_quad(theme::control_surface(self.rect, self.open));
            }
        }

        let header_text_color = match self.color_mode {
            ColorMode::Light => theme::LT_TEXT_PRIMARY,
            ColorMode::Dark => theme::TEXT_PRIMARY,
        };

        let label = format!("{} v", self.selected_text());
        ctx.push_text_run(TextRun {
            x: self.rect.x + 14.0,
            y: self.rect.y + (self.rect.h - 13.0) * 0.5 - 1.0,
            text: label,
            font_size: 13.0,
            color: header_text_color,
            ..Default::default()
        });

        // Open dropdown items
        if self.open_anim.value() > 0.01 {
            for (i, option) in self.options.iter().enumerate() {
                let item_r = self.item_rect(i);
                let is_selected = i == self.selected;

                let (item_tint, item_border) = match self.color_mode {
                    ColorMode::Light => {
                        if is_selected {
                            (
                                [1.0, 1.0, 1.0, 0.95],
                                [
                                    theme::PRIMARY_BLUE[0],
                                    theme::PRIMARY_BLUE[1],
                                    theme::PRIMARY_BLUE[2],
                                    0.40,
                                ],
                            )
                        } else {
                            ([1.0, 1.0, 1.0, 0.90], [0.831, 0.843, 0.863, 0.60])
                        }
                    }
                    ColorMode::Dark => {
                        if is_selected {
                            (
                                [0.22, 0.30, 0.39, 0.96],
                                [theme::ACCENT[0], theme::ACCENT[1], theme::ACCENT[2], 0.32],
                            )
                        } else {
                            ([0.14, 0.18, 0.24, 0.92], [1.0, 1.0, 1.0, 0.10])
                        }
                    }
                };

                ctx.push_glass_quad(theme::glass_quad(
                    item_r,
                    item_tint,
                    item_border,
                    theme::CHIP_RADIUS,
                ));

                let item_text_color = match self.color_mode {
                    ColorMode::Light => {
                        if is_selected {
                            theme::LT_TEXT_PRIMARY
                        } else {
                            theme::LT_TEXT_SECONDARY
                        }
                    }
                    ColorMode::Dark => {
                        if is_selected {
                            theme::TEXT_PRIMARY
                        } else {
                            theme::TEXT_SECONDARY
                        }
                    }
                };

                ctx.push_text_run(TextRun {
                    x: item_r.x + 14.0,
                    y: item_r.y + 7.0,
                    text: option.clone(),
                    font_size: 13.0,
                    color: item_text_color,
                    ..Default::default()
                });
            }
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
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
        Dropdown::new(
            vec!["Option A".into(), "Option B".into(), "Option C".into()],
            0,
        )
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
        assert!(
            !dd.is_open(),
            "dropdown should be closed after second click"
        );
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
            y: 82.0,
            button: MouseButton::Left,
        });

        assert_eq!(dd.selected_index(), 1, "expected Option B selected");
        assert!(!dd.is_open(), "dropdown should close after selection");
    }
}
