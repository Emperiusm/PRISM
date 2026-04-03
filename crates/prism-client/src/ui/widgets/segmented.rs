// SPDX-License-Identifier: AGPL-3.0-or-later
//! Segmented control widget for mutually exclusive linear options.

use super::{EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiEvent, Widget};
use crate::ui::theme;

pub struct SegmentedControl {
    options: Vec<String>,
    selected_index: usize,
    hovered_index: Option<usize>,
    rect: Rect,
}

impl SegmentedControl {
    pub fn new(options: Vec<String>, selected_index: usize) -> Self {
        Self {
            options,
            selected_index,
            hovered_index: None,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn set_selected(&mut self, index: usize) {
        self.selected_index = index;
    }

    fn index_rect(&self, idx: usize) -> Rect {
        let w = self.rect.w / self.options.len() as f32;
        Rect::new(self.rect.x + (idx as f32) * w, self.rect.y, w, self.rect.h)
    }
}

impl Widget for SegmentedControl {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Base track
        ctx.push_glass_quad(theme::glass_quad(
            self.rect,
            [1.0, 1.0, 1.0, 0.04],
            [1.0, 1.0, 1.0, 0.1],
            theme::CARD_RADIUS, // Assuming CARD_RADIUS makes a nice pill or rounded rect
        ));

        for (idx, label) in self.options.iter().enumerate() {
            let r = self.index_rect(idx);
            let selected = idx == self.selected_index;
            let hovered = Some(idx) == self.hovered_index;

            if selected {
                ctx.push_glass_quad(theme::glass_quad(
                    r,
                    [theme::ACCENT[0], theme::ACCENT[1], theme::ACCENT[2], 0.9],
                    [theme::ACCENT[0], theme::ACCENT[1], theme::ACCENT[2], 1.0],
                    theme::CARD_RADIUS,
                ));
            } else if hovered {
                ctx.push_glass_quad(theme::glass_quad(
                    r,
                    [1.0, 1.0, 1.0, 0.08],
                    [1.0, 1.0, 1.0, 0.0],
                    theme::CARD_RADIUS,
                ));
            }

            let text_color = if selected {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                theme::TEXT_SECONDARY
            };

            let tw = theme::text_width(label, 12.0);
            ctx.push_text_run(TextRun {
                x: r.x + (r.w - tw) * 0.5,
                y: r.y + (r.h - 12.0) * 0.5 + 1.0,
                text: label.clone(),
                font_size: 12.0,
                color: text_color,
                monospace: false,
            });
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                if !self.rect.contains(*x, *y) {
                    self.hovered_index = None;
                    return EventResponse::Ignored;
                }
                for idx in 0..self.options.len() {
                    if self.index_rect(idx).contains(*x, *y) {
                        self.hovered_index = Some(idx);
                        return EventResponse::Ignored;
                    }
                }
                self.hovered_index = None;
                EventResponse::Ignored
            }
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                if !self.rect.contains(*x, *y) {
                    return EventResponse::Ignored;
                }
                for idx in 0..self.options.len() {
                    if self.index_rect(idx).contains(*x, *y) {
                        if self.selected_index != idx {
                            self.selected_index = idx;
                            return EventResponse::Consumed;
                        } else {
                            return EventResponse::Ignored;
                        }
                    }
                }
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_changes_selection() {
        let mut seg = SegmentedControl::new(vec!["A".into(), "B".into()], 0);
        seg.layout(Rect::new(0.0, 0.0, 100.0, 30.0));
        
        let resp = seg.handle_event(&UiEvent::MouseDown {
            x: 75.0,
            y: 15.0,
            button: MouseButton::Left,
        });

        assert!(matches!(resp, EventResponse::Consumed));
        assert_eq!(seg.selected_index(), 1);
    }
}
