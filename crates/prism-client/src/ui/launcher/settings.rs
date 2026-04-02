// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher settings page.

use crate::ui::theme;
use crate::ui::widgets::{EventResponse, PaintContext, Rect, Size, TextRun, UiEvent, Widget};

pub struct SettingsPanel {
    rect: Rect,
    identity_path: String,
    version: String,
}

impl SettingsPanel {
    pub fn new(identity_path: String, version: String) -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            identity_path,
            version,
        }
    }
}

impl Widget for SettingsPanel {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let section = Rect::new(self.rect.x, self.rect.y + 58.0, self.rect.w, self.rect.h - 58.0);
        ctx.push_glass_quad(theme::floating_surface(section));

        ctx.push_text_run(TextRun {
            x: self.rect.x,
            y: self.rect.y + 10.0,
            text: "Client Settings".into(),
            font_size: 13.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });

        let rows = [
            (
                "Identity Path",
                self.identity_path.as_str(),
                "Used to load the local client identity and device trust information.",
            ),
            (
                "Streaming Default",
                "Balanced",
                "Applied when a saved desktop does not have a more specific profile.",
            ),
            (
                "Input Capture",
                "Exclusive keyboard capture",
                "Prevents local shortcuts from leaking when a remote session is active.",
            ),
            (
                "Audio Route",
                "Internal audio engine",
                "Plays remote audio locally and keeps microphone passthrough available.",
            ),
            (
                "Client Version",
                self.version.as_str(),
                "Current PRISM client build.",
            ),
        ];

        for (index, (title, value, body)) in rows.iter().enumerate() {
            let row = Rect::new(
                section.x + 18.0,
                section.y + 20.0 + index as f32 * 88.0,
                section.w - 36.0,
                70.0,
            );
            ctx.push_glass_quad(theme::card_surface(row));
            ctx.push_text_run(TextRun {
                x: row.x + 18.0,
                y: row.y + 14.0,
                text: (*title).to_string(),
                font_size: 13.0,
                color: theme::TEXT_MUTED,
                monospace: false,
            });
            ctx.push_text_run(TextRun {
                x: row.x + 18.0,
                y: row.y + 34.0,
                text: (*value).to_string(),
                font_size: 14.0,
                color: theme::TEXT_PRIMARY,
                monospace: false,
            });
            ctx.push_text_run(TextRun {
                x: row.x + 18.0,
                y: row.y + 54.0,
                text: (*body).to_string(),
                font_size: 11.0,
                color: theme::TEXT_SECONDARY,
                monospace: false,
            });
        }
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_panel_paints_rows() {
        let mut panel = SettingsPanel::new(
            "/home/user/.prism/client_identity.json".to_string(),
            "0.1.0".to_string(),
        );
        panel.layout(Rect::new(0.0, 0.0, 900.0, 520.0));

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        assert!(ctx.glass_quads.len() >= 6);
        assert!(ctx.text_runs.len() >= 10);
    }
}
