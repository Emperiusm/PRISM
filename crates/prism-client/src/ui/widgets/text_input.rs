// SPDX-License-Identifier: AGPL-3.0-or-later
//! Single-line text input widget.

use super::{
    ColorMode, EventResponse, KeyCode, MouseButton, PaintContext, Rect, Size, TextRun, UiEvent,
    Widget,
};
use crate::ui::theme;

pub struct TextInput {
    text: String,
    placeholder: String,
    cursor: usize,
    focused: bool,
    rect: Rect,
    autocomplete_candidates: Vec<String>,
    color_mode: ColorMode,
}

impl TextInput {
    pub fn new(placeholder: &str) -> Self {
        Self {
            text: String::new(),
            placeholder: placeholder.to_owned(),
            cursor: 0,
            focused: false,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            autocomplete_candidates: Vec::new(),
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

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_owned();
        self.cursor = self.text.len();
    }

    pub fn set_autocomplete(&mut self, candidates: Vec<String>) {
        self.autocomplete_candidates = candidates;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl Widget for TextInput {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 42.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        match self.color_mode {
            ColorMode::Light => {
                let mut quad = theme::launcher_control_surface(self.rect, self.focused);
                quad.corner_radius = 8.0;
                ctx.push_glass_quad(quad);
            }
            ColorMode::Dark => {
                ctx.push_glass_quad(theme::control_surface(self.rect, self.focused));
            }
        }

        let (display_text, color) = if self.text.is_empty() {
            let placeholder_color = match self.color_mode {
                ColorMode::Light => theme::LT_TEXT_MUTED,
                ColorMode::Dark => theme::TEXT_TERTIARY,
            };
            (self.placeholder.clone(), placeholder_color)
        } else {
            let text_color = match self.color_mode {
                ColorMode::Light => theme::LT_TEXT_PRIMARY,
                ColorMode::Dark => theme::TEXT_PRIMARY,
            };
            (self.text.clone(), text_color)
        };

        ctx.push_text_run(TextRun {
            x: self.rect.x + 14.0,
            y: self.rect.y + (self.rect.h - 14.0) * 0.5 - 1.0,
            text: display_text,
            font_size: 14.0,
            color,
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
                self.focused = self.rect.contains(*x, *y);
                if self.focused {
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::TextInput { ch } if self.focused => {
                // Insert char at cursor position (byte-safe for ASCII; cursor tracks chars)
                let byte_pos = self
                    .text
                    .char_indices()
                    .nth(self.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.text.len());
                self.text.insert(byte_pos, *ch);
                self.cursor += 1;
                EventResponse::Consumed
            }
            UiEvent::KeyDown { key } if self.focused => match key {
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        let byte_pos = self
                            .text
                            .char_indices()
                            .nth(self.cursor - 1)
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        self.text.remove(byte_pos);
                        self.cursor -= 1;
                    }
                    EventResponse::Consumed
                }
                KeyCode::Delete => {
                    let char_count = self.text.chars().count();
                    if self.cursor < char_count {
                        let byte_pos = self
                            .text
                            .char_indices()
                            .nth(self.cursor)
                            .map(|(i, _)| i)
                            .unwrap_or(self.text.len());
                        self.text.remove(byte_pos);
                    }
                    EventResponse::Consumed
                }
                KeyCode::Left => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                    }
                    EventResponse::Consumed
                }
                KeyCode::Right => {
                    let char_count = self.text.chars().count();
                    if self.cursor < char_count {
                        self.cursor += 1;
                    }
                    EventResponse::Consumed
                }
                KeyCode::Home => {
                    self.cursor = 0;
                    EventResponse::Consumed
                }
                KeyCode::End => {
                    self.cursor = self.text.chars().count();
                    EventResponse::Consumed
                }
                _ => EventResponse::Ignored,
            },
            _ => EventResponse::Ignored,
        }
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
        Rect::new(0.0, 0.0, 200.0, 100.0)
    }

    #[test]
    fn typing_appends_text() {
        let mut input = TextInput::new("Enter value");
        input.layout(available());
        input.set_focused(true);

        input.handle_event(&UiEvent::TextInput { ch: '1' });
        input.handle_event(&UiEvent::TextInput { ch: '9' });
        input.handle_event(&UiEvent::TextInput { ch: '2' });

        assert_eq!(input.text(), "192");
    }

    #[test]
    fn backspace_deletes() {
        let mut input = TextInput::new("");
        input.layout(available());
        input.set_text("abc");
        input.set_focused(true);

        input.handle_event(&UiEvent::KeyDown {
            key: KeyCode::Backspace,
        });

        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn unfocused_ignores_input() {
        let mut input = TextInput::new("");
        input.layout(available());
        // Not focused by default

        let resp = input.handle_event(&UiEvent::TextInput { ch: 'x' });
        assert!(matches!(resp, EventResponse::Ignored));
        assert_eq!(input.text(), "");
    }

    #[test]
    fn click_focuses() {
        let mut input = TextInput::new("");
        input.layout(available());

        let resp = input.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 18.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Consumed));
        assert!(input.is_focused());
    }

    #[test]
    fn click_outside_unfocuses() {
        let mut input = TextInput::new("");
        input.layout(available());
        input.set_focused(true);

        input.handle_event(&UiEvent::MouseDown {
            x: 500.0,
            y: 500.0,
            button: MouseButton::Left,
        });
        assert!(!input.is_focused());
    }
}
