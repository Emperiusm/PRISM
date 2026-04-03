// SPDX-License-Identifier: AGPL-3.0-or-later
//! Multi-monitor layout map widget.

use super::{EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiEvent, Widget};
use crate::ui::theme;

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub index: u8,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

pub struct MonitorMap {
    monitors: Vec<MonitorInfo>,
    selected: u8,
    rect: Rect,
}

impl MonitorMap {
    pub fn new(monitors: Vec<MonitorInfo>, selected: u8) -> Self {
        Self {
            monitors,
            selected,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn selected(&self) -> u8 {
        self.selected
    }

    pub fn set_monitors(&mut self, monitors: Vec<MonitorInfo>) {
        self.monitors = monitors;
    }

    /// Compute the scaled rect for a given monitor within the widget bounds.
    pub fn scaled_rect(&self, mon: &MonitorInfo) -> Rect {
        if self.monitors.is_empty() {
            return Rect::new(self.rect.x, self.rect.y, 0.0, 0.0);
        }

        // Compute bounding box of all monitors
        let min_x = self.monitors.iter().map(|m| m.x).min().unwrap_or(0);
        let min_y = self.monitors.iter().map(|m| m.y).min().unwrap_or(0);
        let max_x = self
            .monitors
            .iter()
            .map(|m| m.x + m.width as i32)
            .max()
            .unwrap_or(1);
        let max_y = self
            .monitors
            .iter()
            .map(|m| m.y + m.height as i32)
            .max()
            .unwrap_or(1);

        let total_w = (max_x - min_x) as f32;
        let total_h = (max_y - min_y) as f32;

        // Scale to fit widget rect at 80%
        let avail_w = self.rect.w * 0.8;
        let avail_h = self.rect.h * 0.8;

        let scale = if total_w <= 0.0 || total_h <= 0.0 {
            1.0
        } else {
            (avail_w / total_w).min(avail_h / total_h)
        };

        let scaled_total_w = total_w * scale;
        let scaled_total_h = total_h * scale;

        // Center within widget rect
        let offset_x = self.rect.x + (self.rect.w - scaled_total_w) * 0.5;
        let offset_y = self.rect.y + (self.rect.h - scaled_total_h) * 0.5;

        let x = offset_x + (mon.x - min_x) as f32 * scale;
        let y = offset_y + (mon.y - min_y) as f32 * scale;
        let w = mon.width as f32 * scale;
        let h = mon.height as f32 * scale;

        Rect::new(x, y, w, h)
    }
}

impl Widget for MonitorMap {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 80.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        for mon in &self.monitors {
            let r = self.scaled_rect(mon);
            let is_selected = mon.index == self.selected;

            ctx.push_glass_quad(theme::glass_quad(
                r,
                if is_selected {
                    [0.22, 0.30, 0.39, 0.94]
                } else {
                    [0.14, 0.18, 0.24, 0.86]
                },
                if is_selected {
                    [theme::ACCENT[0], theme::ACCENT[1], theme::ACCENT[2], 0.34]
                } else {
                    [1.0, 1.0, 1.0, 0.12]
                },
                6.0,
            ));

            // Index number centered
            let label = mon.index.to_string();
            ctx.push_text_run(TextRun {
                x: r.x + (r.w - theme::text_width(&label, 11.0)) * 0.5,
                y: r.y + (r.h - 11.0) * 0.5 - 1.0,
                text: label,
                font_size: 11.0,
                color: if is_selected {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                },
                ..Default::default()
            });
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                for mon in &self.monitors {
                    if self.scaled_rect(mon).contains(*x, *y) {
                        self.selected = mon.index;
                        return EventResponse::Consumed;
                    }
                }
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, _dt_ms: f32) {}
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
    fn click_selects_monitor() {
        let mut map = MonitorMap::new(two_monitors(), 0);
        map.layout(Rect::new(0.0, 0.0, 400.0, 80.0));

        // The right monitor's scaled rect should be in the right half of the widget.
        // With two equal-width monitors side by side, the midpoint of right monitor
        // is at roughly x = 0 + (400 * 0.8) * 0.75 + center_offset ~ right half.
        // Let's compute the actual rect and click its center.
        let right_mon = &map.monitors[1].clone();
        let r = map.scaled_rect(right_mon);
        let center_x = r.x + r.w * 0.5;
        let center_y = r.y + r.h * 0.5;

        let resp = map.handle_event(&UiEvent::MouseDown {
            x: center_x,
            y: center_y,
            button: MouseButton::Left,
        });

        assert!(matches!(resp, EventResponse::Consumed));
        assert_eq!(map.selected(), 1, "expected monitor 1 selected");
    }

    #[test]
    fn renders_quads_per_monitor() {
        let mut map = MonitorMap::new(two_monitors(), 0);
        map.layout(Rect::new(0.0, 0.0, 400.0, 80.0));

        let mut ctx = PaintContext::new();
        map.paint(&mut ctx);

        assert_eq!(
            ctx.glass_quads.len(),
            2,
            "expected 2 glass quads for 2 monitors"
        );
        assert_eq!(
            ctx.text_runs.len(),
            2,
            "expected 2 text runs for monitor indices"
        );
    }
}
