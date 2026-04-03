// SPDX-License-Identifier: AGPL-3.0-or-later
//! Display sub-panel — monitor selection, resolution, refresh rate.

use crate::ui::theme;
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::label::Label;
use crate::ui::widgets::monitor_map::{MonitorInfo, MonitorMap};
use crate::ui::widgets::{
    EventResponse, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct DisplayPanel {
    monitor_dropdown: Dropdown,
    resolution_label: Label,
    refresh_label: Label,
    monitor_map: MonitorMap,
    rect: Rect,
    visible: bool,
}

const PANEL_W: f32 = 260.0;
const PANEL_H: f32 = 220.0;

impl DisplayPanel {
    pub fn new() -> Self {
        Self {
            monitor_dropdown: Dropdown::new(vec!["0: Primary".into()], 0),
            resolution_label: Label::new("Resolution: —", 12.0),
            refresh_label: Label::new("Refresh: —", 12.0),
            monitor_map: MonitorMap::new(vec![], 0),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            visible: false,
        }
    }

    pub fn set_monitors(&mut self, monitors: Vec<MonitorInfo>) {
        // Update dropdown options
        let options: Vec<String> = monitors
            .iter()
            .map(|m| {
                if m.is_primary {
                    format!("{}: Primary", m.index)
                } else {
                    format!("{}: Monitor", m.index)
                }
            })
            .collect();

        // Preserve or reset selection
        let selected = self
            .monitor_dropdown
            .selected_index()
            .min(options.len().saturating_sub(1));
        self.monitor_dropdown = Dropdown::new(options, selected);

        // Update resolution/refresh labels from the selected monitor (if any)
        if let Some(mon) = monitors.get(selected) {
            self.resolution_label
                .set_text(&format!("{}×{}", mon.width, mon.height));
            // Refresh not in MonitorInfo — placeholder
            self.refresh_label.set_text("Refresh: —");
        }

        self.monitor_map.set_monitors(monitors);
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

    fn layout_children(&mut self) {
        let pad = 8.0;
        let inner_w = PANEL_W - pad * 2.0;
        let x = self.rect.x + pad;
        let mut cur_y = self.rect.y + 28.0;

        self.monitor_dropdown
            .layout(Rect::new(x, cur_y, inner_w, 32.0));
        cur_y += 36.0;
        self.resolution_label
            .layout(Rect::new(x, cur_y, inner_w, 16.0));
        cur_y += 20.0;
        self.refresh_label
            .layout(Rect::new(x, cur_y, inner_w, 16.0));
        cur_y += 20.0;
        self.monitor_map.layout(Rect::new(x, cur_y, inner_w, 80.0));
    }
}

impl Default for DisplayPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for DisplayPanel {
    fn layout(&mut self, available: Rect) -> Size {
        if !self.visible {
            return Size { w: 0.0, h: 0.0 };
        }
        self.rect = Rect::new(available.x, available.y, PANEL_W, PANEL_H);
        self.layout_children();
        Size {
            w: PANEL_W,
            h: PANEL_H,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        if !self.visible {
            return;
        }

        ctx.push_glass_quad(theme::floating_surface(self.rect));

        ctx.push_text_run(TextRun {
            x: self.rect.x + 12.0,
            y: self.rect.y + 10.0,
            text: "Display".into(),
            font_size: 13.0,
            color: theme::TEXT_PRIMARY,
            ..Default::default()
        });

        self.monitor_dropdown.paint(ctx);
        self.resolution_label.paint(ctx);
        self.refresh_label.paint(ctx);
        self.monitor_map.paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Monitor map selection
        let old_map_selected = self.monitor_map.selected();
        let r = self.monitor_map.handle_event(event);
        if self.monitor_map.selected() != old_map_selected {
            return EventResponse::Action(UiAction::SelectMonitor(self.monitor_map.selected()));
        }
        if matches!(r, EventResponse::Consumed) {
            return r;
        }

        // Dropdown selection
        let old_dd = self.monitor_dropdown.selected_index();
        let r = self.monitor_dropdown.handle_event(event);
        if self.monitor_dropdown.selected_index() != old_dd {
            return EventResponse::Action(UiAction::SelectMonitor(
                self.monitor_dropdown.selected_index() as u8,
            ));
        }
        if matches!(r, EventResponse::Consumed) {
            return r;
        }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        self.monitor_dropdown.animate(dt_ms);
        self.monitor_map.animate(dt_ms);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn two_monitors() -> Vec<MonitorInfo> {
        vec![
            MonitorInfo {
                index: 0,
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                is_primary: true,
            },
            MonitorInfo {
                index: 1,
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
                is_primary: false,
            },
        ]
    }

    #[test]
    fn display_panel_paints() {
        let mut panel = DisplayPanel::new();
        panel.show();
        panel.set_monitors(two_monitors());
        panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        let texts: Vec<&str> = ctx.text_runs.iter().map(|t| t.text.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains("Display")),
            "expected Display title, got: {texts:?}"
        );
        assert!(!ctx.glass_quads.is_empty(), "expected glass quads");
    }

    #[test]
    fn display_panel_hidden() {
        let mut panel = DisplayPanel::new();
        let size = panel.layout(Rect::new(0.0, 0.0, 400.0, 400.0));
        assert!((size.w).abs() < 0.01 && (size.h).abs() < 0.01);
        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);
        assert!(ctx.glass_quads.is_empty());
        assert!(ctx.text_runs.is_empty());
    }
}
