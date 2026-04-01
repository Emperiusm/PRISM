// SPDX-License-Identifier: AGPL-3.0-or-later
//! Minimal settings panel showing identity path and version.

use crate::ui::widgets::{
    EventResponse, GlassQuad, PaintContext, Rect, Size, TextRun, UiEvent, Widget,
};

// ---------------------------------------------------------------------------
// SettingsPanel
// ---------------------------------------------------------------------------

pub struct SettingsPanel {
    rect: Rect,
    visible: bool,
    identity_path: String,
    version: String,
}

impl SettingsPanel {
    pub fn new(identity_path: String, version: String) -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            visible: false,
            identity_path,
            version,
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
}

impl Widget for SettingsPanel {
    fn layout(&mut self, available: Rect) -> Size {
        if !self.visible {
            return Size { w: 0.0, h: 0.0 };
        }

        let panel_w = 300.0;
        let panel_h = 200.0;
        self.rect = Rect::new(available.x, available.y, panel_w, panel_h);
        Size { w: panel_w, h: panel_h }
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
        ctx.push_text_run(TextRun {
            x: self.rect.x + 12.0,
            y: self.rect.y + 16.0,
            text: "Settings".to_string(),
            font_size: 16.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });

        // Identity path label
        ctx.push_text_run(TextRun {
            x: self.rect.x + 12.0,
            y: self.rect.y + 60.0,
            text: format!("Identity: {}", self.identity_path),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.75],
            monospace: true,
        });

        // Version label
        ctx.push_text_run(TextRun {
            x: self.rect.x + 12.0,
            y: self.rect.y + 90.0,
            text: format!("Version: {}", self.version),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.75],
            monospace: false,
        });
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
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
        Rect::new(0.0, 0.0, 600.0, 400.0)
    }

    #[test]
    fn settings_hidden_by_default() {
        let mut panel = SettingsPanel::new(
            "/home/user/.prism/identity".to_string(),
            "0.1.0".to_string(),
        );
        let size = panel.layout(available());
        assert_eq!(size.h, 0.0, "settings panel should be hidden by default");
    }

    #[test]
    fn settings_visible_paints() {
        let mut panel = SettingsPanel::new(
            "/home/user/.prism/identity".to_string(),
            "0.1.0".to_string(),
        );
        panel.show();
        panel.layout(available());

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        assert!(ctx.glass_quads.len() > 0, "expected glass quads when panel is visible");
        // Should have title + 2 labels
        assert!(ctx.text_runs.len() >= 3, "expected at least 3 text runs");
    }
}
