// SPDX-License-Identifier: AGPL-3.0-or-later
//! Top capsule overlay with single-panel expansion and floating bottom bar accessories.

use super::conn_panel::ConnPanel;
use super::display_panel::DisplayPanel;
use super::perf_panel::PerfPanel;
use super::quality_panel::QualityPanel;
use super::stats_bar::{SessionStats, StatsBar};
use crate::ui::theme;
use crate::ui::widgets::icon::{ICON_BOLT, ICON_CLOCK, ICON_MONITOR, ICON_STREAMING, Icon};
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapsulePanel {
    Performance,
    Quality,
    Connection,
    Display,
}

pub struct OverlayCapsule {
    stats_bar: StatsBar,
    perf_panel: PerfPanel,
    quality_panel: QualityPanel,
    conn_panel: ConnPanel,
    display_panel: DisplayPanel,
    rect: Rect,
    capsule_rect: Rect,
    sidebar_rect: Rect,
    bottom_bar_rect: Rect,
    disconnect_rect: Rect,
    gpu_card_rect: Rect,
    panel_rect: Option<Rect>,
    visible: bool,
    active_panel: Option<CapsulePanel>,
    sidebar_nav_rects: Vec<(CapsulePanel, Rect)>,
}

impl OverlayCapsule {
    pub fn new() -> Self {
        Self {
            stats_bar: StatsBar::new(),
            perf_panel: PerfPanel::new(),
            quality_panel: QualityPanel::new(),
            conn_panel: ConnPanel::new(),
            display_panel: DisplayPanel::new(),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            capsule_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            sidebar_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            bottom_bar_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            disconnect_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            gpu_card_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            panel_rect: None,
            visible: false,
            active_panel: None,
            sidebar_nav_rects: Vec::new(),
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.stats_bar.show();
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.stats_bar.hide();
        self.set_active_panel(None);
    }

    pub fn update_stats(&mut self, stats: SessionStats) {
        self.stats_bar.update_stats(stats.clone());
        self.perf_panel.update(&stats);
    }

    fn set_active_panel(&mut self, panel: Option<CapsulePanel>) {
        self.active_panel = panel;
        self.perf_panel.hide();
        self.quality_panel.hide();
        self.conn_panel.hide();
        self.display_panel.hide();
        match panel {
            Some(CapsulePanel::Performance) => self.perf_panel.show(),
            Some(CapsulePanel::Quality) => self.quality_panel.show(),
            Some(CapsulePanel::Connection) => self.conn_panel.show(),
            Some(CapsulePanel::Display) => self.display_panel.show(),
            None => {}
        }
    }

    fn open_panel_by_name(&mut self, name: &str) {
        let panel = match name {
            "performance" => Some(CapsulePanel::Performance),
            "quality" => Some(CapsulePanel::Quality),
            "connection" => Some(CapsulePanel::Connection),
            "display" => Some(CapsulePanel::Display),
            _ => None,
        };
        self.set_active_panel(panel);
    }

    fn selected_panel(&self) -> CapsulePanel {
        self.active_panel.unwrap_or(CapsulePanel::Performance)
    }

    fn panel_label(panel: CapsulePanel) -> &'static str {
        match panel {
            CapsulePanel::Performance => "Performance",
            CapsulePanel::Quality => "Quality",
            CapsulePanel::Connection => "Connection",
            CapsulePanel::Display => "Display",
        }
    }

    fn panel_icon(panel: CapsulePanel) -> char {
        match panel {
            CapsulePanel::Performance => ICON_BOLT,
            CapsulePanel::Quality => ICON_CLOCK,
            CapsulePanel::Connection => ICON_STREAMING,
            CapsulePanel::Display => ICON_MONITOR,
        }
    }
}

impl Default for OverlayCapsule {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for OverlayCapsule {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        if !self.visible {
            return Size { w: 0.0, h: 0.0 };
        }

        let capsule_w = (available.w - 40.0).clamp(520.0, 780.0);
        let capsule_x = available.x + (available.w - capsule_w) * 0.5;
        self.capsule_rect = Rect::new(capsule_x, available.y + 18.0, capsule_w, 56.0);
        self.stats_bar.layout(self.capsule_rect);

