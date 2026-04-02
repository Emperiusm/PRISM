// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saved server card with status chips and stronger actions.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::servers::SavedServer;
use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::button::{Button, ButtonStyle};
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

const WEEK_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFilter {
    All,
    Recent,
    Dormant,
    New,
}

impl CardFilter {
    pub fn label(self, total_cards: usize) -> String {
        match self {
            CardFilter::All => format!("All Hosts ({total_cards})"),
            CardFilter::Recent => "Recent".to_string(),
            CardFilter::Dormant => "Dormant".to_string(),
            CardFilter::New => "New".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStatus {
    Recent,
    Dormant,
    New,
}

impl CardStatus {
    fn label(self) -> &'static str {
        match self {
            CardStatus::Recent => "Recent",
            CardStatus::Dormant => "Dormant",
            CardStatus::New => "New",
        }
    }

    fn tone(self) -> [f32; 4] {
        match self {
            CardStatus::Recent => theme::SUCCESS,
            CardStatus::Dormant => theme::WARNING,
            CardStatus::New => theme::ACCENT,
        }
    }
}

pub struct ServerCard {
    server_id: uuid::Uuid,
    display_name: String,
    address: String,
    last_profile: String,
    last_connected: Option<u64>,
    last_info: String,
    accent_color: [f32; 3],
    connect_button: Button,
    edit_button: Button,
    hover_anim: Animation,
    hovered: bool,
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

        Self {
            server_id: server.id,
            display_name: server.display_name.clone(),
            address: server.address.clone(),
            last_profile: if server.default_profile.is_empty() {
                "Default".to_string()
            } else {
                server.default_profile.clone()
            },
            last_connected: server.last_connected,
            last_info,
            accent_color,
            connect_button: Button::new(
                "Connect",
                UiAction::Connect {
                    address: server.address.clone(),
                    noise_key: None,
                },
            )
            .with_style(ButtonStyle::Primary),
            edit_button: Button::new("Edit", UiAction::EditServer(server.id))
                .with_style(ButtonStyle::Secondary),
            hover_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            hovered: false,
            rect: Rect::new(0.0, 0.0, Self::WIDTH, Self::HEIGHT),
        }
    }

    pub fn matches_filter(&self, filter: CardFilter) -> bool {
        match filter {
            CardFilter::All => true,
            CardFilter::Recent => self.status() == CardStatus::Recent,
            CardFilter::Dormant => self.status() == CardStatus::Dormant,
            CardFilter::New => self.status() == CardStatus::New,
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

    fn status_chip_rect(&self) -> Rect {
        let w = theme::text_width(self.status().label(), 10.0) + 26.0;
        Rect::new(self.rect.x + 18.0, self.rect.y + 16.0, w, 20.0)
    }

    fn profile_chip_rect(&self) -> Rect {
        let label = self.last_profile.to_uppercase();
        let w = theme::text_width(&label, 10.0) + 26.0;
        Rect::new(
            self.rect.x + self.rect.w - 18.0 - w,
            self.rect.y + 16.0,
            w,
            20.0,
        )
    }

    fn buttons_y(&self) -> f32 {
        self.rect.y + self.rect.h - 54.0
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
}

impl Widget for ServerCard {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, Self::WIDTH, Self::HEIGHT);
        let button_y = self.buttons_y();
        self.connect_button.layout(Rect::new(
            self.rect.x + 18.0,
            button_y,
            self.rect.w - 114.0,
            40.0,
        ));
        self.edit_button.layout(Rect::new(
            self.rect.x + self.rect.w - 84.0,
            button_y,
            66.0,
            40.0,
        ));
        Size {
            w: Self::WIDTH,
            h: Self::HEIGHT,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let r = self.rect;
        let hover = self.hover_anim.value();
        let status = self.status();
        let status_tone = status.tone();
        let accent = [
            self.accent_color[0],
            self.accent_color[1],
            self.accent_color[2],
            1.0,
        ];

        ctx.push_glass_quad(theme::card_surface(r));
        if hover > 0.01 {
            ctx.push_glass_quad(theme::glass_quad(
                r,
                [accent[0], accent[1], accent[2], 0.04 + hover * 0.08],
                [accent[0], accent[1], accent[2], 0.08 + hover * 0.10],
                theme::CARD_RADIUS,
            ));
        }

        let status_rect = self.status_chip_rect();
        ctx.push_glass_quad(theme::glass_quad(
            status_rect,
            [status_tone[0], status_tone[1], status_tone[2], 0.18],
            [status_tone[0], status_tone[1], status_tone[2], 0.22],
            theme::CHIP_RADIUS,
        ));
        ctx.push_text_run(TextRun {
            x: status_rect.x + 12.0,
            y: status_rect.y + 4.0,
            text: status.label().to_string(),
            font_size: 10.0,
            color: [status_tone[0], status_tone[1], status_tone[2], 0.96],
            monospace: false,
        });

        let profile_rect = self.profile_chip_rect();
        let profile_label = self.last_profile.to_uppercase();
        ctx.push_glass_quad(theme::glass_quad(
            profile_rect,
            [1.0, 1.0, 1.0, 0.08],
            [1.0, 1.0, 1.0, 0.10],
            theme::CHIP_RADIUS,
        ));
        ctx.push_text_run(TextRun {
            x: profile_rect.x + 12.0,
            y: profile_rect.y + 4.0,
            text: profile_label,
            font_size: 10.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: r.x + 18.0,
            y: r.y + 52.0,
            text: self.display_name.clone(),
            font_size: 16.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: r.x + 18.0,
            y: r.y + 76.0,
            text: self.address.clone(),
            font_size: 11.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: r.x + 18.0,
            y: r.y + 110.0,
            text: self.relative_last_connected(),
            font_size: 11.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: r.x + 18.0,
            y: r.y + 132.0,
            text: self.last_info.clone(),
            font_size: 11.0,
            color: theme::TEXT_TERTIARY,
            monospace: false,
        });

        self.connect_button.paint(ctx);
        self.edit_button.paint(ctx);
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
                    MouseButton::Right => EventResponse::Action(UiAction::EditServer(self.server_id)),
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
        card.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let resp = card.handle_event(&UiEvent::MouseDown {
            x: card.rect.x + card.rect.w - 40.0,
            y: card.rect.y + card.rect.h - 30.0,
            button: MouseButton::Left,
        });

        assert!(matches!(
            resp,
            EventResponse::Action(UiAction::EditServer(id)) if id == server_id
        ));
    }
}
