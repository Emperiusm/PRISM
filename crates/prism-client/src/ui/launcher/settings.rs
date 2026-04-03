// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher settings page with functional preferences.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::config::client_config_prefs::UserPrefs;
use crate::ui::theme;
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::toggle::Toggle;
use crate::ui::widgets::{
    ColorMode, EventResponse, PaintContext, Rect, Size, TextRun, UiEvent, Widget,
};

const ROW_GAP: f32 = 28.0;

pub struct SettingsPanel {
    rect: Rect,
    identity_path: String,
    version: String,
    user_prefs: Option<Arc<Mutex<UserPrefs>>>,
    prefs_dir: Option<PathBuf>,
    default_profile_dropdown: Dropdown,
    exclusive_keyboard_toggle: Toggle,
    relative_mouse_toggle: Toggle,
    audio_output_dropdown: Dropdown,
    mic_dropdown: Dropdown,

    // Simplest native scroll tracking possible without overengineering clipped contexts
    scroll_y: f32,
    max_scroll: f32,
}

impl SettingsPanel {
    pub fn new(identity_path: String, version: String) -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            identity_path,
            version,
            user_prefs: None,
            prefs_dir: None,
            default_profile_dropdown: Dropdown::new(
                vec![
                    "Gaming".to_string(),
                    "Coding".to_string(),
                    "Balanced".to_string(),
                    "Low Bandwidth".to_string(),
                ],
                2,
            )
            .with_color_mode(ColorMode::Light),
            exclusive_keyboard_toggle: Toggle::new(true).with_color_mode(ColorMode::Light),
            relative_mouse_toggle: Toggle::new(false).with_color_mode(ColorMode::Light),
            audio_output_dropdown: Dropdown::new(
                vec![
                    "System Default".to_string(),
                    "Primary Speakers".to_string(),
                    "Headset".to_string(),
                ],
                0,
            )
            .with_color_mode(ColorMode::Light),
            mic_dropdown: Dropdown::new(
                vec![
                    "System Default".to_string(),
                    "Built-in Mic".to_string(),
                    "USB Interface".to_string(),
                ],
                0,
            )
            .with_color_mode(ColorMode::Light),
            scroll_y: 0.0,
            max_scroll: 0.0,
        }
    }

    pub fn set_profile_names(&mut self, names: Vec<String>) {
        let selected = self.default_profile_dropdown.selected_text().to_string();
        let options = if names.is_empty() {
            vec!["Balanced".to_string()]
        } else {
            names
        };
        let selected_index = options
            .iter()
            .position(|name| *name == selected)
            .or_else(|| options.iter().position(|name| name == "Balanced"))
            .unwrap_or(0);
        self.default_profile_dropdown = Dropdown::new(options, selected_index);
    }

    pub fn set_user_prefs(&mut self, prefs: Arc<Mutex<UserPrefs>>, dir: PathBuf) {
        self.user_prefs = Some(prefs.clone());
        self.prefs_dir = Some(dir);
        if let Ok(guard) = prefs.lock() {
            self.apply_prefs_to_controls(&guard);
        }
    }

    fn apply_prefs_to_controls(&mut self, prefs: &UserPrefs) {
        if !self
            .default_profile_dropdown
            .set_selected_by_text(&prefs.default_profile)
        {
            let _ = self
                .default_profile_dropdown
                .set_selected_by_text("Balanced");
        }

        self.exclusive_keyboard_toggle
            .set_on(prefs.exclusive_keyboard);
        self.relative_mouse_toggle.set_on(prefs.relative_mouse);
    }

    fn persist_user_prefs(&self) {
        let Some(prefs_arc) = &self.user_prefs else {
            return;
        };
        let Some(prefs_dir) = &self.prefs_dir else {
            return;
        };

        if let Ok(mut prefs) = prefs_arc.lock() {
            prefs.default_profile = self.default_profile_dropdown.selected_text().to_string();
            prefs.exclusive_keyboard = self.exclusive_keyboard_toggle.is_on();
            prefs.relative_mouse = self.relative_mouse_toggle.is_on();
            let _ = prefs.save(prefs_dir);
        }
    }
}

