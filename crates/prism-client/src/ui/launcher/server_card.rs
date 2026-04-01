// SPDX-License-Identifier: AGPL-3.0-or-later
//! Glassmorphism server card with hover animation.

use crate::config::servers::SavedServer;
use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::widgets::{
    EventResponse, GlassQuad, GlowRect, MouseButton, PaintContext, Rect, Size, TextRun, UiAction,
    UiEvent, Widget,
};

// ---------------------------------------------------------------------------
// ServerCard
// ---------------------------------------------------------------------------

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
    /// Build a `ServerCard` from a `SavedServer`.
    pub fn from_saved(server: &SavedServer) -> Self {
        let accent_color = [
            server.accent_color[0] as f32 / 255.0,
            server.accent_color[1] as f32 / 255.0,
            server.accent_color[2] as f32 / 255.0,
        ];

        let last_info = match (&server.last_codec, &server.last_resolution) {
            (Some(codec), Some((w, h))) => format!("{} · {}×{}", codec, w, h),
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
            rect: Rect::new(0.0, 0.0, 240.0, 140.0),
        }
    }

    pub fn server_id(&self) -> uuid::Uuid {
        self.server_id
    }
}

impl Widget for ServerCard {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, 240.0, 140.0);
        Size { w: 240.0, h: 140.0 }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let r = self.rect;

        // 1. Glass card body
        ctx.push_glass_quad(GlassQuad {
            rect: r,
            blur_rect: r,
            tint: [0.1, 0.0, 0.2, 0.12],
            border_color: [1.0, 1.0, 1.0, 0.15],
            corner_radius: 10.0,
            noise_intensity: 0.03,
        });

        // 2. Accent stripe — 4 px wide on left edge
        let stripe = Rect::new(r.x, r.y, 4.0, r.h);
        ctx.push_glow_rect(GlowRect {
            rect: stripe,
            color: [
                self.accent_color[0],
                self.accent_color[1],
                self.accent_color[2],
                0.7,
            ],
            spread: 0.0,
            intensity: 1.0,
        });

        // 3. Hover glow (when hover_anim > 0.01)
        let hover_val = self.hover_anim.value();
        if hover_val > 0.01 {
            ctx.push_glow_rect(GlowRect {
                rect: r,
                color: [
                    self.accent_color[0],
                    self.accent_color[1],
                    self.accent_color[2],
                    hover_val * 0.15,
                ],
                spread: 8.0,
                intensity: hover_val * 0.3,
            });
        }

        // 4. Display name
        ctx.push_text_run(TextRun {
            x: r.x + 16.0,
            y: r.y + 16.0,
            text: self.display_name.clone(),
            font_size: 15.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });

        // 5. Address
        ctx.push_text_run(TextRun {
            x: r.x + 16.0,
            y: r.y + 38.0,
            text: self.address.clone(),
            font_size: 12.0,
            color: [1.0, 1.0, 1.0, 0.5],
            monospace: false,
        });

        // 6. Last info
        ctx.push_text_run(TextRun {
            x: r.x + 16.0,
            y: r.y + 58.0,
            text: self.last_info.clone(),
            font_size: 11.0,
            color: [1.0, 1.0, 1.0, 0.4],
            monospace: false,
        });

        // 7. Last profile
        ctx.push_text_run(TextRun {
            x: r.x + 16.0,
            y: r.y + 110.0,
            text: self.last_profile.clone(),
            font_size: 11.0,
            color: [
                self.accent_color[0],
                self.accent_color[1],
                self.accent_color[2],
                0.8,
            ],
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

        assert!(ctx.glass_quads.len() >= 1, "expected at least 1 glass quad");
        assert!(ctx.text_runs.len() >= 3, "expected at least 3 text runs");
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
