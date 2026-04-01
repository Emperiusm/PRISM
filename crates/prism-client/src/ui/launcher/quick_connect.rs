// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hero quick-connect bar: address input + connect button in a glass panel.

use crate::ui::widgets::{
    EventResponse, GlassQuad, KeyCode, PaintContext, Rect, Size, UiAction, UiEvent, Widget,
};
use crate::ui::widgets::text_input::TextInput;
use crate::ui::widgets::button::Button;

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
            address_input: TextInput::new("Enter server address..."),
            connect_button: Button::new(
                "Connect",
                UiAction::Connect { address: String::new(), noise_key: None },
            ),
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
        let panel_h = 60.0;
        self.rect = Rect::new(available.x, available.y, available.w, panel_h);

        // Address input: x+12, y+12, w-120, 36
        self.address_input.layout(Rect::new(
            available.x + 12.0,
            available.y + 12.0,
            available.w - 120.0,
            36.0,
        ));

        // Connect button: x+w-100, y+12, 88, 36
        self.connect_button.layout(Rect::new(
            available.x + available.w - 100.0,
            available.y + 12.0,
            88.0,
            36.0,
        ));

        Size { w: available.w, h: panel_h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Glass panel background
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.08, 0.0, 0.15, 0.15],
            border_color: [1.0, 1.0, 1.0, 0.2],
            corner_radius: 10.0,
            noise_intensity: 0.03,
        });

        self.address_input.paint(ctx);
        self.connect_button.paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Enter key when input is focused and non-empty → Connect action
        if let UiEvent::KeyDown { key: KeyCode::Enter } = event {
            if self.address_input.is_focused() && !self.address_input.text().is_empty() {
                return EventResponse::Action(UiAction::Connect {
                    address: self.address_input.text().to_string(),
                    noise_key: None,
                });
            }
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

        let resp = qc.handle_event(&UiEvent::KeyDown { key: KeyCode::Enter });

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
