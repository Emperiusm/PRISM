// SPDX-License-Identifier: AGPL-3.0-or-later
//! Connection info sub-panel — server address, encryption, session duration.

use crate::ui::widgets::{
    EventResponse, GlassQuad, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};
use crate::ui::widgets::label::Label;
use crate::ui::widgets::button::Button;

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct ConnPanel {
    server_label: Label,
    address_label: Label,
    encryption_label: Label,
    session_label: Label,
    client_id_label: Label,
    disconnect_button: Button,
    rect: Rect,
    visible: bool,
}

const PANEL_W: f32 = 260.0;
const PANEL_H: f32 = 200.0;

impl ConnPanel {
    pub fn new() -> Self {
        Self {
            server_label: Label::new("Server: —", 12.0),
            address_label: Label::new("Address: —", 12.0),
            encryption_label: Label::new("Encryption: —", 12.0),
            session_label: Label::new("Session: —", 12.0),
            client_id_label: Label::new("Client ID: —", 12.0),
            disconnect_button: Button::new("Disconnect", UiAction::Disconnect),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            visible: false,
        }
    }

    pub fn update(
        &mut self,
        server: &str,
        address: &str,
        encrypted: bool,
        session_duration: &str,
        client_id: &str,
    ) {
        self.server_label.set_text(&format!("Server: {server}"));
        self.address_label.set_text(&format!("Address: {address}"));
        self.encryption_label.set_text(if encrypted {
            "Encryption: Noise (E2E)"
        } else {
            "Encryption: None"
        });
        self.session_label.set_text(&format!("Session: {session_duration}"));
        self.client_id_label.set_text(&format!("Client ID: {client_id}"));
    }

    pub fn show(&mut self) { self.visible = true; }
    pub fn hide(&mut self) { self.visible = false; }
    pub fn is_visible(&self) -> bool { self.visible }

    fn layout_children(&mut self) {
        let pad = 8.0;
        let inner_w = PANEL_W - pad * 2.0;
        let x = self.rect.x + pad;
        let mut cur_y = self.rect.y + 28.0;

        self.server_label.layout(Rect::new(x, cur_y, inner_w, 16.0));
        cur_y += 20.0;
        self.address_label.layout(Rect::new(x, cur_y, inner_w, 16.0));
        cur_y += 20.0;
        self.encryption_label.layout(Rect::new(x, cur_y, inner_w, 16.0));
        cur_y += 20.0;
        self.session_label.layout(Rect::new(x, cur_y, inner_w, 16.0));
        cur_y += 20.0;
        self.client_id_label.layout(Rect::new(x, cur_y, inner_w, 16.0));
        cur_y += 24.0;
        self.disconnect_button.layout(Rect::new(x, cur_y, inner_w, 36.0));
    }
}

impl Default for ConnPanel {
    fn default() -> Self { Self::new() }
}

impl Widget for ConnPanel {
    fn layout(&mut self, available: Rect) -> Size {
        if !self.visible {
            return Size { w: 0.0, h: 0.0 };
        }
        self.rect = Rect::new(available.x, available.y, PANEL_W, PANEL_H);
        self.layout_children();
        Size { w: PANEL_W, h: PANEL_H }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        if !self.visible { return; }

        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.06, 0.0, 0.12, 0.25],
            border_color: [1.0, 1.0, 1.0, 0.15],
            corner_radius: 8.0,
            noise_intensity: 0.02,
        });

        ctx.push_text_run(TextRun {
            x: self.rect.x + 8.0,
            y: self.rect.y + 8.0,
            text: "Connection".into(),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });

        self.server_label.paint(ctx);
        self.address_label.paint(ctx);
        self.encryption_label.paint(ctx);
        self.session_label.paint(ctx);
        self.client_id_label.paint(ctx);
        self.disconnect_button.paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Only the disconnect button is interactive
        let resp = self.disconnect_button.handle_event(event);
        if !matches!(resp, EventResponse::Ignored) {
            return resp;
        }

        // Swallow clicks inside the panel
        match event {
            UiEvent::MouseDown { x, y, button: MouseButton::Left }
                if self.rect.contains(*x, *y) =>
            {
                EventResponse::Consumed
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.disconnect_button.animate(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conn_panel_paints() {
        let mut panel = ConnPanel::new();
        panel.show();
        panel.update("prism.local", "192.168.1.10:7272", true, "00:05:32", "c0ffee");
        panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        let texts: Vec<&str> = ctx.text_runs.iter().map(|t| t.text.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains("Connection")),
            "expected Connection title, got: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("prism.local")),
            "expected server name, got: {texts:?}"
        );
        assert!(!ctx.glass_quads.is_empty(), "expected glass quads");
    }

    #[test]
    fn conn_panel_disconnect() {
        let mut panel = ConnPanel::new();
        panel.show();
        panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));

        // Disconnect button is at rect.y + 28 + 4*20 + 24 = 28 + 80 + 24 = 132, h=36
        let btn_y = panel.rect.y + 28.0 + 4.0 * 20.0 + 24.0 + 18.0; // center of button
        let resp = panel.handle_event(&UiEvent::MouseDown {
            x: panel.rect.x + 60.0,
            y: btn_y,
            button: MouseButton::Left,
        });
        assert!(
            matches!(resp, EventResponse::Action(UiAction::Disconnect)),
            "expected Disconnect action, got {resp:?}"
        );
    }
}
