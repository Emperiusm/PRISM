// SPDX-License-Identifier: AGPL-3.0-or-later
//! Add/Edit server form overlay.

use crate::config::servers::SavedServer;
use crate::ui::widgets::button::Button;
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::text_input::TextInput;
use crate::ui::widgets::{
    EventResponse, GlassQuad, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

// ---------------------------------------------------------------------------
// ServerForm
// ---------------------------------------------------------------------------

pub struct ServerForm {
    name_input: TextInput,
    address_input: TextInput,
    noise_key_input: TextInput,
    profile_dropdown: Dropdown,
    save_button: Button,
    cancel_button: Button,
    editing_id: Option<uuid::Uuid>,
    rect: Rect,
    visible: bool,
}

impl ServerForm {
    pub fn new() -> Self {
        Self {
            name_input: TextInput::new("Server name"),
            address_input: TextInput::new("host:port"),
            noise_key_input: TextInput::new("Noise public key (optional)"),
            profile_dropdown: Dropdown::new(vec!["Gaming".into(), "Coding".into()], 0),
            save_button: Button::new("Save", UiAction::AddServer),
            cancel_button: Button::new("Cancel", UiAction::CloseOverlay),
            editing_id: None,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            visible: false,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Populate fields from an existing saved server for editing.
    pub fn set_editing(&mut self, server: &SavedServer) {
        self.editing_id = Some(server.id);
        self.name_input.set_text(&server.display_name);
        self.address_input.set_text(&server.address);
        if let Some(ref key) = server.noise_public_key {
            self.noise_key_input.set_text(key);
        } else {
            self.noise_key_input.set_text("");
        }
        // Set profile dropdown: find matching profile
        let profile_options = ["Gaming", "Coding"];
        if let Some(idx) = profile_options
            .iter()
            .position(|&p| p == server.default_profile)
        {
            self.profile_dropdown.set_selected(idx);
        }
    }

    /// Reset all fields to defaults.
    pub fn clear(&mut self) {
        self.editing_id = None;
        self.name_input.set_text("");
        self.address_input.set_text("");
        self.noise_key_input.set_text("");
        self.profile_dropdown.set_selected(0);
    }

    /// Returns the current form data as a (name, address, noise_key, profile) tuple.
    pub fn form_data(&self) -> (String, String, Option<String>, String) {
        let name = self.name_input.text().to_string();
        let address = self.address_input.text().to_string();
        let noise_key = {
            let t = self.noise_key_input.text();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let profile = self.profile_dropdown.selected_text().to_string();
        (name, address, noise_key, profile)
    }
}

impl Default for ServerForm {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ServerForm {
    fn layout(&mut self, available: Rect) -> Size {
        if !self.visible {
            return Size { w: 0.0, h: 0.0 };
        }

        let panel_w = 300.0;
        let panel_h = 280.0;
        self.rect = Rect::new(available.x, available.y, panel_w, panel_h);

        let x = available.x + 12.0;
        let w = panel_w - 24.0;

        // Name input at y+50
        self.name_input
            .layout(Rect::new(x, available.y + 50.0, w, 36.0));
        // Address at y+95
        self.address_input
            .layout(Rect::new(x, available.y + 95.0, w, 36.0));
        // Noise key at y+140
        self.noise_key_input
            .layout(Rect::new(x, available.y + 140.0, w, 36.0));
        // Profile dropdown at y+185
        self.profile_dropdown
            .layout(Rect::new(x, available.y + 185.0, w, 32.0));
        // Save button at y+230
        self.save_button
            .layout(Rect::new(x, available.y + 230.0, (w / 2.0) - 4.0, 36.0));
        // Cancel beside save
        self.cancel_button.layout(Rect::new(
            x + (w / 2.0) + 4.0,
            available.y + 230.0,
            (w / 2.0) - 4.0,
            36.0,
        ));

        Size {
            w: panel_w,
            h: panel_h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        if !self.visible {
            return;
        }

        // Glass panel background
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.08, 0.0, 0.15, 0.20],
            border_color: [1.0, 1.0, 1.0, 0.2],
            corner_radius: 12.0,
            noise_intensity: 0.03,
        });

        // Title
        let title = if self.editing_id.is_some() {
            "Edit Server"
        } else {
            "Add Server"
        };
        ctx.push_text_run(TextRun {
            x: self.rect.x + 12.0,
            y: self.rect.y + 16.0,
            text: title.to_string(),
            font_size: 16.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });

        self.name_input.paint(ctx);
        self.address_input.paint(ctx);
        self.noise_key_input.paint(ctx);
        self.profile_dropdown.paint(ctx);
        self.save_button.paint(ctx);
        self.cancel_button.paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        if !self.visible {
            return EventResponse::Ignored;
        }

        // Cancel button → hide
        let cancel_resp = self.cancel_button.handle_event(event);
        if matches!(cancel_resp, EventResponse::Action(UiAction::CloseOverlay)) {
            self.visible = false;
            return EventResponse::Consumed;
        }

        // Save button → signal parent
        let save_resp = self.save_button.handle_event(event);
        if matches!(save_resp, EventResponse::Action(_)) {
            return EventResponse::Consumed;
        }

        // Remaining sub-widgets
        let resp = self.name_input.handle_event(event);
        if !matches!(resp, EventResponse::Ignored) {
            return resp;
        }

        let resp = self.address_input.handle_event(event);
        if !matches!(resp, EventResponse::Ignored) {
            return resp;
        }

        let resp = self.noise_key_input.handle_event(event);
        if !matches!(resp, EventResponse::Ignored) {
            return resp;
        }

        let resp = self.profile_dropdown.handle_event(event);
        if !matches!(resp, EventResponse::Ignored) {
            return resp;
        }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        self.name_input.animate(dt_ms);
        self.address_input.animate(dt_ms);
        self.noise_key_input.animate(dt_ms);
        self.profile_dropdown.animate(dt_ms);
        self.save_button.animate(dt_ms);
        self.cancel_button.animate(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 600.0, 400.0)
    }

    #[test]
    fn form_hidden_by_default() {
        let mut form = ServerForm::new();
        let size = form.layout(available());
        assert_eq!(size.h, 0.0, "hidden form should report height 0");
    }

    #[test]
    fn form_visible_paints() {
        let mut form = ServerForm::new();
        form.show();
        form.layout(available());

        let mut ctx = PaintContext::new();
        form.paint(&mut ctx);

        assert!(
            ctx.glass_quads.len() > 0,
            "expected glass quads when form is visible"
        );
    }
}
