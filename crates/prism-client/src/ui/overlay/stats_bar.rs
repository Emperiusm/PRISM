// SPDX-License-Identifier: AGPL-3.0-or-later
//! Full-width stats bar — live session metrics at the top of the overlay.

use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::widgets::{
    EventResponse, GlassQuad, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};
use crate::ui::widgets::dropdown::Dropdown;

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    pub fps: f32,
    pub latency_ms: f32,
    pub decode_time_ms: f32,
    pub bandwidth_bps: u64,
    pub codec: String,
    pub resolution: (u32, u32),
    pub active_profile: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return green/yellow/red colour depending on direction of "good".
/// `lower_is_better=false` → green if value >= good_threshold, yellow if >= warn_threshold.
/// `lower_is_better=true`  → green if value <  good_threshold, yellow if <  warn_threshold.
fn metric_color(value: f32, good_threshold: f32, warn_threshold: f32) -> [f32; 4] {
    if value >= good_threshold {
        [0.2, 0.9, 0.3, 1.0] // green
    } else if value >= warn_threshold {
        [1.0, 0.85, 0.1, 1.0] // yellow
    } else {
        [0.95, 0.25, 0.2, 1.0] // red
    }
}

/// Latency: lower is better.
fn latency_color(ms: f32) -> [f32; 4] {
    if ms < 20.0 {
        [0.2, 0.9, 0.3, 1.0]
    } else if ms < 50.0 {
        [1.0, 0.85, 0.1, 1.0]
    } else {
        [0.95, 0.25, 0.2, 1.0]
    }
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

pub struct StatsBar {
    stats: SessionStats,
    profile_dropdown: Dropdown,
    pinned: bool,
    visible: bool,
    fade_anim: Animation,
    rect: Rect,
}

impl StatsBar {
    pub fn new() -> Self {
        Self {
            stats: SessionStats::default(),
            profile_dropdown: Dropdown::new(
                vec!["Gaming".into(), "Coding".into()],
                0,
            ),
            pinned: false,
            visible: false,
            fade_anim: Animation::new(EaseCurve::EaseOut, 200.0),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn update_stats(&mut self, stats: SessionStats) {
        // Sync dropdown selection with active profile
        let profile = stats.active_profile.clone();
        self.stats = stats;
        let options = ["Gaming", "Coding"];
        if let Some(idx) = options.iter().position(|&p| p == profile.as_str()) {
            self.profile_dropdown.set_selected(idx);
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.fade_anim.set_target(1.0);
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.fade_anim.set_target(0.0);
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
    }

    // Hit-test rects (computed relative to self.rect)
    fn fps_rect(&self) -> Rect {
        Rect::new(self.rect.x + 8.0, self.rect.y, 90.0, self.rect.h)
    }

    fn codec_rect(&self) -> Rect {
        // Codec + resolution share an area for the quality panel shortcut
        Rect::new(self.rect.x + 260.0, self.rect.y, 160.0, self.rect.h)
    }

    fn pin_rect(&self) -> Rect {
        let x = self.rect.x + self.rect.w - 80.0;
        Rect::new(x, self.rect.y, 30.0, self.rect.h)
    }

    fn close_rect(&self) -> Rect {
        let x = self.rect.x + self.rect.w - 44.0;
        Rect::new(x, self.rect.y, 30.0, self.rect.h)
    }

    fn dropdown_rect(&self) -> Rect {
        let x = self.rect.x + self.rect.w - 200.0 - 44.0 - 30.0 - 8.0;
        Rect::new(x, self.rect.y + 2.0, 120.0, 32.0)
    }
}

impl Default for StatsBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for StatsBar {
    fn layout(&mut self, available: Rect) -> Size {
        if !self.visible && self.fade_anim.value() < 0.01 {
            return Size { w: 0.0, h: 0.0 };
        }
        let w = available.w;
        let h = 36.0;
        self.rect = Rect::new(available.x, available.y, w, h);

        // Layout the dropdown within the bar
        let dd_rect = self.dropdown_rect();
        self.profile_dropdown.layout(dd_rect);

        Size { w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let alpha = self.fade_anim.value();
        if alpha < 0.01 {
            return;
        }

        let base_tint_alpha = if self.pinned { 0.1 } else { 0.2 };

        // Bar background
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.06, 0.0, 0.12, base_tint_alpha * alpha],
            border_color: [1.0, 1.0, 1.0, 0.15 * alpha],
            corner_radius: 0.0,
            noise_intensity: 0.02,
        });

        let y_text = self.rect.y + 11.0;
        let mut x_cursor = self.rect.x + 8.0;
        let sep_color = [1.0, 1.0, 1.0, 0.12 * alpha];

        // Helper to push a vertical separator
        macro_rules! push_sep {
            ($ctx:expr, $x:expr, $rect:expr) => {
                $ctx.push_glass_quad(GlassQuad {
                    rect: Rect::new($x, $rect.y + 4.0, 1.0, $rect.h - 8.0),
                    blur_rect: Rect::new($x, $rect.y + 4.0, 1.0, $rect.h - 8.0),
                    tint: sep_color,
                    border_color: [0.0; 4],
                    corner_radius: 0.0,
                    noise_intensity: 0.0,
                });
            };
        }

        // FPS
        let fps_color = metric_color(self.stats.fps, 30.0, 15.0);
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: format!("FPS: {:.0}", self.stats.fps),
            font_size: 13.0,
            color: [fps_color[0], fps_color[1], fps_color[2], fps_color[3] * alpha],
            monospace: true,
        });
        x_cursor += 90.0;

        push_sep!(ctx, x_cursor, self.rect);
        x_cursor += 8.0;

        // Latency
        let lat_color = latency_color(self.stats.latency_ms);
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: format!("Latency: {:.1}ms", self.stats.latency_ms),
            font_size: 13.0,
            color: [lat_color[0], lat_color[1], lat_color[2], lat_color[3] * alpha],
            monospace: true,
        });
        x_cursor += 130.0;

        push_sep!(ctx, x_cursor, self.rect);
        x_cursor += 8.0;

        // Codec
        let codec = if self.stats.codec.is_empty() { "—".to_owned() } else { self.stats.codec.clone() };
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: codec,
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.9 * alpha],
            monospace: false,
        });
        x_cursor += 70.0;

        push_sep!(ctx, x_cursor, self.rect);
        x_cursor += 8.0;

        // Resolution
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: format!("{}×{}", self.stats.resolution.0, self.stats.resolution.1),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.9 * alpha],
            monospace: false,
        });
        x_cursor += 90.0;

        push_sep!(ctx, x_cursor, self.rect);
        x_cursor += 8.0;

        // Bandwidth
        let mbps = self.stats.bandwidth_bps as f32 / 1_000_000.0;
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: format!("{mbps:.1} Mbps"),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.9 * alpha],
            monospace: true,
        });

        // Profile dropdown
        self.profile_dropdown.paint(ctx);

        // Pin icon
        let pin_x = self.rect.x + self.rect.w - 80.0;
        ctx.push_text_run(TextRun {
            x: pin_x,
            y: y_text,
            text: if self.pinned { "[*]".into() } else { "[pin]".into() },
            font_size: 12.0,
            color: [1.0, 1.0, 1.0, if self.pinned { 0.9 } else { 0.5 } * alpha],
            monospace: false,
        });

        // Close icon
        let close_x = self.rect.x + self.rect.w - 44.0;
        ctx.push_text_run(TextRun {
            x: close_x,
            y: y_text,
            text: "[x]".into(),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.6 * alpha],
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // First try the profile dropdown; track old selection
        let old_selection = self.profile_dropdown.selected_index();
        let dd_resp = self.profile_dropdown.handle_event(event);

        // Check if selection changed
        let new_selection = self.profile_dropdown.selected_index();
        if new_selection != old_selection {
            let profile = self.profile_dropdown.selected_text().to_owned();
            return EventResponse::Action(UiAction::SwitchProfile(profile));
        }

        if matches!(dd_resp, EventResponse::Consumed) {
            return EventResponse::Consumed;
        }

        match event {
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                if self.close_rect().contains(*x, *y) {
                    return EventResponse::Action(UiAction::CloseOverlay);
                }
                if self.pin_rect().contains(*x, *y) {
                    self.toggle_pin();
                    return EventResponse::Consumed;
                }
                if self.fps_rect().contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenPanel("performance".into()));
                }
                if self.codec_rect().contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenPanel("quality".into()));
                }
                if self.rect.contains(*x, *y) {
                    return EventResponse::Consumed;
                }
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.fade_anim.tick(dt_ms);
        self.profile_dropdown.animate(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stats() -> SessionStats {
        SessionStats {
            fps: 60.0,
            latency_ms: 12.5,
            decode_time_ms: 3.0,
            bandwidth_bps: 8_000_000,
            codec: "H264".into(),
            resolution: (1920, 1080),
            active_profile: "Gaming".into(),
        }
    }

    fn make_visible_bar(stats: SessionStats) -> StatsBar {
        let mut bar = StatsBar::new();
        bar.show();
        bar.animate(300.0); // snap fade anim to 1.0
        bar.update_stats(stats);
        bar
    }

    #[test]
    fn stats_bar_paints_metrics() {
        let mut bar = make_visible_bar(sample_stats());
        bar.layout(Rect::new(0.0, 0.0, 1920.0, 36.0));

        let mut ctx = PaintContext::new();
        bar.paint(&mut ctx);

        // Should have text runs containing fps and latency
        let texts: Vec<&str> = ctx.text_runs.iter().map(|t| t.text.as_str()).collect();
        let has_fps = texts.iter().any(|t| t.contains("FPS:") && t.contains("60"));
        let has_latency = texts.iter().any(|t| t.contains("Latency:") && t.contains("12.5"));
        assert!(has_fps, "expected FPS metric in text runs, got: {texts:?}");
        assert!(has_latency, "expected Latency metric in text runs, got: {texts:?}");
    }

    #[test]
    fn stats_bar_profile_switch() {
        let mut bar = make_visible_bar(sample_stats());
        bar.layout(Rect::new(0.0, 0.0, 1920.0, 36.0));

        // Compute the dropdown header rect and click it to open
        let dd_rect = bar.dropdown_rect();
        bar.handle_event(&UiEvent::MouseDown {
            x: dd_rect.x + 10.0,
            y: dd_rect.y + 10.0,
            button: MouseButton::Left,
        });

        // Item 1 (Coding) is at y = dd_rect.y + 32 + 1*28, center = dd_rect.y + 32 + 14 + 28 = dd_rect.y + 74
        let item_y = dd_rect.y + 32.0 + 28.0 + 14.0;
        let resp = bar.handle_event(&UiEvent::MouseDown {
            x: dd_rect.x + 10.0,
            y: item_y,
            button: MouseButton::Left,
        });

        match &resp {
            EventResponse::Action(UiAction::SwitchProfile(p)) => {
                assert_eq!(p, "Coding", "expected Coding profile, got {p}");
            }
            other => panic!("expected SwitchProfile action, got {other:?}"),
        }
    }

    #[test]
    fn stats_bar_hidden_no_paint() {
        let mut bar = StatsBar::new();
        // Never shown — fade_anim stays at 0.0
        bar.layout(Rect::new(0.0, 0.0, 1920.0, 36.0));
        let mut ctx = PaintContext::new();
        bar.paint(&mut ctx);
        assert!(ctx.text_runs.is_empty(), "hidden bar should emit no text runs");
        assert!(ctx.glass_quads.is_empty(), "hidden bar should emit no glass quads");
    }
}