        self.sidebar_rect = Rect::new(
            available.x + 24.0,
            available.y + 72.0,
            256.0,
            (available.h - 96.0).max(420.0),
        );
        self.gpu_card_rect = Rect::new(
            self.sidebar_rect.x + 16.0,
            self.sidebar_rect.y + self.sidebar_rect.h - 60.0,
            self.sidebar_rect.w - 32.0,
            44.0,
        );
        self.sidebar_nav_rects.clear();
        let nav_x = self.sidebar_rect.x + 16.0;
        let mut nav_y = self.sidebar_rect.y + 72.0;
        for panel in [
            CapsulePanel::Performance,
            CapsulePanel::Quality,
            CapsulePanel::Connection,
            CapsulePanel::Display,
        ] {
            self.sidebar_nav_rects.push((
                panel,
                Rect::new(nav_x, nav_y, self.sidebar_rect.w - 32.0, 32.0),
            ));
            nav_y += 34.0;
        }

        // Bottom Info Bar
        let bot_w = 420.0;
        self.bottom_bar_rect = Rect::new(
            available.x + (available.w - bot_w) * 0.5,
            available.y + available.h - 60.0,
            bot_w,
            40.0,
        );

        // Disconnect Circle Bottom Right
        self.disconnect_rect = Rect::new(
            available.x + available.w - 80.0,
            available.y + available.h - 80.0,
            56.0,
            56.0,
        );

        self.panel_rect = None;
        if let Some(panel) = self.active_panel {
            // Dropdown panel dimensions
            let panel_w = 320.0;
            let panel_x = self.sidebar_rect.x + self.sidebar_rect.w + 24.0;
            let panel_y = self.sidebar_rect.y + 12.0;
            let panel_rect = Rect::new(panel_x, panel_y, panel_w, 320.0);
            self.panel_rect = Some(panel_rect);
            match panel {
                CapsulePanel::Performance => {
                    self.perf_panel.layout(panel_rect);
                }
                CapsulePanel::Quality => {
                    self.quality_panel.layout(panel_rect);
                }
                CapsulePanel::Connection => {
                    self.conn_panel.layout(panel_rect);
                }
                CapsulePanel::Display => {
                    self.display_panel.layout(panel_rect);
                }
            }
        }

        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        if !self.visible {
            return;
        }

        self.stats_bar.paint(ctx);
        let selected_panel = self.selected_panel();

