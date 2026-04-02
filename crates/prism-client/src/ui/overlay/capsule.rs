// SPDX-License-Identifier: AGPL-3.0-or-later
//! Top capsule overlay with single-panel expansion.

use super::conn_panel::ConnPanel;
use super::display_panel::DisplayPanel;
use super::perf_panel::PerfPanel;
use super::quality_panel::QualityPanel;
use super::stats_bar::{SessionStats, StatsBar};
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, UiAction, UiEvent, Widget,
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
    panel_rect: Option<Rect>,
    visible: bool,
    active_panel: Option<CapsulePanel>,
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
            panel_rect: None,
            visible: false,
            active_panel: None,
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

        let capsule_w = (available.w - 40.0).clamp(320.0, 660.0);
        let capsule_x = available.x + (available.w - capsule_w) * 0.5;
        self.capsule_rect = Rect::new(capsule_x, available.y + 18.0, capsule_w, 48.0);
        self.stats_bar.layout(self.capsule_rect);

        self.panel_rect = None;
        if let Some(panel) = self.active_panel {
            let panel_w = 260.0;
            let panel_x = capsule_x + (capsule_w - panel_w) * 0.5;
            let panel_y = self.capsule_rect.y + self.capsule_rect.h + 18.0;
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
        match self.active_panel {
            Some(CapsulePanel::Performance) => self.perf_panel.paint(ctx),
            Some(CapsulePanel::Quality) => self.quality_panel.paint(ctx),
            Some(CapsulePanel::Connection) => self.conn_panel.paint(ctx),
            Some(CapsulePanel::Display) => self.display_panel.paint(ctx),
            None => {}
        }
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
                    && let Some(panel_rect) = self.panel_rect
                    && !panel_rect.contains(*x, *y)
                    && !self.capsule_rect.contains(*x, *y)
                {
                    self.set_active_panel(None);
                    return EventResponse::Consumed;
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
