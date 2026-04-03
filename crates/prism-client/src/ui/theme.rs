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
pub const FONT_HERO: f32 = 36.0;
pub const FONT_DISPLAY: f32 = 30.0;
pub const FONT_HEADLINE: f32 = 20.0;
pub const FONT_BODY: f32 = 14.0;
pub const FONT_LABEL: f32 = 13.0;
pub const FONT_CAPTION: f32 = 11.0;
pub const FONT_CHIP: f32 = 10.0;
pub const MODAL_RADIUS: f32 = 22.0;
pub const CAPSULE_RADIUS: f32 = 24.0;
pub const TOGGLE_RADIUS: f32 = 10.0;

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

pub fn list_row_surface(rect: Rect, hovered: bool) -> GlassQuad {
    glass_quad(
        rect,
        if hovered {
            [0.16, 0.20, 0.27, 0.82]
        } else {
            [0.14, 0.18, 0.24, 0.76]
        },
        [1.0, 1.0, 1.0, 0.10],
        CONTROL_RADIUS,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipTone {
    Success,
    Warning,
    Danger,
    Accent,
    Neutral,
}

pub fn status_chip(rect: Rect, tone: ChipTone) -> GlassQuad {
    let (tint, border) = match tone {
        ChipTone::Success => (
            [SUCCESS[0], SUCCESS[1], SUCCESS[2], 0.14],
            [SUCCESS[0], SUCCESS[1], SUCCESS[2], 0.22],
        ),
        ChipTone::Warning => (
            [WARNING[0], WARNING[1], WARNING[2], 0.14],
            [WARNING[0], WARNING[1], WARNING[2], 0.22],
        ),
        ChipTone::Danger => (
            [DANGER[0], DANGER[1], DANGER[2], 0.14],
            [DANGER[0], DANGER[1], DANGER[2], 0.22],
        ),
        ChipTone::Accent => (
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.12],
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.18],
        ),
        ChipTone::Neutral => ([1.0, 1.0, 1.0, 0.06], [1.0, 1.0, 1.0, 0.10]),
    };
    glass_quad(rect, tint, border, CHIP_RADIUS)
}

pub fn section_header_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.13, 0.17, 0.23, 0.60],
        [1.0, 1.0, 1.0, 0.06],
        CONTROL_RADIUS,
    )
}

pub fn modal_scrim(rect: Rect) -> GlassQuad {
    glass_quad(rect, [0.0, 0.0, 0.0, 0.48], [0.0, 0.0, 0.0, 0.0], 0.0)
}

pub fn modal_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.12, 0.16, 0.22, 0.94],
        [1.0, 1.0, 1.0, 0.14],
        MODAL_RADIUS,
    )
}

pub fn capsule_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.10, 0.14, 0.19, 0.88],
        [1.0, 1.0, 1.0, 0.12],
        CAPSULE_RADIUS,
    )
}

pub fn capsule_dropdown_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.11, 0.15, 0.21, 0.92],
        [1.0, 1.0, 1.0, 0.10],
        PANEL_RADIUS,
    )
}

pub fn toggle_track(rect: Rect, on: bool) -> GlassQuad {
    glass_quad(
        rect,
        if on {
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.72]
        } else {
            [0.22, 0.26, 0.33, 0.88]
        },
        if on {
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.32]
        } else {
            [1.0, 1.0, 1.0, 0.10]
        },
        TOGGLE_RADIUS,
    )
}

pub fn toggle_thumb(rect: Rect, on: bool) -> GlassQuad {
    glass_quad(
        rect,
        if on {
            [0.95, 0.97, 1.0, 0.96]
        } else {
            [0.70, 0.75, 0.82, 0.88]
        },
        [0.0, 0.0, 0.0, 0.0],
        TOGGLE_RADIUS,
    )
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

// ---------------------------------------------------------------------------
// Light-mode launcher palette
// ---------------------------------------------------------------------------

/// Launcher background — light blue base matching Stitch gradient.
/// Used as the wgpu clear-color when UiState == Launcher.
pub const LAUNCHER_BACKDROP: [f64; 3] = [0.561, 0.682, 0.878];

// Light-mode text (dark on light surfaces) — launcher only
pub const LT_TEXT_PRIMARY: [f32; 4] = [0.059, 0.090, 0.165, 1.0]; // #0f172a
pub const LT_TEXT_SECONDARY: [f32; 4] = [0.278, 0.333, 0.412, 1.0]; // #475569
pub const LT_TEXT_MUTED: [f32; 4] = [0.580, 0.639, 0.722, 1.0]; // #94a3b8

/// Corporate blue primary button.
pub const PRIMARY_BLUE: [f32; 4] = [0.059, 0.424, 0.741, 1.0]; // #0F6CBD

// Light-mode slider
pub const SLIDER_TRACK_LIGHT: [f32; 4] = [0.059, 0.424, 0.741, 0.10]; // primary at 10%
pub const SLIDER_THUMB_LIGHT: [f32; 4] = PRIMARY_BLUE;
pub const SLIDER_THUMB_BORDER: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

// Light-mode segmented control
pub const SEGMENTED_CONTAINER_LIGHT: [f32; 4] = [1.0, 1.0, 1.0, 0.50]; // bg-white/50
pub const SEGMENTED_ACTIVE_LIGHT: [f32; 4] = PRIMARY_BLUE;

// ---------------------------------------------------------------------------
// Launcher surface helpers
// ---------------------------------------------------------------------------

/// Launcher sidebar — cream Mica tint.
pub fn launcher_sidebar_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.922, 0.945, 0.965, 0.92], // #EBF1F6 at high opacity
        [1.0, 1.0, 1.0, 0.40],
        SIDEBAR_RADIUS,
    )
}

