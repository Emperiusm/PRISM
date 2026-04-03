// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lightweight Recent Connections list for the Home screen.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::servers::{SavedServer, ServerStatus};
use crate::ui::theme;
use crate::ui::widgets::button::{Button, ButtonStyle};
use crate::ui::widgets::{
    ColorMode, EventResponse, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};

const ROW_H: f32 = 54.0;
const ROW_PAD: f32 = 18.0;
const MAX_VISIBLE: usize = 5;

// ---------------------------------------------------------------------------
// RecentList
// ---------------------------------------------------------------------------

pub struct RecentList {
    rows: Vec<RecentRow>,
    rect: Rect,
    list_rect: Rect,
}

struct RecentRow {
    #[allow(dead_code)]
    server_id: uuid::Uuid,
    display_name: String,
    #[allow(dead_code)]
    address: String,
    status: ServerStatus,
    last_connected: Option<u64>,
    reconnect_button: Button,
    row_rect: Rect,
    hovered: bool,
}

impl RecentRow {
    fn from_saved(server: &SavedServer) -> Self {
        Self {
            server_id: server.id,
            display_name: server.display_name.clone(),
            address: server.address.clone(),
            status: server.derived_status(),
            last_connected: server.last_connected,
            reconnect_button: Button::new(
                "Reconnect",
                UiAction::Connect {
                    address: server.address.clone(),
                    noise_key: None,
                },
            )
            .with_style(ButtonStyle::Secondary)
            .with_color_mode(ColorMode::Light),
            row_rect: Rect::new(0.0, 0.0, 0.0, ROW_H),
            hovered: false,
        }
    }

    fn relative_time(&self) -> String {
        match self.last_connected {
            Some(epoch_secs) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let delta = now.saturating_sub(epoch_secs);
                if delta < 60 * 60 {
                    format!("{} min ago", (delta / 60).max(1))
                } else if delta < 24 * 60 * 60 {
                    format!("{} hours ago", delta / (60 * 60))
                } else {
                    format!("{} days ago", delta / (24 * 60 * 60))
                }
            }
            None => "Never connected".to_string(),
        }
    }

    fn status_label(&self) -> &'static str {
        match self.status {
            ServerStatus::Online => "Online",
            ServerStatus::Sleeping => "Sleeping",
            ServerStatus::Unreachable => "Unreachable",
        }
    }

    fn chip_tone(&self) -> theme::ChipTone {
        match self.status {
            ServerStatus::Online => theme::ChipTone::Success,
            ServerStatus::Sleeping => theme::ChipTone::Warning,
            ServerStatus::Unreachable => theme::ChipTone::Danger,
        }
    }
}