impl Widget for SettingsPanel {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;

        let content_x = available.x + 40.0;
        let content_w = (available.w - 80.0).clamp(400.0, 900.0);

        // Settings inner components width mappings (split layout logic)
        let _left_w = content_w * 0.35;
        let right_w = content_w * 0.65;
        let right_x = content_x + content_w - right_w;

        // Base y is affected smoothly by manual scrolling
        let mut cursor_y = available.y + 110.0 - self.scroll_y;

        // Sections
        // Identity
        cursor_y += 34.0;

        // Device Trust
        cursor_y += ROW_GAP + 20.0 + 34.0;

        // Streaming Defaults
        cursor_y += ROW_GAP + 20.0;
        self.default_profile_dropdown
            .layout(Rect::new(right_x, cursor_y, right_w, 40.0));
        cursor_y += 40.0;

        // Input
        cursor_y += ROW_GAP + 20.0;
        self.exclusive_keyboard_toggle.layout(Rect::new(
            right_x + right_w - 42.0,
            cursor_y + 14.0,
            42.0,
            22.0,
        ));
        cursor_y += 56.0;
        self.relative_mouse_toggle.layout(Rect::new(
            right_x + right_w - 42.0,
            cursor_y + 14.0,
            42.0,
            22.0,
        ));
        cursor_y += 50.0;

        // Audio
        cursor_y += ROW_GAP + 20.0;
        self.audio_output_dropdown
            .layout(Rect::new(right_x, cursor_y + 24.0, right_w, 40.0));
        cursor_y += 76.0;
        self.mic_dropdown
            .layout(Rect::new(right_x, cursor_y + 24.0, right_w, 40.0));
        cursor_y += 76.0;

        // Compute total unscaled height
        let total_content_h = (cursor_y + self.scroll_y - available.y) + 120.0;

