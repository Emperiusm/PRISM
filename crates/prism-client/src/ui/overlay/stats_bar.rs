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

fn metric_color(value: f32, good_threshold: f32, warn_threshold: f32) -> [f32; 4] {
    if value >= good_threshold {
        theme::SUCCESS
    } else if value >= warn_threshold {
        theme::WARNING
    } else {
        theme::DANGER
    }
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

    fn fps_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + 18.0,
            self.rect.y + 6.0,
            92.0,
            self.rect.h - 12.0,
        )
    }

    fn codec_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + 252.0,
            self.rect.y + 6.0,
            220.0,
            self.rect.h - 12.0,
        )
    }

    fn pin_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + self.rect.w - 124.0,
            self.rect.y + 8.0,
            54.0,
            self.rect.h - 16.0,
        )
    }

    fn close_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + self.rect.w - 62.0,
            self.rect.y + 8.0,
            44.0,
            self.rect.h - 16.0,
        )
    }

    fn dropdown_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + self.rect.w - 270.0,
            self.rect.y + 4.0,
            132.0,
            self.rect.h - 8.0,
        )
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

        let h = available.h.max(48.0);
        self.rect = Rect::new(available.x, available.y, available.w, h);
        self.profile_dropdown.layout(self.dropdown_rect());

        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let alpha = self.fade_anim.value();
        if alpha < 0.01 {
            return;
        }

        ctx.push_glass_quad(theme::glass_quad(
            self.rect,
            [0.12, 0.16, 0.22, 0.80 * alpha],
            if self.pinned {
                [
                    theme::ACCENT[0],
                    theme::ACCENT[1],
                    theme::ACCENT[2],
                    0.20 * alpha,
                ]
            } else {
                [1.0, 1.0, 1.0, 0.16 * alpha]
            },
            theme::PANEL_RADIUS,
        ));

        let y_text = self.rect.y + 16.0;
        let mut x_cursor = self.rect.x + 18.0;

        let fps_color = metric_color(self.stats.fps, 30.0, 15.0);
        let fps_text = format!("{:.0} FPS", self.stats.fps);
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: fps_text,
            font_size: 13.0,
            color: [fps_color[0], fps_color[1], fps_color[2], alpha],
            monospace: true,
        });
        x_cursor += 92.0;

        ctx.push_glass_quad(theme::separator(Rect::new(
            x_cursor,
            self.rect.y + 10.0,
            1.0,
            self.rect.h - 20.0,
        )));
        x_cursor += 14.0;

        let latency = latency_color(self.stats.latency_ms);
        let latency_text = format!("{:.1} ms", self.stats.latency_ms);
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: latency_text,
            font_size: 13.0,
            color: [latency[0], latency[1], latency[2], alpha],
            monospace: true,
        });
        x_cursor += 104.0;

        ctx.push_glass_quad(theme::separator(Rect::new(
            x_cursor,
            self.rect.y + 10.0,
            1.0,
            self.rect.h - 20.0,
        )));
        x_cursor += 14.0;

        let codec = if self.stats.codec.is_empty() {
            "Codec --".to_owned()
        } else {
            format!("Codec {}", self.stats.codec)
        };
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: codec,
            font_size: 13.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
        x_cursor += 108.0;

        let resolution = format!("{}x{}", self.stats.resolution.0, self.stats.resolution.1);
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: resolution,
            font_size: 13.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });
        x_cursor += 98.0;

        ctx.push_glass_quad(theme::separator(Rect::new(
            x_cursor,
            self.rect.y + 10.0,
            1.0,
            self.rect.h - 20.0,
        )));
        x_cursor += 14.0;

        let mbps = self.stats.bandwidth_bps as f32 / 1_000_000.0;
        ctx.push_text_run(TextRun {
            x: x_cursor,
            y: y_text,
            text: format!("{mbps:.1} Mbps"),
            font_size: 13.0,
            color: theme::TEXT_SECONDARY,
            monospace: true,
        });

        self.profile_dropdown.paint(ctx);

        let pin_rect = self.pin_rect();
        ctx.push_glass_quad(theme::glass_quad(
            pin_rect,
            if self.pinned {
                [
                    theme::ACCENT[0],
                    theme::ACCENT[1],
                    theme::ACCENT[2],
                    0.14 * alpha,
                ]
            } else {
                [1.0, 1.0, 1.0, 0.05 * alpha]
            },
            if self.pinned {
                [
                    theme::ACCENT[0],
                    theme::ACCENT[1],
                    theme::ACCENT[2],
                    0.22 * alpha,
                ]
            } else {
                [1.0, 1.0, 1.0, 0.08 * alpha]
            },
            theme::CHIP_RADIUS,
        ));
        let pin_label = if self.pinned { "Pinned" } else { "Pin" };
        ctx.push_text_run(TextRun {
            x: pin_rect.x + (pin_rect.w - theme::text_width(pin_label, 12.0)) * 0.5,
            y: pin_rect.y + 6.0,
            text: pin_label.into(),
            font_size: 12.0,
            color: if self.pinned {
                theme::accent(alpha)
            } else {
                theme::TEXT_SECONDARY
            },
            monospace: false,
        });

        let close_rect = self.close_rect();
        ctx.push_glass_quad(theme::glass_quad(
            close_rect,
            [1.0, 1.0, 1.0, 0.05 * alpha],
            [1.0, 1.0, 1.0, 0.08 * alpha],
            theme::CHIP_RADIUS,
        ));
        ctx.push_text_run(TextRun {
            x: close_rect.x + (close_rect.w - theme::text_width("Done", 12.0)) * 0.5,
            y: close_rect.y + 6.0,
            text: "Done".into(),
            font_size: 12.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });
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
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
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
        let has_fps = texts.iter().any(|t| t.contains("60 FPS"));
        let has_latency = texts.iter().any(|t| t.contains("12.5 ms"));
        assert!(has_fps, "expected FPS metric in text runs, got: {texts:?}");
        assert!(
            has_latency,
            "expected latency metric in text runs, got: {texts:?}"
        );
    }

    #[test]
    fn stats_bar_profile_switch() {
        let mut bar = make_visible_bar(sample_stats());
        bar.layout(Rect::new(0.0, 0.0, 960.0, 48.0));

        let dd_rect = bar.dropdown_rect();
        bar.handle_event(&UiEvent::MouseDown {
            x: dd_rect.x + 10.0,
            y: dd_rect.y + 10.0,
            button: MouseButton::Left,
        });

        let item_y = dd_rect.y + dd_rect.h + 28.0 + 14.0;
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
        bar.layout(Rect::new(0.0, 0.0, 960.0, 48.0));
        let mut ctx = PaintContext::new();
        bar.paint(&mut ctx);
        assert!(
            ctx.text_runs.is_empty(),
            "hidden bar should emit no text runs"
        );
        assert!(
            ctx.glass_quads.is_empty(),
            "hidden bar should emit no glass quads"
        );
    }
}
