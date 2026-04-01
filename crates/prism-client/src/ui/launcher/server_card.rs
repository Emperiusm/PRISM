// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saved server card with calmer glass styling and hover feedback.

use crate::config::servers::SavedServer;
use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

pub struct ServerCard {
    server_id: uuid::Uuid,
    display_name: String,
    address: String,
    last_profile: String,
    last_info: String,
    accent_color: [f32; 3],
    hover_anim: Animation,
    hovered: bool,
    rect: Rect,
}

impl ServerCard {
    pub const WIDTH: f32 = 264.0;
    pub const HEIGHT: f32 = 164.0;

    /// Build a `ServerCard` from a `SavedServer`.
    pub fn from_saved(server: &SavedServer) -> Self {
        let accent_color = [
            server.accent_color[0] as f32 / 255.0,
            server.accent_color[1] as f32 / 255.0,
            server.accent_color[2] as f32 / 255.0,
        ];

        let last_info = match (&server.last_codec, &server.last_resolution) {
            (Some(codec), Some((w, h))) => format!("{codec} at {w}x{h}"),
            _ => String::from("Never connected"),
        };

        Self {
            server_id: server.id,
            display_name: server.display_name.clone(),
            address: server.address.clone(),
            last_profile: server.default_profile.clone(),
            last_info,
            accent_color,
            hover_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            hovered: false,
            rect: Rect::new(0.0, 0.0, Self::WIDTH, Self::HEIGHT),
        }
    }

    pub fn server_id(&self) -> uuid::Uuid {
        self.server_id
    }
}

impl Widget for ServerCard {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, Self::WIDTH, Self::HEIGHT);
        Size {
            w: Self::WIDTH,
            h: Self::HEIGHT,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let r = self.rect;
        let hover = self.hover_anim.value();
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
                [accent[0], accent[1], accent[2], 0.05 + hover * 0.08],
                [accent[0], accent[1], accent[2], 0.08 + hover * 0.10],
                theme::CARD_RADIUS,
            ));
        }

        let accent_dot = Rect::new(r.x + 18.0, r.y + 18.0, 10.0, 10.0);
        ctx.push_glass_quad(theme::glass_quad(
            accent_dot,
            [accent[0], accent[1], accent[2], 0.78],
            [1.0, 1.0, 1.0, 0.08],
            5.0,
        ));

        ctx.push_text_run(TextRun {
            x: r.x + 36.0,
            y: r.y + 14.0,
            text: self.display_name.clone(),
            font_size: 16.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: r.x + 18.0,
            y: r.y + 44.0,
            text: self.address.clone(),
            font_size: 12.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: r.x + 18.0,
            y: r.y + 78.0,
            text: "Last session".into(),
            font_size: 11.0,
            color: theme::TEXT_TERTIARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: r.x + 18.0,
            y: r.y + 96.0,
            text: self.last_info.clone(),
            font_size: 12.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        let chip_w = theme::text_width(&self.last_profile, 11.0) + 24.0;
        let chip_rect = Rect::new(r.x + 18.0, r.y + r.h - 38.0, chip_w, 22.0);
        ctx.push_glass_quad(theme::glass_quad(
            chip_rect,
            [accent[0], accent[1], accent[2], 0.14],
            [accent[0], accent[1], accent[2], 0.18],
            theme::CHIP_RADIUS,
        ));
        ctx.push_text_run(TextRun {
            x: chip_rect.x + 12.0,
            y: chip_rect.y + 4.0,
            text: self.last_profile.clone(),
            font_size: 11.0,
            color: [accent[0], accent[1], accent[2], 0.92],
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
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
                if self.rect.contains(*x, *y) {
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
                } else {
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.hover_anim.tick(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::servers::SavedServer;

    fn make_server() -> SavedServer {
        SavedServer::new("Test Server", "192.168.1.10:4000")
    }

    #[test]
    fn card_from_saved() {
        let server = make_server();
        let card = ServerCard::from_saved(&server);
        assert_eq!(card.display_name, "Test Server");
    }

    #[test]
    fn card_paint_emits_elements() {
        let server = make_server();
        let mut card = ServerCard::from_saved(&server);
        card.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let mut ctx = PaintContext::new();
        card.paint(&mut ctx);

        assert!(
            ctx.glass_quads.len() >= 2,
            "expected card body and profile chip"
        );
        assert!(ctx.text_runs.len() >= 4, "expected primary card labels");
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
}