        // Update valid max scroll bounds dynamically
        self.max_scroll = (total_content_h - available.h).max(0.0);

        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Sticky Header Region Layout (unaffected by scroll_y directly, visually floats if needed, but we'll draw it to scroll for harmony with Stitch).
        let scroll_top = self.rect.y - self.scroll_y;

        let content_x = self.rect.x + 40.0;
        let content_w = (self.rect.w - 80.0).clamp(400.0, 900.0);

        // Header
        ctx.push_text_run(TextRun {
            x: content_x,
            y: scroll_top + 40.0,
            text: "Identity & Security".to_string(),
            font_size: theme::FONT_DISPLAY,
            color: theme::LT_TEXT_PRIMARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: content_x,
            y: scroll_top + 74.0,
            text: "Manage your digital footprint and application settings.".to_string(),
            font_size: theme::FONT_BODY,
            color: theme::LT_TEXT_MUTED,
            monospace: false,
        });

        // Main Card Surface bounding all attributes
        let card_y = scroll_top + 110.0;
        let card_h = (self.max_scroll + self.rect.h) - 200.0; // Approximation of content depth
        let card_rect = Rect::new(content_x, card_y, content_w, card_h.max(680.0));

        ctx.push_glass_quad(theme::launcher_hero_surface(card_rect));

        // Drawing Helper
        let draw_row = |ctx: &mut PaintContext, y: f32, title: &str, subtitle: &str| {
            ctx.push_text_run(TextRun {
                x: content_x + 32.0,
                y: y + 20.0,
                text: title.to_string(),
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_PRIMARY,
                monospace: false,
            });
            ctx.push_text_run(TextRun {
                x: content_x + 32.0,
                y: y + 42.0,
                text: subtitle.to_string(),
                font_size: theme::FONT_CAPTION,
                color: theme::LT_TEXT_MUTED,
                monospace: false,
            });
        };

        let draw_separator = |ctx: &mut PaintContext, y: f32| {
            ctx.push_glass_quad(theme::launcher_inner_separator(Rect::new(
                content_x + 32.0,
                y,
                content_w - 64.0,
                1.0,
            )));
        };

        let mut cy = card_y;

        // Identity Path
        draw_row(
            ctx,
            cy,
            "Identity Path",
            "Your unique cryptographic identifier.",
        );
        let id_badge_w = theme::text_width(&self.identity_path, theme::FONT_LABEL) + 32.0;
        ctx.push_glass_quad(theme::glass_quad(
            Rect::new(
                content_x + content_w - id_badge_w - 32.0,
                cy + 12.0,
                id_badge_w,
                36.0,
            ),
            [1.0, 1.0, 1.0, 0.70],
            [0.0, 0.0, 0.0, 0.06],
            theme::CONTROL_RADIUS,
        ));
        ctx.push_text_run(TextRun {
            x: content_x + content_w - id_badge_w - 16.0,
            y: cy + 24.0,
            text: self.identity_path.clone(),
            font_size: theme::FONT_LABEL,
            color: theme::PRIMARY_BLUE,
            monospace: true,
        });

        cy += 74.0;
        draw_separator(ctx, cy);
        cy += ROW_GAP;

        // Device Trust
        draw_row(
            ctx,
            cy,
            "Device Trust",
            "Validation status of this hardware endpoint.",
        );
        let trust_badge = Rect::new(content_x + content_w * 0.35, cy + 12.0, 110.0, 24.0);
        ctx.push_glass_quad(theme::launcher_status_chip(
            trust_badge,
            theme::ChipTone::Success,
        ));
        ctx.push_text_run(TextRun {
            x: trust_badge.x + 12.0,
            y: trust_badge.y + 5.0,
            text: "Trusted Device".to_string(),
            font_size: theme::FONT_CAPTION,
            color: theme::launcher_chip_text_color(theme::ChipTone::Success),
            monospace: false,
        });

        cy += 74.0;
        draw_separator(ctx, cy);
        cy += ROW_GAP;

        // Streaming Defaults
        draw_row(
            ctx,
            cy,
            "Streaming Defaults",
            "Balance latency and fidelity.",
        );
        self.default_profile_dropdown.paint(ctx);

        cy += 84.0;
        draw_separator(ctx, cy);
        cy += ROW_GAP;

        // Input
        let right_x = content_x + content_w * 0.35;
        let right_w = content_w * 0.65 - 32.0;

        draw_row(
            ctx,
            cy,
            "Input",
            "Configure how local peripherals interact.",
        );
        ctx.push_glass_quad(theme::launcher_toggle_card_surface(
            Rect::new(right_x, cy, right_w, 56.0),
            0.30,
        ));
        ctx.push_text_run(TextRun {
            x: right_x + 16.0,
            y: cy + 16.0,
            text: "Exclusive Keyboard Capture".to_string(),
            font_size: theme::FONT_LABEL,
            color: theme::LT_TEXT_PRIMARY,
            monospace: false,
        });
        self.exclusive_keyboard_toggle.paint(ctx);
        cy += 64.0;

        ctx.push_glass_quad(theme::launcher_toggle_card_surface(
            Rect::new(right_x, cy, right_w, 56.0),
            0.30,
        ));
        ctx.push_text_run(TextRun {
            x: right_x + 16.0,
            y: cy + 16.0,
            text: "Relative Mouse Movement".to_string(),
            font_size: theme::FONT_LABEL,
            color: theme::LT_TEXT_PRIMARY,
            monospace: false,
        });
        self.relative_mouse_toggle.paint(ctx);

        cy += 84.0;
        draw_separator(ctx, cy);
        cy += ROW_GAP;

        // Audio
        draw_row(
            ctx,
            cy,
            "Audio",
            "Route sound between local and remote boundaries.",
        );
        let audio_label_color = theme::LT_TEXT_MUTED;
        ctx.push_text_run(TextRun {
            x: right_x,
            y: cy + 6.0,
            text: "REMOTE OUTPUT".to_string(),
            font_size: 10.0,
            color: audio_label_color,
            monospace: false,
        });
        self.audio_output_dropdown.paint(ctx);

        cy += 76.0;
        ctx.push_text_run(TextRun {
            x: right_x,
            y: cy + 6.0,
            text: "LOCAL MIC PATH".to_string(),
            font_size: 10.0,
            color: audio_label_color,
            monospace: false,
        });
        self.mic_dropdown.paint(ctx);

        // Versioning watermark at the bottom
        let watermark_y = card_y + card_h + 30.0;
        ctx.push_text_run(TextRun {
            x: content_x + (content_w - 200.0) / 2.0,
            y: watermark_y,
            text: format!("PRISM Professional Edition • {}", self.version),
            font_size: 10.0,
            color: [
                theme::LT_TEXT_PRIMARY[0],
                theme::LT_TEXT_PRIMARY[1],
                theme::LT_TEXT_PRIMARY[2],
                0.3,
            ],
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Handle scroll behavior at container level
        if let UiEvent::Scroll { dy, .. } = event {
            self.scroll_y = (self.scroll_y - dy).clamp(0.0, self.max_scroll);
            return EventResponse::Consumed;
        }

        let old_profile = self.default_profile_dropdown.selected_index();
        let profile_resp = self.default_profile_dropdown.handle_event(event);
        if self.default_profile_dropdown.selected_index() != old_profile {
            self.persist_user_prefs();
            return EventResponse::Consumed;
        }
        if !matches!(profile_resp, EventResponse::Ignored) {
            return profile_resp;
        }

        let old_exclusive = self.exclusive_keyboard_toggle.is_on();
        let exclusive_resp = self.exclusive_keyboard_toggle.handle_event(event);
        if self.exclusive_keyboard_toggle.is_on() != old_exclusive {
            self.persist_user_prefs();
            return EventResponse::Consumed;
        }
        if !matches!(exclusive_resp, EventResponse::Ignored) {
            return exclusive_resp;
        }

        let old_relative = self.relative_mouse_toggle.is_on();
        let relative_resp = self.relative_mouse_toggle.handle_event(event);
        if self.relative_mouse_toggle.is_on() != old_relative {
            self.persist_user_prefs();
            return EventResponse::Consumed;
        }
        if !matches!(relative_resp, EventResponse::Ignored) {
            return relative_resp;
        }

        let audio_resp = self.audio_output_dropdown.handle_event(event);
        if !matches!(audio_resp, EventResponse::Ignored) {
            return audio_resp;
        }

        let mic_resp = self.mic_dropdown.handle_event(event);
        if !matches!(mic_resp, EventResponse::Ignored) {
            return mic_resp;
        }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        self.default_profile_dropdown.animate(dt_ms);
        self.exclusive_keyboard_toggle.animate(dt_ms);
        self.relative_mouse_toggle.animate(dt_ms);
        self.audio_output_dropdown.animate(dt_ms);
        self.mic_dropdown.animate(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_panel_paints_single_card() {
        let mut panel = SettingsPanel::new(
            "/home/user/.prism/client_identity.json".to_string(),
            "0.1.0".to_string(),
        );
        panel.layout(Rect::new(0.0, 0.0, 900.0, 720.0));

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        // Core UI elements verify single card surface + internal dividers and backgrounds
        assert!(ctx.glass_quads.len() >= 4);
    }

    #[test]
    fn scrolling_updates_offset() {
        let mut panel = SettingsPanel::new("id".to_string(), "0.1.0".to_string());
        panel.layout(Rect::new(0.0, 0.0, 900.0, 300.0)); // Small height to induce max_scroll bounds

        // Dispatch a scroll event
        let resp = panel.handle_event(&UiEvent::Scroll { dx: 0.0, dy: -20.0 });

        assert!(matches!(resp, EventResponse::Consumed));
        assert!(panel.scroll_y > 0.0);
    }
}
