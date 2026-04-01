// SPDX-License-Identifier: AGPL-3.0-or-later
//! Quality settings sub-panel — codec, FPS, bandwidth, lossless.

use crate::ui::widgets::{
    EventResponse, GlassQuad, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::checkbox::Checkbox;
use crate::ui::widgets::slider::Slider;

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct QualityPanel {
    profile_dropdown: Dropdown,
    encoder_dropdown: Dropdown,
    fps_dropdown: Dropdown,
    lossless_checkbox: Checkbox,
    region_checkbox: Checkbox,
    bandwidth_slider: Slider,
    rect: Rect,
    visible: bool,
}

const PANEL_W: f32 = 260.0;
const PANEL_H: f32 = 280.0;

impl QualityPanel {
    pub fn new() -> Self {
        Self {
            profile_dropdown: Dropdown::new(
                vec!["Gaming".into(), "Coding".into()],
                0,
            ),
            encoder_dropdown: Dropdown::new(
                vec!["Ultra Low".into(), "Balanced".into(), "Quality".into()],
                1,
            ),
            fps_dropdown: Dropdown::new(
                vec!["30".into(), "60".into(), "120".into()],
                1,
            ),
            lossless_checkbox: Checkbox::new("Lossless text", false),
            region_checkbox: Checkbox::new("Region detection", false),
            bandwidth_slider: Slider::new("BW limit", 1.0, 100.0, 100.0)
                .with_format(|v| format!("{v:.0} Mbps")),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            visible: false,
        }
    }

    pub fn show(&mut self) { self.visible = true; }
    pub fn hide(&mut self) { self.visible = false; }
    pub fn is_visible(&self) -> bool { self.visible }

    fn layout_children(&mut self) {
        let pad = 8.0;
        let inner_w = PANEL_W - pad * 2.0;
        let x = self.rect.x + pad;
        let mut cur_y = self.rect.y + 28.0;

        self.profile_dropdown.layout(Rect::new(x, cur_y, inner_w, 32.0));
        cur_y += 36.0;
        self.encoder_dropdown.layout(Rect::new(x, cur_y, inner_w, 32.0));
        cur_y += 36.0;
        self.fps_dropdown.layout(Rect::new(x, cur_y, inner_w, 32.0));
        cur_y += 36.0;
        self.lossless_checkbox.layout(Rect::new(x, cur_y, inner_w, 24.0));
        cur_y += 28.0;
        self.region_checkbox.layout(Rect::new(x, cur_y, inner_w, 24.0));
        cur_y += 28.0;
        self.bandwidth_slider.layout(Rect::new(x, cur_y, inner_w, 32.0));
    }
}

impl Default for QualityPanel {
    fn default() -> Self { Self::new() }
}

impl Widget for QualityPanel {
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
            text: "Quality".into(),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });

        self.profile_dropdown.paint(ctx);
        self.encoder_dropdown.paint(ctx);
        self.fps_dropdown.paint(ctx);
        self.lossless_checkbox.paint(ctx);
        self.region_checkbox.paint(ctx);
        self.bandwidth_slider.paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Profile dropdown
        let old_profile = self.profile_dropdown.selected_index();
        let r = self.profile_dropdown.handle_event(event);
        if self.profile_dropdown.selected_index() != old_profile {
            let preset = Some(self.profile_dropdown.selected_text().to_owned());
            return EventResponse::Action(UiAction::UpdateQuality {
                preset,
                max_fps: None,
                lossless_text: None,
                region_detection: None,
            });
        }
        if matches!(r, EventResponse::Consumed) { return r; }

        // Encoder dropdown (maps to preset)
        let old_enc = self.encoder_dropdown.selected_index();
        let r = self.encoder_dropdown.handle_event(event);
        if self.encoder_dropdown.selected_index() != old_enc {
            let preset = Some(self.encoder_dropdown.selected_text().to_owned());
            return EventResponse::Action(UiAction::UpdateQuality {
                preset,
                max_fps: None,
                lossless_text: None,
                region_detection: None,
            });
        }
        if matches!(r, EventResponse::Consumed) { return r; }

        // FPS dropdown
        let old_fps = self.fps_dropdown.selected_index();
        let r = self.fps_dropdown.handle_event(event);
        if self.fps_dropdown.selected_index() != old_fps {
            let fps_str = self.fps_dropdown.selected_text();
            let max_fps = fps_str.parse::<u8>().ok();
            return EventResponse::Action(UiAction::UpdateQuality {
                preset: None,
                max_fps,
                lossless_text: None,
                region_detection: None,
            });
        }
        if matches!(r, EventResponse::Consumed) { return r; }

        // Lossless checkbox
        let old_lossless = self.lossless_checkbox.is_checked();
        let r = self.lossless_checkbox.handle_event(event);
        if self.lossless_checkbox.is_checked() != old_lossless {
            return EventResponse::Action(UiAction::UpdateQuality {
                preset: None,
                max_fps: None,
                lossless_text: Some(self.lossless_checkbox.is_checked()),
                region_detection: None,
            });
        }
        if matches!(r, EventResponse::Consumed) { return r; }

        // Region checkbox
        let old_region = self.region_checkbox.is_checked();
        let r = self.region_checkbox.handle_event(event);
        if self.region_checkbox.is_checked() != old_region {
            return EventResponse::Action(UiAction::UpdateQuality {
                preset: None,
                max_fps: None,
                lossless_text: None,
                region_detection: Some(self.region_checkbox.is_checked()),
            });
        }
        if matches!(r, EventResponse::Consumed) { return r; }

        // Bandwidth slider
        let old_bw = self.bandwidth_slider.value();
        let r = self.bandwidth_slider.handle_event(event);
        if (self.bandwidth_slider.value() - old_bw).abs() > 0.01 {
            let bps = (self.bandwidth_slider.value() * 1_000_000.0) as u64;
            return EventResponse::Action(UiAction::SetBandwidthLimit(bps));
        }
        if matches!(r, EventResponse::Consumed) { return r; }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        self.profile_dropdown.animate(dt_ms);
        self.encoder_dropdown.animate(dt_ms);
        self.fps_dropdown.animate(dt_ms);
        self.lossless_checkbox.animate(dt_ms);
        self.region_checkbox.animate(dt_ms);
        self.bandwidth_slider.animate(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_panel_paints() {
        let mut panel = QualityPanel::new();
        panel.show();
        panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        assert!(!ctx.glass_quads.is_empty(), "expected at least one glass quad");
        let texts: Vec<&str> = ctx.text_runs.iter().map(|t| t.text.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains("Quality")),
            "expected Quality title, got: {texts:?}"
        );
    }

    #[test]
    fn quality_panel_hidden() {
        let mut panel = QualityPanel::new();
        let size = panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));
        assert!((size.w).abs() < 0.01 && (size.h).abs() < 0.01);
        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);
        assert!(ctx.glass_quads.is_empty());
        assert!(ctx.text_runs.is_empty());
    }
}
