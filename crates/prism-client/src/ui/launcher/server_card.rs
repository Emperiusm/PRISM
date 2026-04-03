// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saved server card with status chips and stronger actions.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::servers::{SavedServer, ServerStatus};
use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::button::{Button, ButtonStyle};
use crate::ui::widgets::icon::{ICON_EDIT, ICON_HEART, ICON_MORE_VERT, Icon};
use crate::ui::widgets::{
    ColorMode, EventResponse, GlassQuad, MouseButton, PaintContext, Rect, Size, TextRun, UiAction,
    UiEvent, Widget,
};

const WEEK_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardLayoutMode {
    Card,
    Row,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardFilter {
    All,
    Recent,
    Dormant,
    New,
    Tag(String), // e.g. "WORK", "PERSONAL"
}

impl CardFilter {
    pub fn label(&self) -> String {
        match self {
            Self::All => "All Hosts".into(),
            Self::Recent => "Recent".into(),
            Self::Dormant => "Dormant".into(),
            Self::New => "New".into(),
            Self::Tag(t) => t.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStatus {
    Recent,
    Dormant,
    New,
}

pub struct ServerCard {
    server_id: uuid::Uuid,
    display_name: String,
    address: String,
    _last_profile: String,
    last_connected: Option<u64>,
    _last_info: String,
    accent_color: [f32; 3],
    tags: Vec<String>,
    os_label: Option<String>,
    wol_supported: bool,
    last_latency_ms: Option<u32>,
    server_status: ServerStatus,
    connect_button: Button,
    edit_button: Button,
    delete_button: Button,
    hover_anim: Animation,
    hovered: bool,
    layout_mode: CardLayoutMode,
    index: Option<usize>,
    rect: Rect,
}

impl ServerCard {
    pub const WIDTH: f32 = 282.0;
    pub const HEIGHT: f32 = 198.0;

    pub fn from_saved(server: &SavedServer) -> Self {
        let accent_color = [
            server.accent_color[0] as f32 / 255.0,
            server.accent_color[1] as f32 / 255.0,
            server.accent_color[2] as f32 / 255.0,
        ];

        let last_info = match (&server.last_codec, &server.last_resolution) {
            (Some(codec), Some((w, h))) => format!("{codec} • {w}x{h}"),
            (Some(codec), None) => codec.clone(),
            _ => String::from("No previous session details"),
        };

        let status = server.derived_status();
        let (connect_label, connect_style) = match status {
            ServerStatus::Online => ("Connect", ButtonStyle::Primary),
            ServerStatus::Sleeping => ("Wake & Connect", ButtonStyle::Secondary),
            ServerStatus::Unreachable => ("Retry Discovery", ButtonStyle::Secondary),
        };

        Self {
            server_id: server.id,
            display_name: server.display_name.clone(),
            address: server.address.clone(),
            _last_profile: if server.default_profile.is_empty() {
                "Default".to_string()
            } else {
                server.default_profile.clone()
            },
            last_connected: server.last_connected,
            _last_info: last_info,
            accent_color,
            tags: server.tags.clone(),
            os_label: server.os_label.clone(),
            wol_supported: server.wol_supported,
            last_latency_ms: server.last_latency_ms,
            server_status: status,
            connect_button: Button::new(
                connect_label,
                UiAction::Connect {
                    address: server.address.clone(),
                    noise_key: None,
                },
            )
            .with_style(connect_style)
            .with_color_mode(ColorMode::Light),
            edit_button: Button::new("Edit", UiAction::EditServer(server.id))
                .with_style(ButtonStyle::Secondary)
                .with_color_mode(ColorMode::Light),
            delete_button: Button::new("Del", UiAction::DeleteServer(server.id))
                .with_style(ButtonStyle::Destructive)
                .with_color_mode(ColorMode::Light),
            hover_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            hovered: false,
            layout_mode: CardLayoutMode::Card,
            index: None,
            rect: Rect::new(0.0, 0.0, Self::WIDTH, Self::HEIGHT),
        }
    }

    pub fn with_index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    pub fn with_layout_mode(mut self, mode: CardLayoutMode) -> Self {
        self.layout_mode = mode;
        self
    }

    pub fn set_layout_mode(&mut self, mode: CardLayoutMode) {
        self.layout_mode = mode;
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn matches_filter(&self, filter: &CardFilter) -> bool {
        match filter {
            CardFilter::All => true,
            CardFilter::Recent => self.status() == CardStatus::Recent,
            CardFilter::Dormant => self.status() == CardStatus::Dormant,
            CardFilter::New => self.status() == CardStatus::New,
            CardFilter::Tag(tag) => self.tags.contains(tag),
        }
    }

    pub fn status(&self) -> CardStatus {
        match self.last_connected {
            Some(last_connected) => {
                let now_secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if now_secs.saturating_sub(last_connected) <= WEEK_SECS {
                    CardStatus::Recent
                } else {
                    CardStatus::Dormant
                }
            }
            None => CardStatus::New,
        }
    }

    fn server_status_label(&self) -> &'static str {
        match self.server_status {
            ServerStatus::Online => "Online",
            ServerStatus::Sleeping => "Sleeping",
            ServerStatus::Unreachable => "Unreachable",
        }
    }

    fn server_status_tone(&self) -> theme::ChipTone {
        match self.server_status {
            ServerStatus::Online => theme::ChipTone::Success,
            ServerStatus::Sleeping => theme::ChipTone::Warning,
            ServerStatus::Unreachable => theme::ChipTone::Danger,
        }
    }

    fn buttons_y(&self) -> f32 {
        if self.layout_mode == CardLayoutMode::Row {
            self.rect.y + 12.0
        } else {
            self.rect.y + self.rect.h - 52.0
        }
    }

    fn relative_last_connected(&self) -> String {
        match self.last_connected {
            Some(last_connected) => {
                let now_secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let delta = now_secs.saturating_sub(last_connected);
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

    fn paint_row(&self, ctx: &mut PaintContext, r: Rect, hover: f32) {
        let accent = [
            self.accent_color[0],
            self.accent_color[1],
            self.accent_color[2],
            1.0,
        ];

        ctx.push_glass_quad(theme::launcher_row_surface(r, hover > 0.01));

        let status_label = self.server_status_label();
        let status_w = theme::text_width(status_label, 10.0) + 26.0;
        let status_rect = Rect::new(r.x + 220.0, r.y + 22.0, status_w, 20.0);

        ctx.push_glass_quad(theme::launcher_status_chip(
            status_rect,
            self.server_status_tone(),
        ));
        ctx.push_text_run(TextRun {
            x: status_rect.x + 12.0,
            y: status_rect.y + 4.0,
            text: status_label.to_string(),
            font_size: 10.0,
            color: theme::launcher_chip_text_color(self.server_status_tone()),
            ..Default::default()
        });

        ctx.push_glass_quad(theme::glass_quad(
            Rect::new(r.x + 18.0, r.y + 16.0, 32.0, 32.0),
            [accent[0], accent[1], accent[2], 0.1],
            [0.0, 0.0, 0.0, 0.0],
            8.0,
        ));
        let index_text = format!("{:02}", self.index.unwrap_or(0));
        let idx_w = theme::text_width(&index_text, 14.0);
        ctx.push_text_run(TextRun {
            x: r.x + 18.0 + (32.0 - idx_w) * 0.5,
            y: r.y + 26.0,
            text: index_text,
            font_size: 14.0,
            color: accent,
            monospace: true,
            ..Default::default()
        });
        ctx.push_text_run(TextRun {
            x: r.x + 64.0,
            y: r.y + 16.0,
            text: self.display_name.clone(),
            font_size: 14.0,
            color: theme::LT_TEXT_PRIMARY,
            ..Default::default()
        });
        ctx.push_text_run(TextRun {
            x: r.x + 64.0,
            y: r.y + 36.0,
            text: self.address.clone(),
            font_size: 11.0,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });

        let status_end = status_rect.x + status_rect.w;
        ctx.push_text_run(TextRun {
            x: status_end + 32.0,
            y: r.y + 24.0,
            text: self.relative_last_connected(),
            font_size: 12.0,
            color: theme::LT_TEXT_SECONDARY,
            ..Default::default()
        });

        self.connect_button.paint(ctx);
        self.edit_button.paint(ctx);
        self.delete_button.paint(ctx);
    }

    fn append_shifted(ctx: &mut PaintContext, mut source: PaintContext, dy: f32) {
        for mut quad in source.glass_quads.drain(..) {
            quad.rect.y -= dy;
            quad.blur_rect.y -= dy;
            ctx.push_glass_quad(quad);
        }
        for mut glow in source.glow_rects.drain(..) {
            glow.rect.y -= dy;
            ctx.push_glow_rect(glow);
        }
        for mut run in source.text_runs.drain(..) {
            run.y -= dy;
            ctx.push_text_run(run);
        }
    }
}

impl Widget for ServerCard {
    fn layout(&mut self, available: Rect) -> Size {
        const ACTION_GAP: f32 = 8.0;
        const SECONDARY_W: f32 = 62.0;

        let h = match self.layout_mode {
            CardLayoutMode::Card => Self::HEIGHT,
            CardLayoutMode::Row => 64.0,
        };
        let w = match self.layout_mode {
            CardLayoutMode::Card => Self::WIDTH,
            CardLayoutMode::Row => available.w,
        };

        self.rect = Rect::new(available.x, available.y, w, h);
        let button_y = self.buttons_y();

        if self.layout_mode == CardLayoutMode::Row {
            let delete_x = self.rect.x + self.rect.w - 18.0 - SECONDARY_W;
            let edit_x = delete_x - ACTION_GAP - SECONDARY_W;
            let connect_x = edit_x - ACTION_GAP - 90.0;
            self.connect_button
                .layout(Rect::new(connect_x, button_y, 90.0, 40.0));
            self.edit_button
                .layout(Rect::new(edit_x, button_y, SECONDARY_W, 40.0));
            self.delete_button
                .layout(Rect::new(delete_x, button_y, SECONDARY_W, 40.0));
        } else {
            // Card mode: action button + edit icon in footer
            let edit_icon_w = 32.0;
            let connect_x = self.rect.x + 12.0;
            let edit_x = self.rect.x + self.rect.w - 12.0 - edit_icon_w;
            let connect_w = (edit_x - ACTION_GAP - connect_x).max(80.0);
            self.connect_button
                .layout(Rect::new(connect_x, button_y, connect_w, 32.0));
            self.edit_button
                .layout(Rect::new(edit_x, button_y, edit_icon_w, 32.0));
            // delete button not shown in card grid view — keep offscreen
            self.delete_button
                .layout(Rect::new(-100.0, -100.0, 0.0, 0.0));
        }

        Size { w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let base_rect = self.rect;
        let hover = self.hover_anim.value();
        let is_unreachable = self.server_status == ServerStatus::Unreachable;
        let card_alpha = if is_unreachable { 0.80 } else { 1.0 };

        if self.layout_mode == CardLayoutMode::Row {
            self.paint_row(ctx, base_rect, hover);
            return;
        }

        let lift = hover * 2.0;
        let r = Rect::new(base_rect.x, base_rect.y - lift, base_rect.w, base_rect.h);
        if hover > 0.01 {
            ctx.push_glow_rect(theme::hover_elevation_shadow(r, theme::CARD_RADIUS, hover));
        } else {
            ctx.push_glow_rect(theme::signature_shadow(r, theme::CARD_RADIUS));
        }

        // ── Card surface ──
        let mut card_quad = theme::launcher_card_surface(r);
        card_quad.tint[3] *= card_alpha;
        ctx.push_glass_quad(card_quad);
        if hover > 0.01 {
            ctx.push_glass_quad(theme::launcher_card_hover(r));
        }

        // ── Hero placeholder area (top ~55%) ──
        let hero_h = (r.h * 0.55).round();
        let hero_rect = Rect::new(r.x, r.y, r.w, hero_h);
        ctx.push_glass_quad(GlassQuad {
            rect: hero_rect,
            tint: [0.85, 0.90, 0.95, card_alpha],
            corner_radius: theme::CARD_RADIUS,
            ..Default::default()
        });

        // ── Badges over hero ──
        let badge_y = r.y + 8.0;
        let badge_x = r.x + 8.0;

        // Status badge
        let status_label = self.server_status_label();
        let status_w = theme::text_width(status_label, 10.0) + 20.0;
        let status_rect = Rect::new(badge_x, badge_y, status_w, 20.0);
        ctx.push_glass_quad(theme::launcher_status_chip(
            status_rect,
            self.server_status_tone(),
        ));
        ctx.push_text_run(TextRun {
            x: status_rect.x + 10.0,
            y: status_rect.y + 4.0,
            text: status_label.to_string(),
            font_size: 10.0,
            color: theme::launcher_chip_text_color(self.server_status_tone()),
            ..Default::default()
        });

        // Tag badge (first tag, if any)
        if let Some(tag) = self.tags.first() {
            let tag_upper = tag.to_uppercase();
            let tag_w = theme::text_width(&tag_upper, 10.0) + 20.0;
            let tag_rect = Rect::new(status_rect.x + status_rect.w + 4.0, badge_y, tag_w, 20.0);
            ctx.push_glass_quad(theme::glass_quad(
                tag_rect,
                [1.0, 1.0, 1.0, 0.70],
                [1.0, 1.0, 1.0, 0.40],
                theme::CHIP_RADIUS,
            ));
            ctx.push_text_run(TextRun {
                x: tag_rect.x + 10.0,
                y: tag_rect.y + 4.0,
                text: tag_upper,
                font_size: 10.0,
                color: theme::LT_TEXT_SECONDARY,
                letter_spacing: 0.05,
                ..Default::default()
            });
        }

        // Heart icon (top-right)
        Icon::new(ICON_HEART)
            .with_size(16.0)
            .with_color([1.0, 1.0, 1.0, 0.80])
            .at(r.x + r.w - 24.0, badge_y + 2.0)
            .paint(ctx);

        // ── Card body (below hero) ──
        let body_y = r.y + hero_h + 6.0;
        let body_pad = 12.0;

        // Server name (bold) + kebab menu
        let name_color = if is_unreachable {
            theme::LT_TEXT_MUTED
        } else {
            theme::LT_TEXT_PRIMARY
        };
        ctx.push_text_run(TextRun {
            x: r.x + body_pad,
            y: body_y,
            text: self.display_name.clone(),
            font_size: 13.0,
            color: name_color,
            bold: true,
            ..Default::default()
        });

        // Kebab menu (⋮)
        Icon::new(ICON_MORE_VERT)
            .with_size(16.0)
            .with_color(theme::LT_TEXT_MUTED)
            .at(r.x + r.w - body_pad - 16.0, body_y)
            .paint(ctx);

        // OS + IP subtitle
        let subtitle = match &self.os_label {
            Some(os) => format!("{os} • {}", self.address),
            None => self.address.clone(),
        };
        ctx.push_text_run(TextRun {
            x: r.x + body_pad,
            y: body_y + 16.0,
            text: subtitle,
            font_size: 10.0,
            color: theme::LT_TEXT_MUTED,
            letter_spacing: 0.05,
            ..Default::default()
        });

        // Last connected timestamp + latency chip
        let mut info_x = r.x + body_pad;
        let info_y = body_y + 30.0;
        ctx.push_text_run(TextRun {
            x: info_x,
            y: info_y,
            text: self.relative_last_connected(),
            font_size: 10.0,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });

        info_x += theme::text_width(&self.relative_last_connected(), 10.0) + 8.0;

        // Latency chip
        if let Some(ms) = self.last_latency_ms {
            let lat_text = format!("{ms}ms");
            let lat_w = theme::text_width(&lat_text, 9.0) + 14.0;
            let lat_rect = Rect::new(info_x, info_y - 1.0, lat_w, 16.0);
            ctx.push_glass_quad(theme::glass_quad(
                lat_rect,
                [1.0, 1.0, 1.0, 0.50],
                [0.0, 0.0, 0.0, 0.06],
                8.0,
            ));
            ctx.push_text_run(TextRun {
                x: lat_rect.x + 7.0,
                y: lat_rect.y + 2.0,
                text: lat_text,
                font_size: 9.0,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });
            info_x += lat_w + 6.0;
        }

        // WOL chip (for sleeping servers)
        if self.wol_supported && self.server_status == ServerStatus::Sleeping {
            let wol_text = "WOL";
            let wol_w = theme::text_width(wol_text, 9.0) + 14.0;
            let wol_rect = Rect::new(info_x, info_y - 1.0, wol_w, 16.0);
            ctx.push_glass_quad(theme::glass_quad(
                wol_rect,
                [1.0, 1.0, 1.0, 0.50],
                [0.0, 0.0, 0.0, 0.06],
                8.0,
            ));
            ctx.push_text_run(TextRun {
                x: wol_rect.x + 7.0,
                y: wol_rect.y + 2.0,
                text: wol_text.to_string(),
                font_size: 9.0,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });
        }

        // ── Footer: action button + edit icon ──
        let mut connect_ctx = PaintContext::new();
        self.connect_button.paint(&mut connect_ctx);
        Self::append_shifted(ctx, connect_ctx, lift);
        // Edit icon button
        let edit_rect = self.edit_button.rect();
        let shifted_edit_rect =
            Rect::new(edit_rect.x, edit_rect.y - lift, edit_rect.w, edit_rect.h);
        let mut edit_ctx = PaintContext::new();
        self.edit_button.paint(&mut edit_ctx);
        Self::append_shifted(ctx, edit_ctx, lift);
        Icon::new(ICON_EDIT)
            .with_size(16.0)
            .with_color(theme::LT_TEXT_SECONDARY)
            .at(shifted_edit_rect.x + 8.0, shifted_edit_rect.y + 8.0)
            .paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        let connect_resp = self.connect_button.handle_event(event);
        if !matches!(connect_resp, EventResponse::Ignored) {
            return connect_resp;
        }

        let edit_resp = self.edit_button.handle_event(event);
        if !matches!(edit_resp, EventResponse::Ignored) {
            return edit_resp;
        }

        let delete_resp = self.delete_button.handle_event(event);
        if !matches!(delete_resp, EventResponse::Ignored) {
            return delete_resp;
        }

        match event {
            UiEvent::MouseMove { x, y } => {
                let was = self.hovered;
                self.hovered = self.rect.contains(*x, *y);
                if self.hovered != was {
                    self.hover_anim
                        .set_target(if self.hovered { 1.0 } else { 0.0 });
                }
                EventResponse::Ignored
            }
            UiEvent::MouseDown { x, y, button } => {
                if !self.rect.contains(*x, *y) {
                    return EventResponse::Ignored;
                }

                match button {
                    MouseButton::Left => EventResponse::Action(UiAction::Connect {
                        address: self.address.clone(),
                        noise_key: None,
                    }),
                    MouseButton::Right => {
                        EventResponse::Action(UiAction::EditServer(self.server_id))
                    }
                    _ => EventResponse::Ignored,
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.hover_anim.tick(dt_ms);
        self.connect_button.animate(dt_ms);
        self.edit_button.animate(dt_ms);
        self.delete_button.animate(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::servers::SavedServer;

    fn make_server() -> SavedServer {
        let mut server = SavedServer::new("Test Server", "192.168.1.10:4000");
        server.last_connected = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(60 * 60),
        );
        server.default_profile = "Gaming".to_string();
        server
    }

    #[test]
    fn card_from_saved() {
        let server = make_server();
        let card = ServerCard::from_saved(&server);
        assert_eq!(card.display_name, "Test Server");
        assert_eq!(card.status(), CardStatus::Recent);
    }

    #[test]
    fn card_paint_emits_elements() {
        let server = make_server();
        let mut card = ServerCard::from_saved(&server);
        card.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let mut ctx = PaintContext::new();
        card.paint(&mut ctx);

        assert!(ctx.glass_quads.len() >= 4);
        assert!(ctx.text_runs.len() >= 6);
    }

    #[test]
    fn card_click_connects() {
        let server = make_server();
        let address = server.address.clone();
        let mut card = ServerCard::from_saved(&server);
        card.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let resp = card.handle_event(&UiEvent::MouseDown {
            x: 120.0,
            y: 70.0,
            button: MouseButton::Left,
        });

        match resp {
            EventResponse::Action(UiAction::Connect { address: a, .. }) => {
                assert_eq!(a, address);
            }
            other => panic!("expected Connect action, got {:?}", other),
        }
    }

    #[test]
    fn edit_button_returns_edit_action() {
        let server = make_server();
        let server_id = server.id;
        let mut card = ServerCard::from_saved(&server);
        card.set_layout_mode(CardLayoutMode::Row);
        card.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let edit_rect = card.edit_button.rect();
        let resp = card.handle_event(&UiEvent::MouseDown {
            x: edit_rect.x + edit_rect.w * 0.5,
            y: edit_rect.y + edit_rect.h * 0.5,
            button: MouseButton::Left,
        });

        assert!(matches!(
            resp,
            EventResponse::Action(UiAction::EditServer(id)) if id == server_id
        ));
    }

    #[test]
    fn delete_button_returns_delete_action() {
        let server = make_server();
        let server_id = server.id;
        let mut card = ServerCard::from_saved(&server);
        card.set_layout_mode(CardLayoutMode::Row);
        card.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let del_rect = card.delete_button.rect();
        let resp = card.handle_event(&UiEvent::MouseDown {
            x: del_rect.x + del_rect.w * 0.5,
            y: del_rect.y + del_rect.h * 0.5,
            button: MouseButton::Left,
        });

        assert!(matches!(
            resp,
            EventResponse::Action(UiAction::DeleteServer(id)) if id == server_id
        ));
    }
}
