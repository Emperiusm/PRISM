// SPDX-License-Identifier: AGPL-3.0-or-later
//! Clickable button widget with hover/press animation.

use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::{
    ColorMode, EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Destructive,
    Text, // No background, no border — text only
}

pub struct Button {
    label: String,
    action: UiAction,
    style: ButtonStyle,
    color_mode: ColorMode,
    rect: Rect,
    hover_anim: Animation,
    hovered: bool,
    focused: bool,
    radius_override: Option<f32>,
}

impl Button {
    pub fn new(label: &str, action: UiAction) -> Self {
        Self {
            label: label.to_owned(),
            action,
            style: ButtonStyle::Primary,
            color_mode: ColorMode::Dark,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            hover_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            hovered: false,
            focused: false,
            radius_override: None,
        }
    }

    pub fn with_style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_color_mode(mut self, mode: ColorMode) -> Self {
        self.color_mode = mode;
        self
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius_override = Some(radius);
        self
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl Widget for Button {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 40.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Text-only style — no background, just a label
        if self.style == ButtonStyle::Text {
            let text_color = if self.hovered {
                theme::LT_TEXT_PRIMARY
            } else {
                theme::LT_TEXT_SECONDARY
            };
            let font_size = 13.0;
            let text_x =
                self.rect.x + (self.rect.w - theme::text_width(&self.label, font_size)) * 0.5;
            let text_y = self.rect.y + (self.rect.h - font_size) * 0.5 - 1.0;
            if self.focused {
                ctx.push_glass_quad(theme::focus_ring(self.rect, 8.0));
            }
            ctx.push_text_run(TextRun {
                x: text_x,
                y: text_y,
                text: self.label.clone(),
                font_size,
                color: text_color,
                ..Default::default()
            });
            return;
        }

        let hover = self.hover_anim.value();
        let (tint, border, text_color) = match self.color_mode {
            ColorMode::Light => match self.style {
                // ALT-005: flat PRIMARY_BLUE per approved mockups — not radial gradient per DESIGN.md
                ButtonStyle::Primary => (
                    [
                        theme::PRIMARY_BLUE[0],
                        theme::PRIMARY_BLUE[1],
                        theme::PRIMARY_BLUE[2],
                        0.92 + hover * 0.08,
                    ],
                    [
                        theme::PRIMARY_BLUE[0],
                        theme::PRIMARY_BLUE[1],
                        theme::PRIMARY_BLUE[2],
                        0.30 + hover * 0.10,
                    ],
                    [1.0, 1.0, 1.0, 1.0],
                ),
                ButtonStyle::Secondary => (
                    [1.0, 1.0, 1.0, 0.80 + hover * 0.10],
                    [0.831, 0.843, 0.863, 0.80 + hover * 0.10],
                    theme::LT_TEXT_PRIMARY,
                ),
                ButtonStyle::Destructive => (
                    [
                        theme::DANGER[0],
                        theme::DANGER[1],
                        theme::DANGER[2],
                        0.84 + hover * 0.08,
                    ],
                    [1.0, 1.0, 1.0, 0.14 + hover * 0.08],
                    [1.0, 1.0, 1.0, 1.0],
                ),
                ButtonStyle::Text => unreachable!(),
            },
            ColorMode::Dark => match self.style {
                ButtonStyle::Primary => (
                    [
                        theme::ACCENT[0],
                        theme::ACCENT[1],
                        theme::ACCENT[2],
                        0.86 + hover * 0.10,
                    ],
                    [1.0, 1.0, 1.0, 0.18 + hover * 0.10],
                    theme::TEXT_PRIMARY,
                ),
                ButtonStyle::Secondary => (
                    [0.18, 0.22, 0.29, 0.82 + hover * 0.08],
                    [1.0, 1.0, 1.0, 0.12 + hover * 0.08],
                    theme::TEXT_PRIMARY,
                ),
                ButtonStyle::Destructive => (
                    [0.42, 0.18, 0.22, 0.84 + hover * 0.08],
                    [1.0, 1.0, 1.0, 0.14 + hover * 0.08],
                    theme::TEXT_PRIMARY,
                ),
                ButtonStyle::Text => unreachable!(),
            },
        };
        let radius = self.radius_override.unwrap_or(match self.color_mode {
            ColorMode::Light => 8.0,
            ColorMode::Dark => theme::CONTROL_RADIUS,
        });
        ctx.push_glass_quad(theme::glass_quad(
            self.rect,
            tint,
            border,
            radius,
        ));

        if hover > 0.01 {
            let overlay = match self.color_mode {
                ColorMode::Light => match self.style {
                    ButtonStyle::Primary => [
                        theme::PRIMARY_BLUE[0],
                        theme::PRIMARY_BLUE[1],
                        theme::PRIMARY_BLUE[2],
                        0.06 + hover * 0.06,
                    ],
                    ButtonStyle::Secondary => [1.0, 1.0, 1.0, 0.04 + hover * 0.06],
                    ButtonStyle::Destructive => theme::destructive(0.06 + hover * 0.08),
                    ButtonStyle::Text => unreachable!(),
                },
                ColorMode::Dark => match self.style {
                    ButtonStyle::Primary => theme::accent(0.08 + hover * 0.08),
                    ButtonStyle::Secondary => [1.0, 1.0, 1.0, 0.04 + hover * 0.05],
                    ButtonStyle::Destructive => theme::destructive(0.06 + hover * 0.08),
                    ButtonStyle::Text => unreachable!(),
                },
            };
            ctx.push_glass_quad(theme::glass_quad(
                self.rect,
                overlay,
                [0.0, 0.0, 0.0, 0.0],
                radius,
            ));
        }

        // Focus ring overlay
        if self.focused {
            ctx.push_glass_quad(theme::focus_ring(self.rect, radius));
        }

        let font_size = 13.0;
        let text_x = self.rect.x + (self.rect.w - theme::text_width(&self.label, font_size)) * 0.5;
        let text_y = self.rect.y + (self.rect.h - font_size) * 0.5 - 1.0;
        ctx.push_text_run(TextRun {
            x: text_x,
            y: text_y,
            text: self.label.clone(),
            font_size,
            color: text_color,
            ..Default::default()
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                let now_hovered = self.rect.contains(*x, *y);
                if now_hovered != self.hovered {
                    self.hovered = now_hovered;
                    self.hover_anim
                        .set_target(if now_hovered { 1.0 } else { 0.0 });
                }
                EventResponse::Ignored
            }
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                if self.rect.contains(*x, *y) {
                    EventResponse::Action(self.action.clone())
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

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 200.0, 100.0)
    }

    fn make_button() -> Button {
        Button::new("Connect", UiAction::AddServer)
    }

    #[test]
    fn button_emits_glass_quad_and_text() {
        let mut btn = make_button();
        btn.layout(available());
        let mut ctx = PaintContext::new();
        btn.paint(&mut ctx);
        assert_eq!(ctx.glass_quads.len(), 1);
        assert_eq!(ctx.text_runs.len(), 1);
        assert_eq!(ctx.text_runs[0].text, "Connect");
    }

    #[test]
    fn button_hover_adds_overlay_surface() {
        let mut btn = make_button();
        btn.layout(available());

        // Move mouse inside the button
        btn.handle_event(&UiEvent::MouseMove { x: 100.0, y: 18.0 });
        // Animate enough to get visible glow
        btn.animate(200.0);

        let mut ctx = PaintContext::new();
        btn.paint(&mut ctx);
        assert!(
            ctx.glass_quads.len() >= 2,
            "expected an extra overlay quad after hovering"
        );
    }

    #[test]
    fn button_click_returns_action() {
        let mut btn = make_button();
        btn.layout(available());
        let resp = btn.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 18.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Action(_)));
    }

    #[test]
    fn button_click_outside_ignored() {
        let mut btn = make_button();
        btn.layout(available());
        let resp = btn.handle_event(&UiEvent::MouseDown {
            x: 500.0,
            y: 500.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Ignored));
    }
}
