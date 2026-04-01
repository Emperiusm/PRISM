// SPDX-License-Identifier: AGPL-3.0-or-later
//! Performance sub-panel — FPS, latency, decode time, bandwidth sparklines.

use super::stats_bar::SessionStats;
use crate::ui::widgets::label::Label;
use crate::ui::widgets::sparkline::Sparkline;
use crate::ui::widgets::{
    EventResponse, GlassQuad, PaintContext, Rect, Size, TextRun, UiEvent, Widget,
};

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct PerfPanel {
    fps_label: Label,
    fps_sparkline: Sparkline,
    latency_label: Label,
    latency_sparkline: Sparkline,
    decode_label: Label,
    bandwidth_label: Label,
    gaps_label: Label,
    rect: Rect,
    visible: bool,
}

const PANEL_W: f32 = 260.0;
const PANEL_H: f32 = 220.0;

impl PerfPanel {
    pub fn new() -> Self {
        Self {
            fps_label: Label::new("FPS: —", 12.0).with_monospace(true),
            fps_sparkline: Sparkline::new(60).with_range(0.0, 144.0),
            latency_label: Label::new("Latency: —", 12.0).with_monospace(true),
            latency_sparkline: Sparkline::new(60).with_range(0.0, 100.0),
            decode_label: Label::new("Decode: —", 12.0).with_monospace(true),
            bandwidth_label: Label::new("BW: —", 12.0).with_monospace(true),
            gaps_label: Label::new("Gaps: 0", 12.0),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            visible: false,
        }
    }

    pub fn update(&mut self, stats: &SessionStats) {
        self.fps_label.set_text(&format!("FPS: {:.0}", stats.fps));
        self.fps_sparkline.push(stats.fps);

        self.latency_label
            .set_text(&format!("Latency: {:.1}ms", stats.latency_ms));
        self.latency_sparkline.push(stats.latency_ms);

        self.decode_label
            .set_text(&format!("Decode: {:.1}ms", stats.decode_time_ms));

        let mbps = stats.bandwidth_bps as f32 / 1_000_000.0;
        self.bandwidth_label
            .set_text(&format!("BW: {mbps:.1} Mbps"));
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

impl Default for PerfPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for PerfPanel {
    fn layout(&mut self, available: Rect) -> Size {
        if !self.visible {
            return Size { w: 0.0, h: 0.0 };
        }
        let x = available.x;
        let y = available.y;
        self.rect = Rect::new(x, y, PANEL_W, PANEL_H);

        // Stack widgets inside panel with 8px padding
        let pad = 8.0;
        let inner_w = PANEL_W - pad * 2.0;
        let mut cur_y = y + 28.0; // leave room for title

        self.fps_label
            .layout(Rect::new(x + pad, cur_y, inner_w, 16.0));
        cur_y += 16.0 + 2.0;
        self.fps_sparkline
            .layout(Rect::new(x + pad, cur_y, inner_w, 24.0));
        cur_y += 24.0 + 4.0;

        self.latency_label
            .layout(Rect::new(x + pad, cur_y, inner_w, 16.0));
        cur_y += 16.0 + 2.0;
        self.latency_sparkline
            .layout(Rect::new(x + pad, cur_y, inner_w, 24.0));
        cur_y += 24.0 + 4.0;

        self.decode_label
            .layout(Rect::new(x + pad, cur_y, inner_w, 16.0));
        cur_y += 16.0 + 4.0;
        self.bandwidth_label
            .layout(Rect::new(x + pad, cur_y, inner_w, 16.0));
        cur_y += 16.0 + 4.0;
        self.gaps_label
            .layout(Rect::new(x + pad, cur_y, inner_w, 16.0));

        Size {
            w: PANEL_W,
            h: PANEL_H,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        if !self.visible {
            return;
        }

        // Panel background
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.06, 0.0, 0.12, 0.25],
            border_color: [1.0, 1.0, 1.0, 0.15],
            corner_radius: 8.0,
            noise_intensity: 0.02,
        });

        // Title
        ctx.push_text_run(TextRun {
            x: self.rect.x + 8.0,
            y: self.rect.y + 8.0,
            text: "Performance".into(),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });

        self.fps_label.paint(ctx);
        self.fps_sparkline.paint(ctx);
        self.latency_label.paint(ctx);
        self.latency_sparkline.paint(ctx);
        self.decode_label.paint(ctx);
        self.bandwidth_label.paint(ctx);
        self.gaps_label.paint(ctx);
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

    fn sample_stats() -> SessionStats {
        SessionStats {
            fps: 59.0,
            latency_ms: 8.3,
            decode_time_ms: 2.1,
            bandwidth_bps: 12_000_000,
            codec: "AV1".into(),
            resolution: (2560, 1440),
            active_profile: "Gaming".into(),
        }
    }

    #[test]
    fn perf_panel_updates_labels() {
        let mut panel = PerfPanel::new();
        panel.show();
        panel.update(&sample_stats());
        panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        let texts: Vec<&str> = ctx.text_runs.iter().map(|t| t.text.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains("FPS:") && t.contains("59")),
            "expected FPS label in text runs, got: {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("Latency:") && t.contains("8.3")),
            "expected Latency label in text runs, got: {texts:?}"
        );
    }

    #[test]
    fn perf_panel_hidden() {
        let mut panel = PerfPanel::new();
        // Not shown — layout returns zero size
        let size = panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));
        assert!(
            (size.w).abs() < 0.01 && (size.h).abs() < 0.01,
            "hidden panel should have zero size"
        );

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);
        assert!(
            ctx.glass_quads.is_empty(),
            "hidden panel should emit nothing"
        );
        assert!(ctx.text_runs.is_empty(), "hidden panel should emit nothing");
    }
}
