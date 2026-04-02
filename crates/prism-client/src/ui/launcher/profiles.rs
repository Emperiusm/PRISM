// SPDX-License-Identifier: AGPL-3.0-or-later
//! First-pass profiles editor for launcher mode.

use crate::ui::theme;
use crate::ui::widgets::{EventResponse, PaintContext, Rect, Size, TextRun, UiEvent, Widget};

pub struct ProfilesPanel {
    rect: Rect,
}

impl ProfilesPanel {
    pub fn new() -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    fn list_rect(&self) -> Rect {
        Rect::new(self.rect.x, self.rect.y + 58.0, 248.0, self.rect.h - 58.0)
    }

    fn editor_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + 280.0,
            self.rect.y,
            (self.rect.w - 280.0).max(0.0),
            self.rect.h,
        )
    }
}

impl Default for ProfilesPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ProfilesPanel {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let list = self.list_rect();
        let editor = self.editor_rect();
        ctx.push_glass_quad(theme::floating_surface(list));
        ctx.push_glass_quad(theme::floating_surface(editor));

        ctx.push_text_run(TextRun {
            x: list.x + 18.0,
            y: list.y + 16.0,
            text: "Presets".into(),
            font_size: 12.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });

        let presets = [
            ("Gaming", "Low latency, high refresh", true),
            ("Coding", "Sharper text and balanced fidelity", false),
            ("Balanced", "Adaptive quality for general use", false),
            ("Low Bandwidth", "Stays responsive on weaker links", false),
        ];

        for (index, (title, body, active)) in presets.iter().enumerate() {
            let row = Rect::new(list.x + 12.0, list.y + 42.0 + index as f32 * 62.0, list.w - 24.0, 54.0);
            ctx.push_glass_quad(theme::nav_item_surface(row, *active, false));
            ctx.push_text_run(TextRun {
                x: row.x + 14.0,
                y: row.y + 12.0,
                text: (*title).to_string(),
                font_size: 14.0,
                color: theme::TEXT_PRIMARY,
                monospace: false,
            });
            ctx.push_text_run(TextRun {
                x: row.x + 14.0,
                y: row.y + 32.0,
                text: (*body).to_string(),
                font_size: 11.0,
                color: theme::TEXT_MUTED,
                monospace: false,
            });
        }

        ctx.push_text_run(TextRun {
            x: editor.x + 24.0,
            y: editor.y + 24.0,
            text: "Gaming".into(),
            font_size: 24.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: editor.x + 24.0,
            y: editor.y + 58.0,
            text: "Optimized for high-refresh, low-latency desktop interaction.".into(),
            font_size: 13.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        let bitrate = Rect::new(editor.x + 24.0, editor.y + 108.0, editor.w - 48.0, 84.0);
        let display = Rect::new(editor.x + 24.0, editor.y + 210.0, editor.w - 48.0, 110.0);
        let connectivity = Rect::new(editor.x + 24.0, editor.y + 338.0, editor.w - 48.0, 122.0);

        for rect in [bitrate, display, connectivity] {
            ctx.push_glass_quad(theme::card_surface(rect));
        }

        ctx.push_text_run(TextRun {
            x: bitrate.x + 18.0,
            y: bitrate.y + 16.0,
            text: "Performance".into(),
            font_size: 12.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: bitrate.x + 18.0,
            y: bitrate.y + 40.0,
            text: "Bitrate Preference".into(),
            font_size: 14.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: bitrate.x + bitrate.w - 76.0,
            y: bitrate.y + 38.0,
            text: "35 Mbps".into(),
            font_size: 14.0,
            color: theme::accent(0.92),
            monospace: false,
        });
        ctx.push_glass_quad(theme::control_surface(
            Rect::new(bitrate.x + 18.0, bitrate.y + 58.0, bitrate.w - 36.0, 14.0),
            true,
        ));

        ctx.push_text_run(TextRun {
            x: display.x + 18.0,
            y: display.y + 16.0,
            text: "Display and Audio".into(),
            font_size: 12.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        for (index, label) in ["Native scaling", "Stereo audio", "AV1 preferred"]
            .iter()
            .enumerate()
        {
            let control = Rect::new(display.x + 18.0, display.y + 42.0 + index as f32 * 28.0, display.w - 36.0, 24.0);
            ctx.push_glass_quad(theme::control_surface(control, false));
            ctx.push_text_run(TextRun {
                x: control.x + 12.0,
                y: control.y + 5.0,
                text: (*label).to_string(),
                font_size: 12.0,
                color: theme::TEXT_SECONDARY,
                monospace: false,
            });
        }

        ctx.push_text_run(TextRun {
            x: connectivity.x + 18.0,
            y: connectivity.y + 16.0,
            text: "Input and Connectivity".into(),
            font_size: 12.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        for (index, (label, state)) in [
            ("Exclusive input", "On"),
            ("Touch mode", "Off"),
            ("Auto reconnect", "On"),
        ]
        .iter()
        .enumerate()
        {
            let row = Rect::new(connectivity.x + 18.0, connectivity.y + 42.0 + index as f32 * 28.0, connectivity.w - 36.0, 22.0);
            ctx.push_text_run(TextRun {
                x: row.x,
                y: row.y + 3.0,
                text: (*label).to_string(),
                font_size: 12.0,
                color: theme::TEXT_PRIMARY,
                monospace: false,
            });
            ctx.push_text_run(TextRun {
                x: row.x + row.w - 22.0,
                y: row.y + 3.0,
                text: (*state).to_string(),
                font_size: 12.0,
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
    fn profiles_panel_paints_structure() {
        let mut panel = ProfilesPanel::new();
        panel.layout(Rect::new(0.0, 0.0, 900.0, 520.0));
        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        assert!(ctx.glass_quads.len() >= 8);
        assert!(ctx.text_runs.len() >= 10);
    }
}