        ctx.push_glow_rect(theme::hover_elevation_shadow(self.sidebar_rect, 16.0, 1.0));
        ctx.push_glass_quad(theme::sidebar_mica_surface(self.sidebar_rect, 16.0));

        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: self.sidebar_rect.x + 20.0,
                y: self.sidebar_rect.y + 22.0,
                text: "Stream Control".into(),
                font_size: 16.0,
                color: theme::LT_TEXT_PRIMARY,
                bold: true,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );
        ctx.push_glass_quad(theme::glass_quad(
            Rect::new(
                self.sidebar_rect.x + 20.0,
                self.sidebar_rect.y + 42.0,
                8.0,
                8.0,
            ),
            [0.086, 0.702, 0.286, 1.0],
            [0.086, 0.702, 0.286, 1.0],
            4.0,
        ));
        let resolution_label = if self.stats_bar.stats.resolution.0 >= 3840 {
            "4K"
        } else if self.stats_bar.stats.resolution.0 >= 2560 {
            "1440P"
        } else {
            "1080P"
        };
        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: self.sidebar_rect.x + 34.0,
                y: self.sidebar_rect.y + 38.0,
                text: format!(
                    "{resolution_label} @ {:.0}FPS • ACTIVE",
                    self.stats_bar.stats.fps.max(1.0)
                ),
                font_size: 11.0,
                color: theme::LT_TEXT_SECONDARY,
                bold: true,
                letter_spacing: 0.05,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );

        for (panel, rect) in &self.sidebar_nav_rects {
            let active = *panel == selected_panel;
            if active {
                ctx.push_glass_quad(theme::glass_quad(
                    *rect,
                    theme::PRIMARY_BLUE,
                    [0.0, 0.0, 0.0, 0.0],
                    8.0,
                ));
            }

            Icon::new(Self::panel_icon(*panel))
                .with_size(14.0)
                .with_color(if active {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    theme::LT_TEXT_PRIMARY
                })
                .at(rect.x + 12.0, rect.y + 9.0)
                .paint(ctx);

            let nav_run = TextRun {
                x: rect.x + 34.0,
                y: rect.y + 9.0,
                text: Self::panel_label(*panel).into(),
                font_size: 13.0,
                color: if active {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    theme::LT_TEXT_PRIMARY
                },
                bold: true,
                ..Default::default()
            };
            if active {
                ctx.push_text_run(nav_run);
            } else {
                theme::push_text_with_shadow(ctx, nav_run, theme::CONTRAST_TEXT_SHADOW);
            }
        }

        ctx.push_glass_quad(theme::glass_panel_light_surface(self.gpu_card_rect, 12.0));
        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: self.gpu_card_rect.x + 12.0,
                y: self.gpu_card_rect.y + 10.0,
                text: "GPU USAGE".into(),
                font_size: 10.0,
                color: theme::LT_TEXT_PRIMARY,
                bold: true,
                letter_spacing: 0.05,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );
        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: self.gpu_card_rect.x + self.gpu_card_rect.w - 34.0,
                y: self.gpu_card_rect.y + 10.0,
                text: "32%".into(),
                font_size: 10.0,
                color: theme::PRIMARY_BLUE,
                bold: true,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );
        let gpu_track = Rect::new(
            self.gpu_card_rect.x + 12.0,
            self.gpu_card_rect.y + self.gpu_card_rect.h - 14.0,
            self.gpu_card_rect.w - 24.0,
            6.0,
        );
        ctx.push_glass_quad(theme::glass_quad(
            gpu_track,
            [0.0, 0.0, 0.0, 0.08],
            [0.0, 0.0, 0.0, 0.0],
            3.0,
        ));
        ctx.push_glass_quad(theme::glass_quad(
            Rect::new(gpu_track.x, gpu_track.y, gpu_track.w * 0.32, gpu_track.h),
            theme::PRIMARY_BLUE,
            [0.0, 0.0, 0.0, 0.0],
            3.0,
        ));

        match self.active_panel {
            Some(CapsulePanel::Performance) => self.perf_panel.paint(ctx),
            Some(CapsulePanel::Quality) => self.quality_panel.paint(ctx),
            Some(CapsulePanel::Connection) => self.conn_panel.paint(ctx),
            Some(CapsulePanel::Display) => self.display_panel.paint(ctx),
            None => {}
        }

        // Paint Bottom Technical Specs Bar
        ctx.push_glow_rect(theme::signature_shadow(self.bottom_bar_rect, 20.0));
        ctx.push_glass_quad(theme::glass_panel_light_surface(self.bottom_bar_rect, 20.0));

        let mut bl_x = self.bottom_bar_rect.x + 30.0;
        let bl_y = self.bottom_bar_rect.y + 24.0;
        let txt_c = [
            theme::LT_TEXT_PRIMARY[0],
            theme::LT_TEXT_PRIMARY[1],
            theme::LT_TEXT_PRIMARY[2],
            0.85,
        ];

        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: bl_x,
                y: bl_y,
                text: "DECODE".into(),
                font_size: 10.0,
                color: txt_c,
                bold: true,
                letter_spacing: 0.05,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );
        bl_x += 46.0;
        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: bl_x,
                y: bl_y,
                text: format!("{:.1}ms", self.stats_bar.stats.decode_time_ms),
                font_size: 11.0,
                color: theme::PRIMARY_BLUE,
                monospace: true,
                bold: true,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );
        bl_x += 50.0;

        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: bl_x,
                y: bl_y,
                text: "RES".into(),
                font_size: 10.0,
                color: txt_c,
                bold: true,
                letter_spacing: 0.05,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );
        bl_x += 24.0;
        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: bl_x,
                y: bl_y,
                text: format!(
                    "{}x{}",
                    self.stats_bar.stats.resolution.0, self.stats_bar.stats.resolution.1
                ),
                font_size: 11.0,
                color: theme::PRIMARY_BLUE,
                monospace: true,
                bold: true,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );
        bl_x += 80.0;

        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: bl_x,
                y: bl_y,
                text: "SECURE CHANNEL".into(),
                font_size: 10.0,
                color: theme::SUCCESS,
                bold: true,
                letter_spacing: 0.05,
                ..Default::default()
            },
            theme::CONTRAST_TEXT_SHADOW,
        );

        // Paint Disconnect Button (Bottom Corner)
        ctx.push_glow_rect(theme::signature_shadow(self.disconnect_rect, 28.0));
        ctx.push_glass_quad(theme::glass_quad(
            self.disconnect_rect,
            [theme::DANGER[0], theme::DANGER[1], theme::DANGER[2], 0.9],
            [theme::DANGER[0], theme::DANGER[1], theme::DANGER[2], 1.0],
            28.0, // Fully rounded
        ));
        theme::push_text_with_shadow(
            ctx,
            TextRun {
                x: self.disconnect_rect.x + 6.0,
                y: self.disconnect_rect.y + 34.0,
                text: "DISCONNECT".into(),
                font_size: 8.0,
                color: [1.0, 1.0, 1.0, 1.0],
                bold: true,
                ..Default::default()
            },
            [0.0, 0.0, 0.0, 0.12],
        );
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        if !self.visible {
            return EventResponse::Ignored;
        }

        match self.stats_bar.handle_event(event) {
            EventResponse::Action(UiAction::OpenPanel(name)) => {
                self.open_panel_by_name(&name);
                EventResponse::Consumed
            }
            EventResponse::Action(action) => EventResponse::Action(action),
            EventResponse::Consumed => EventResponse::Consumed,
            EventResponse::Ignored => {
                let panel_resp = match self.active_panel {
                    Some(CapsulePanel::Performance) => self.perf_panel.handle_event(event),
                    Some(CapsulePanel::Quality) => self.quality_panel.handle_event(event),
                    Some(CapsulePanel::Connection) => self.conn_panel.handle_event(event),
                    Some(CapsulePanel::Display) => self.display_panel.handle_event(event),
                    None => EventResponse::Ignored,
                };
                if !matches!(panel_resp, EventResponse::Ignored) {
                    return panel_resp;
                }

                if let UiEvent::MouseDown {
                    x,
                    y,
                    button: MouseButton::Left,
                } = event
                {
                    if self.disconnect_rect.contains(*x, *y) {
                        return EventResponse::Action(UiAction::Disconnect);
                    }
                    for (panel, rect) in &self.sidebar_nav_rects {
                        if rect.contains(*x, *y) {
                            self.set_active_panel(Some(*panel));
                            return EventResponse::Consumed;
                        }
                    }
                    if self.sidebar_rect.contains(*x, *y) {
                        return EventResponse::Consumed;
                    }
                    if let Some(panel_rect) = self.panel_rect {
                        if !panel_rect.contains(*x, *y)
                            && !self.capsule_rect.contains(*x, *y)
                            && !self.sidebar_rect.contains(*x, *y)
                            && !self.bottom_bar_rect.contains(*x, *y)
                        {
                            self.set_active_panel(None);
                            return EventResponse::Consumed;
                        }
                    } else {
                        // Clicked outside capsule when no panel open = dismiss overlay
                        if !self.capsule_rect.contains(*x, *y)
                            && !self.disconnect_rect.contains(*x, *y)
                            && !self.sidebar_rect.contains(*x, *y)
                            && !self.bottom_bar_rect.contains(*x, *y)
                        {
                            return EventResponse::Action(UiAction::CloseOverlay);
                        }
                    }
                }

                EventResponse::Ignored
            }
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.stats_bar.animate(dt_ms);
        self.perf_panel.animate(dt_ms);
        self.quality_panel.animate(dt_ms);
        self.conn_panel.animate(dt_ms);
        self.display_panel.animate(dt_ms);
    }
}