impl RecentList {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            list_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn set_servers(&mut self, servers: &[SavedServer]) {
        let mut ordered = servers.to_vec();
        ordered.sort_by(|a, b| {
            b.last_connected
                .unwrap_or(b.created_at)
                .cmp(&a.last_connected.unwrap_or(a.created_at))
        });
        self.rows = ordered
            .iter()
            .take(MAX_VISIBLE)
            .map(|s| RecentRow::from_saved(s))
            .collect();
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl Default for RecentList {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for RecentList {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;

        let list_h = if self.rows.is_empty() {
            60.0
        } else {
            ROW_PAD * 2.0 + self.rows.len() as f32 * ROW_H
        };
        self.list_rect = Rect::new(available.x, available.y, available.w, list_h);

        let btn_w = 100.0;
        let btn_h = 30.0;

        for (i, row) in self.rows.iter_mut().enumerate() {
            let row_y = self.list_rect.y + ROW_PAD + i as f32 * ROW_H;
            row.row_rect = Rect::new(
                self.list_rect.x + ROW_PAD,
                row_y,
                self.list_rect.w - ROW_PAD * 2.0,
                ROW_H,
            );
            // Position reconnect button at right edge of row
            let btn_x = row.row_rect.x + row.row_rect.w - btn_w;
            let btn_y = row_y + (ROW_H - btn_h) * 0.5;
            row.reconnect_button
                .layout(Rect::new(btn_x, btn_y, btn_w, btn_h));
        }

        Size {
            w: available.w,
            h: list_h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // List container
        ctx.push_glass_quad(theme::launcher_list_surface(self.list_rect));

        if self.rows.is_empty() {
            // Empty state
            let msg = "No recent connections.";
            let msg_w = theme::text_width(msg, theme::FONT_BODY);
            ctx.push_text_run(TextRun {
                x: self.list_rect.x + (self.list_rect.w - msg_w) * 0.5,
                y: self.list_rect.y + 20.0,
                text: msg.into(),
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
            return;
        }

        for (i, row) in self.rows.iter().enumerate() {
            // Row hover surface
            ctx.push_glass_quad(theme::launcher_row_surface(row.row_rect, row.hovered));

            let row_y = row.row_rect.y;
            let text_y = row_y + 18.0;

            // (a) Numbered index
            let idx_text = format!("{}", i + 1);
            ctx.push_text_run(TextRun {
                x: row.row_rect.x + 4.0,
                y: text_y,
                text: idx_text,
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });

            // (b) Server name (bold)
            ctx.push_text_run(TextRun {
                x: row.row_rect.x + 32.0,
                y: text_y,
                text: row.display_name.clone(),
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_PRIMARY,
                bold: true,
                ..Default::default()
            });

            // (c) Status chip
            let label = row.status_label();
            let chip_w = theme::text_width(label, theme::FONT_CHIP) + 20.0;
            let chip_rect = Rect::new(
                row.row_rect.x + 200.0,
                row_y + (ROW_H - 22.0) * 0.5,
                chip_w,
                22.0,
            );
            ctx.push_glass_quad(theme::launcher_status_chip(chip_rect, row.chip_tone()));
            ctx.push_text_run(TextRun {
                x: chip_rect.x + 10.0,
                y: chip_rect.y + 4.0,
                text: label.into(),
                font_size: theme::FONT_CHIP,
                color: theme::launcher_chip_text_color(row.chip_tone()),
                ..Default::default()
            });

            // (d) Last connected timestamp
            let time_text = row.relative_time();
            let time_x = chip_rect.x + chip_rect.w + 16.0;
            ctx.push_text_run(TextRun {
                x: time_x,
                y: text_y,
                text: time_text,
                font_size: theme::FONT_LABEL,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });

            // (e) Reconnect button with sync icon to the left
            row.reconnect_button.paint(ctx);

            // Inner separator (except last row)
            if i + 1 < self.rows.len() {
                let sep_y = row_y + ROW_H - 0.5;
                ctx.push_glass_quad(theme::launcher_inner_separator(Rect::new(
                    row.row_rect.x,
                    sep_y,
                    row.row_rect.w,
                    1.0,
                )));
            }
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Update hover state
        if let UiEvent::MouseMove { x, y } = event {
            for row in &mut self.rows {
                row.hovered = row.row_rect.contains(*x, *y);
            }
        }

        // Delegate to reconnect buttons
        for row in &mut self.rows {
            let resp = row.reconnect_button.handle_event(event);
            if !matches!(resp, EventResponse::Ignored) {
                return resp;
            }
        }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        for row in &mut self.rows {
            row.reconnect_button.animate(dt_ms);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_servers() -> Vec<SavedServer> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        vec![
            {
                let mut s = SavedServer::new("Work Desktop", "10.0.0.5:7000");
                s.last_connected = Some(now - 3600); // 1 hour ago → Online
                s
            },
            {
                let mut s = SavedServer::new("Home Server", "192.168.1.100:7000");
                s.last_connected = Some(now - 3 * 24 * 3600); // 3 days ago → Sleeping
                s
            },
            {
                let s = SavedServer::new("Lab Machine", "10.10.5.20:7000");
                // Never connected → Unreachable
                s
            },
        ]
    }

    #[test]
    fn recent_list_renders_rows() {
        let mut list = RecentList::new();
        list.set_servers(&make_servers());
        list.layout(Rect::new(0.0, 0.0, 800.0, 400.0));

        let mut ctx = PaintContext::new();
        list.paint(&mut ctx);

        // Should have list surface + row surfaces + chip surfaces + separators
        assert!(!ctx.glass_quads.is_empty());
        // Should have index + name + chip label + time + button label text runs per row
        assert!(ctx.text_runs.len() >= 9);
    }

    #[test]
    fn recent_list_empty_state() {
        let mut list = RecentList::new();
        list.set_servers(&[]);
        list.layout(Rect::new(0.0, 0.0, 800.0, 400.0));

        let mut ctx = PaintContext::new();
        list.paint(&mut ctx);

        // Should render empty state message
        assert!(
            ctx.text_runs
                .iter()
                .any(|r| r.text.contains("No recent connections"))
        );
    }

    #[test]
    fn recent_list_limits_to_five() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let servers: Vec<SavedServer> = (0..10)
            .map(|i| {
                let mut s = SavedServer::new(format!("Server {}", i), format!("10.0.0.{}:7000", i));
                s.last_connected = Some(now - i * 3600);
                s
            })
            .collect();

        let mut list = RecentList::new();
        list.set_servers(&servers);
        assert_eq!(list.rows.len(), 5);
    }
}
