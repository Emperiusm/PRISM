// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hero quick-connect bar: address input + connect button in a glass panel.

use crate::ui::theme;
use crate::ui::widgets::button::{Button, ButtonStyle};
use crate::ui::widgets::text_input::TextInput;
use crate::ui::widgets::{
    EventResponse, KeyCode, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

// ---------------------------------------------------------------------------
// QuickConnect
// ---------------------------------------------------------------------------

pub struct QuickConnect {
    address_input: TextInput,
    connect_button: Button,
    rect: Rect,
}

impl QuickConnect {
    pub fn new() -> Self {
        Self {
            address_input: TextInput::new("Enter a host or IP address"),
            connect_button: Button::new(
                "Connect",
                UiAction::Connect {
                    address: String::new(),
                    noise_key: None,
                },
            )
            .with_style(ButtonStyle::Primary),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl Default for QuickConnect {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for QuickConnect {
    fn layout(&mut self, available: Rect) -> Size {
        // Hero container sizes
        let panel_h = 260.0;
        self.rect = Rect::new(available.x, available.y, available.w, panel_h);

        let pad_x = 32.0;
        let content_w = available.w - (pad_x * 2.0);
        let input_y = available.y + 128.0;
        let btn_y = input_y + 54.0;

        self.address_input.layout(Rect::new(
            available.x + pad_x,
            input_y,
            content_w,
            42.0,
        ));

        self.connect_button.layout(Rect::new(
            available.x + pad_x,
            btn_y,
            content_w,
            42.0,
        ));

        Size {
            w: available.w,
            h: panel_h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        ctx.push_glass_quad(theme::hero_surface(self.rect));
        
        // Match Stitch visual intent with FONT_HERO heading centered
        let title = "Quick Connect";
        let title_w = theme::text_width(title, theme::FONT_HERO);
        ctx.push_text_run(TextRun {
            x: self.rect.x + (self.rect.w - title_w) * 0.5,
            y: self.rect.y + 40.0,
            text: title.into(),
            font_size: theme::FONT_HERO,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });

        let subtitle = "Enter a hostname or IP address";
        let sub_w = theme::text_width(subtitle, theme::FONT_BODY);
        ctx.push_text_run(TextRun {
            x: self.rect.x + (self.rect.w - sub_w) * 0.5,
            y: self.rect.y + 80.0,
            text: subtitle.into(),
            font_size: theme::FONT_BODY,
            color: theme::TEXT_MUTED,
            monospace: false,
        });

        self.address_input.paint(ctx);
        self.connect_button.paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Enter key when input is focused and non-empty → Connect action
        if let UiEvent::KeyDown {
            key: KeyCode::Enter,
        } = event
            && self.address_input.is_focused()
            && !self.address_input.text().is_empty()
        {
            return EventResponse::Action(UiAction::Connect {
                address: self.address_input.text().to_string(),
                noise_key: None,
            });
        }

        // Delegate to address input first
        let resp = self.address_input.handle_event(event);
        if !matches!(resp, EventResponse::Ignored) {
            return resp;
        }

        // Delegate to connect button; if it fires, override address with current input text
        let resp = self.connect_button.handle_event(event);
        if let EventResponse::Action(UiAction::Connect { noise_key, .. }) = &resp {
            let address = self.address_input.text().to_string();
            let noise_key = noise_key.clone();
            return EventResponse::Action(UiAction::Connect { address, noise_key });
        }

        resp
    }

    fn animate(&mut self, dt_ms: f32) {
        self.address_input.animate(dt_ms);
        self.connect_button.animate(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 800.0, 80.0)
    }

    #[test]
    fn quick_connect_enter_triggers() {
        let mut qc = QuickConnect::new();
        qc.layout(available());

        qc.address_input.set_text("10.0.0.5:7000");
        qc.address_input.set_focused(true);

        let resp = qc.handle_event(&UiEvent::KeyDown {
            key: KeyCode::Enter,
        });

        match resp {
            EventResponse::Action(UiAction::Connect { address, .. }) => {
                assert_eq!(address, "10.0.0.5:7000");
            }
            other => panic!("expected Connect action, got {:?}", other),
        }
    }

    #[test]
    fn quick_connect_paints_panel() {
        let mut qc = QuickConnect::new();
        qc.layout(available());

        let mut ctx = PaintContext::new();
        qc.paint(&mut ctx);

        // At least 1 glass_quad for the panel background (plus sub-widget quads)
        assert!(ctx.glass_quads.len() >= 1, "expected at least 1 glass quad");
        // Sub-widgets contribute text runs
        assert!(!ctx.text_runs.is_empty(), "expected text from sub-widgets");
    }
}