/// Launcher hero / glass-panel — white frosted glass.
pub fn launcher_hero_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.45],
        [1.0, 1.0, 1.0, 0.50],
        HERO_RADIUS,
    )
}

/// Launcher card — white glass, medium opacity.
pub fn launcher_card_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.65],
        [1.0, 1.0, 1.0, 0.70],
        CARD_RADIUS,
    )
}

/// Launcher card hover overlay.
pub fn launcher_card_hover(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.85],
        [1.0, 1.0, 1.0, 0.80],
        CARD_RADIUS,
    )
}

/// Launcher list container — nearly opaque white.
pub fn launcher_list_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.85],
        [1.0, 1.0, 1.0, 0.60],
        CONTROL_RADIUS,
    )
}

/// Launcher list row hover.
pub fn launcher_row_surface(rect: Rect, hovered: bool) -> GlassQuad {
    glass_quad(
        rect,
        if hovered {
            [1.0, 1.0, 1.0, 0.40]
        } else {
            [0.0, 0.0, 0.0, 0.0] // transparent, list bg shows through
        },
        [0.0, 0.0, 0.0, 0.0],
        0.0,
    )
}

/// Launcher active nav item background.
pub fn launcher_nav_item_surface(rect: Rect, active: bool, hovered: bool) -> GlassQuad {
    glass_quad(
        rect,
        if active {
            [0.898, 0.898, 0.898, 0.60]
        } else if hovered {
            [0.898, 0.898, 0.898, 0.30]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        },
        [0.0, 0.0, 0.0, 0.0],
        CONTROL_RADIUS,
    )
}

/// Launcher control (text input, dropdown) — white with subtle border.
pub fn launcher_control_surface(rect: Rect, focused: bool) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.80],
        if focused {
            [PRIMARY_BLUE[0], PRIMARY_BLUE[1], PRIMARY_BLUE[2], 0.60]
        } else {
            [0.831, 0.843, 0.863, 1.0] // border-gray-300
        },
        CONTROL_RADIUS,
    )
}

/// Launcher modal surface — white panel.
pub fn launcher_modal_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.92],
        [1.0, 1.0, 1.0, 0.60],
        MODAL_RADIUS,
    )
}

/// Launcher separator — light gray divider (between sections on gradient bg).
pub fn launcher_separator(rect: Rect) -> GlassQuad {
    glass_quad(rect, [0.898, 0.898, 0.898, 0.50], [0.0, 0.0, 0.0, 0.0], 0.0)
}

/// Subtle in-panel divider — used inside white glass containers.
pub fn launcher_inner_separator(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.0, 0.0, 0.0, 0.05], // bg-black/5
        [0.0, 0.0, 0.0, 0.0],
        0.0,
    )
}

/// Toggle card surface — Profiles uses ~60% white, Settings uses ~30% white.
pub fn launcher_toggle_card_surface(rect: Rect, alpha: f32) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, alpha], // 0.60 for Profiles, 0.30 for Settings
        [1.0, 1.0, 1.0, 0.80],
        CARD_RADIUS, // rounded-2xl = 16px
    )
}

/// Light-mode status chip — opaque pastel bg with colored text.
pub fn launcher_status_chip(rect: Rect, tone: ChipTone) -> GlassQuad {
    let (tint, border) = match tone {
        ChipTone::Success => (
            [0.863, 0.988, 0.906, 1.0], // bg-green-100
            [0.745, 0.933, 0.820, 1.0], // border-green-200
        ),
        ChipTone::Warning => (
            [0.996, 0.976, 0.765, 1.0], // bg-yellow-100
            [0.988, 0.933, 0.600, 1.0], // border-yellow-200
        ),
        ChipTone::Danger => (
            [0.996, 0.886, 0.886, 1.0], // bg-red-100
            [0.988, 0.808, 0.808, 1.0], // border-red-200
        ),
        ChipTone::Accent => (
            [0.855, 0.922, 0.996, 1.0], // bg-blue-100
            [0.745, 0.867, 0.988, 1.0], // border-blue-200
        ),
        ChipTone::Neutral => (
            [0.945, 0.949, 0.957, 1.0], // bg-gray-100
            [0.898, 0.898, 0.898, 1.0], // border-gray-200
        ),
    };
    glass_quad(rect, tint, border, CHIP_RADIUS)
}

/// Returns the text color for a light-mode status chip label.
pub fn launcher_chip_text_color(tone: ChipTone) -> [f32; 4] {
    match tone {
        ChipTone::Success => [0.086, 0.396, 0.204, 1.0], // text-green-800
        ChipTone::Warning => [0.522, 0.302, 0.055, 1.0], // text-yellow-800
        ChipTone::Danger => [0.600, 0.106, 0.106, 1.0],  // text-red-800
        ChipTone::Accent => [0.114, 0.357, 0.627, 1.0],  // text-blue-800
        ChipTone::Neutral => [0.278, 0.333, 0.412, 1.0], // text-gray-600
    }
}
