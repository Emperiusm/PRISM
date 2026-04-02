// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared visual tokens for the PRISM client UI.

use crate::ui::widgets::{GlassQuad, Rect};

pub const BACKDROP: [f64; 3] = [0.047, 0.063, 0.094];

pub const TEXT_PRIMARY: [f32; 4] = [0.97, 0.98, 1.0, 0.96];
pub const TEXT_SECONDARY: [f32; 4] = [0.82, 0.87, 0.94, 0.82];
pub const TEXT_MUTED: [f32; 4] = [0.72, 0.79, 0.88, 0.58];
pub const TEXT_TERTIARY: [f32; 4] = [0.62, 0.70, 0.80, 0.44];

pub const ACCENT: [f32; 4] = [0.47, 0.75, 0.98, 1.0];
pub const ACCENT_SOFT: [f32; 4] = [0.47, 0.75, 0.98, 0.18];
pub const ACCENT_FAINT: [f32; 4] = [0.47, 0.75, 0.98, 0.10];
pub const SUCCESS: [f32; 4] = [0.44, 0.84, 0.60, 1.0];
pub const WARNING: [f32; 4] = [0.96, 0.80, 0.39, 1.0];
pub const DANGER: [f32; 4] = [0.95, 0.48, 0.43, 1.0];

pub const HERO_RADIUS: f32 = 26.0;
pub const PANEL_RADIUS: f32 = 22.0;
pub const CARD_RADIUS: f32 = 20.0;
pub const CONTROL_RADIUS: f32 = 14.0;
pub const CHIP_RADIUS: f32 = 12.0;
pub const SIDEBAR_RADIUS: f32 = 28.0;

pub fn glass_quad(
    rect: Rect,
    tint: [f32; 4],
    border_color: [f32; 4],
    corner_radius: f32,
) -> GlassQuad {
    GlassQuad {
        rect,
        blur_rect: rect,
        tint,
        border_color,
        corner_radius,
        noise_intensity: 0.0,
    }
}

pub fn hero_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.15, 0.19, 0.26, 0.76],
        [1.0, 1.0, 1.0, 0.16],
        HERO_RADIUS,
    )
}

pub fn card_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.14, 0.18, 0.24, 0.72],
        [1.0, 1.0, 1.0, 0.14],
        CARD_RADIUS,
    )
}

pub fn floating_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.12, 0.16, 0.22, 0.78],
        [1.0, 1.0, 1.0, 0.16],
        PANEL_RADIUS,
    )
}

pub fn sidebar_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.10, 0.14, 0.19, 0.84],
        [1.0, 1.0, 1.0, 0.10],
        SIDEBAR_RADIUS,
    )
}

pub fn nav_item_surface(rect: Rect, active: bool, hovered: bool) -> GlassQuad {
    glass_quad(
        rect,
        if active {
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.18]
        } else if hovered {
            [1.0, 1.0, 1.0, 0.06]
        } else {
            [1.0, 1.0, 1.0, 0.0]
        },
        if active {
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.22]
        } else if hovered {
            [1.0, 1.0, 1.0, 0.08]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        },
        CONTROL_RADIUS,
    )
}

pub fn control_surface(rect: Rect, focused: bool) -> GlassQuad {
    glass_quad(
        rect,
        if focused {
            [0.17, 0.22, 0.29, 0.94]
        } else {
            [0.14, 0.18, 0.24, 0.88]
        },
        if focused {
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.42]
        } else {
            [1.0, 1.0, 1.0, 0.12]
        },
        CONTROL_RADIUS,
    )
}

pub fn separator(rect: Rect) -> GlassQuad {
    glass_quad(rect, [1.0, 1.0, 1.0, 0.08], [0.0, 0.0, 0.0, 0.0], 0.0)
}

pub fn accent(alpha: f32) -> [f32; 4] {
    [ACCENT[0], ACCENT[1], ACCENT[2], alpha]
}

pub fn destructive(alpha: f32) -> [f32; 4] {
    [DANGER[0], DANGER[1], DANGER[2], alpha]
}

pub fn text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.52
}
