// SPDX-License-Identifier: AGPL-3.0-or-later
//! Floating session status bar shown at the top of the in-session overlay.

use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

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

fn latency_color(ms: f32) -> [f32; 4] {
    if ms < 20.0 {
        theme::SUCCESS
    } else if ms < 50.0 {
        theme::WARNING
    } else {
        theme::DANGER
    }
}

pub struct StatsBar {
    pub stats: SessionStats,
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
            profile_dropdown: Dropdown::new(vec!["Gaming".into(), "Coding".into()], 0),
            pinned: false,
            visible: false,
            fade_anim: Animation::new(EaseCurve::EaseOut, 200.0),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn update_stats(&mut self, stats: SessionStats) {
        let profile = stats.active_profile.clone();
        self.stats = stats;
        let options = ["Gaming", "Coding", "Balanced", "Low Bandwidth"];
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

    fn perf_btn_rect(&self) -> Rect { Rect::new(self.rect.x + self.rect.w - 220.0, self.rect.y + 12.0, 40.0, 24.0) }
    fn qual_btn_rect(&self) -> Rect { Rect::new(self.rect.x + self.rect.w - 170.0, self.rect.y + 12.0, 40.0, 24.0) }
    fn conn_btn_rect(&self) -> Rect { Rect::new(self.rect.x + self.rect.w - 120.0, self.rect.y + 12.0, 40.0, 24.0) }
    fn disp_btn_rect(&self) -> Rect { Rect::new(self.rect.x + self.rect.w - 70.0,  self.rect.y + 12.0, 40.0, 24.0) }

}

impl Default for StatsBar {
    fn default() -> Self { Self::new() }
}

impl Widget for StatsBar {
    fn layout(&mut self, available: Rect) -> Size {
        if !self.visible && self.fade_anim.value() < 0.01 {
            return Size { w: 0.0, h: 0.0 };
        }

        // Stitch capsule is a thinner, more centered pill
        let h = 56.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        
        let mx = self.rect.x + 130.0 + 45.0 + 65.0 + 65.0 + 60.0;
        self.profile_dropdown.layout(Rect::new(mx, self.rect.y + 16.0, 100.0, 24.0));
        
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let alpha = self.fade_anim.value();
        if alpha < 0.01 {
            return;
        }

        // Stitch glass pill
        ctx.push_glass_quad(theme::glass_quad(
            self.rect,
            [1.0, 1.0, 1.0, 0.45 * alpha],
            [1.0, 1.0, 1.0, 0.50 * alpha],
            28.0, // Fully rounded pill
        ));

        // 1. Brand 
        ctx.push_text_run(TextRun {
            x: self.rect.x + 20.0,
            y: self.rect.y + 20.0,
            text: "PRISM REMOTE".into(),
            font_size: 11.0,
            color: theme::ACCENT,
            monospace: false,
        });

        ctx.push_glass_quad(theme::separator(Rect::new(
            self.rect.x + 115.0,
            self.rect.y + 16.0,
            1.0,
            24.0,
        )));

        // 2. Metrics Block
        let mut mx = self.rect.x + 130.0;
        let metric_label_y = self.rect.y + 8.0;
        let metric_val_y = self.rect.y + 24.0;
        let metric_label_color = [0.0, 0.0, 0.0, 0.6 * alpha];
        let metric_val_color = [0.0, 0.0, 0.0, 0.9 * alpha];

        let draw_metric = |ctx: &mut PaintContext, x: f32, label: &str, val: &str, val_c: [f32; 4]| {
            ctx.push_text_run(TextRun { x, y: metric_label_y, text: label.into(), font_size: 9.0, color: metric_label_color, monospace: false });
            ctx.push_text_run(TextRun { x, y: metric_val_y,   text: val.into(),   font_size: 12.0, color: val_c, monospace: true });
        };

        draw_metric(ctx, mx, "FPS", &format!("{:.0}", self.stats.fps), metric_val_color);
        mx += 45.0;

        let lat_c = latency_color(self.stats.latency_ms);
        draw_metric(ctx, mx, "LATENCY", &format!("{:.0}ms", self.stats.latency_ms), [lat_c[0], lat_c[1], lat_c[2], alpha]);
        mx += 65.0;

        let mbps = self.stats.bandwidth_bps as f32 / 1_000_000.0;
        draw_metric(ctx, mx, "BITRATE", &format!("{:.1}M", mbps), metric_val_color);
        mx += 65.0;

        draw_metric(ctx, mx, "CODEC", if self.stats.codec.is_empty() { "---" } else { &self.stats.codec }, metric_val_color);

        // Profile Dropdown (Restores native interaction and SwitchProfile event)
        self.profile_dropdown.paint(ctx);

        // 3. Navigation Buttons (Right Aligned)
        ctx.push_glass_quad(theme::separator(Rect::new(
            self.perf_btn_rect().x - 16.0,
            self.rect.y + 16.0,
            1.0,
            24.0,
        )));

        let draw_nav_btn = |ctx: &mut PaintContext, r: Rect, label: &str| {
            ctx.push_text_run(TextRun {
                x: r.x + 6.0,
                y: r.y + 6.0,
                text: label.into(),
                font_size: 11.0,
                color: metric_label_color,
                monospace: false,
            });
        };

        draw_nav_btn(ctx, self.perf_btn_rect(), "PERF");
        draw_nav_btn(ctx, self.qual_btn_rect(), "QUAL");
        draw_nav_btn(ctx, self.conn_btn_rect(), "CONN");
        draw_nav_btn(ctx, self.disp_btn_rect(), "DISP");
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        let old_selection = self.profile_dropdown.selected_index();
        let dd_resp = self.profile_dropdown.handle_event(event);
        if self.profile_dropdown.selected_index() != old_selection {
            let profile = self.profile_dropdown.selected_text().to_owned();
            return EventResponse::Action(UiAction::SwitchProfile(profile));
        }
        if matches!(dd_resp, EventResponse::Consumed) {
            return EventResponse::Consumed;
        }

        match event {
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                if self.perf_btn_rect().contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenPanel("performance".into()));
                }
                if self.qual_btn_rect().contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenPanel("quality".into()));
                }
                if self.conn_btn_rect().contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenPanel("connection".into()));
                }
                if self.disp_btn_rect().contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenPanel("display".into()));
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
        bar.animate(300.0);
        bar.update_stats(stats);
        bar
    }

    #[test]
    fn stats_bar_paints_metrics() {
        let mut bar = make_visible_bar(sample_stats());
        bar.layout(Rect::new(0.0, 0.0, 960.0, 48.0));

        let mut ctx = PaintContext::new();
        bar.paint(&mut ctx);

        let texts: Vec<&str> = ctx.text_runs.iter().map(|t| t.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("60")), "expected FPS metric");
        assert!(texts.iter().any(|t| t.contains("12")), "expected latency metric");
    }

    #[test]
    fn stats_bar_navigation_click() {
        let mut bar = make_visible_bar(sample_stats());
        bar.layout(Rect::new(0.0, 0.0, 960.0, 56.0));

        let perf_rect = bar.perf_btn_rect();
        let resp = bar.handle_event(&UiEvent::MouseDown {
            x: perf_rect.x + 5.0,
            y: perf_rect.y + 5.0,
            button: MouseButton::Left,
        });

        match &resp {
            EventResponse::Action(UiAction::OpenPanel(p)) => {
                assert_eq!(p, "performance");
            }
            other => panic!("expected OpenPanel action, got {other:?}"),
        }
    }

    #[test]
    fn stats_bar_profile_dropdown_emits_switch() {
        let mut bar = make_visible_bar(sample_stats());
        bar.layout(Rect::new(0.0, 0.0, 960.0, 56.0));
        
        let mx = bar.rect.x + 130.0 + 45.0 + 65.0 + 65.0 + 60.0;
        let dd_rect = Rect::new(mx, bar.rect.y + 16.0, 100.0, 24.0);

        // Click dropdown open
        bar.handle_event(&UiEvent::MouseDown {
            x: dd_rect.x + 10.0,
            y: dd_rect.y + 10.0,
            button: MouseButton::Left,
        });

        // Dropdown layout hardcodes h=40.0, so options start at rect.y + 40.0.
        // Index 1 (Coding) starts at rect.y + 40.0 + 28.0.
        // Center of Index 1 is rect.y + 40.0 + 28.0 + 14.0.
        let item_y = dd_rect.y + 40.0 + 28.0 + 14.0;
        let resp = bar.handle_event(&UiEvent::MouseDown {
            x: dd_rect.x + 10.0,
            y: item_y,
            button: MouseButton::Left,
        });

        match &resp {
            EventResponse::Action(UiAction::SwitchProfile(p)) => {
                assert_eq!(p, "Coding", "expected SwitchProfile");
            }
            other => panic!("expected SwitchProfile action, got {other:?}"),
        }
    }
}
