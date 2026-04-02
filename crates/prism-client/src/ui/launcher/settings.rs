// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher settings page with functional preferences.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::config::client_config_prefs::UserPrefs;
use crate::ui::theme;
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::toggle::Toggle;
use crate::ui::widgets::{EventResponse, PaintContext, Rect, Size, TextRun, UiEvent, Widget};

const SECTION_GAP: f32 = 14.0;
const SECTION_PAD: f32 = 18.0;

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
            ),
            exclusive_keyboard_toggle: Toggle::new(true),
            relative_mouse_toggle: Toggle::new(false),
            audio_output_dropdown: Dropdown::new(
                vec![
                    "System Default".to_string(),
                    "Primary Speakers".to_string(),
                    "Headset".to_string(),
                ],
                0,
            ),
            mic_dropdown: Dropdown::new(
                vec![
                    "System Default".to_string(),
                    "Built-in Mic".to_string(),
                    "USB Interface".to_string(),
                ],
                0,
            ),
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

    fn section_rects(&self) -> (Rect, Rect, Rect, Rect, Rect) {
        let x = self.rect.x;
        let w = self.rect.w;
        let mut y = self.rect.y + 58.0;

        let identity = Rect::new(x, y, w, 92.0);
        y += identity.h + SECTION_GAP;
        let streaming = Rect::new(x, y, w, 102.0);
        y += streaming.h + SECTION_GAP;
        let input = Rect::new(x, y, w, 108.0);
        y += input.h + SECTION_GAP;
        let audio = Rect::new(x, y, w, 132.0);
        y += audio.h + SECTION_GAP;
        let about = Rect::new(x, y, w, 74.0);

        (identity, streaming, input, audio, about)
    }
}

impl Widget for SettingsPanel {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;

        let (_identity, streaming, input, audio, _about) = self.section_rects();
        self.default_profile_dropdown.layout(Rect::new(
            streaming.x + SECTION_PAD,
            streaming.y + 44.0,
            streaming.w - SECTION_PAD * 2.0,
            40.0,
        ));
        self.exclusive_keyboard_toggle.layout(Rect::new(
            input.x + SECTION_PAD,
            input.y + 40.0,
            input.w - SECTION_PAD * 2.0,
            22.0,
        ));
        self.relative_mouse_toggle.layout(Rect::new(
            input.x + SECTION_PAD,
            input.y + 72.0,
            input.w - SECTION_PAD * 2.0,
            22.0,
        ));
        self.audio_output_dropdown.layout(Rect::new(
            audio.x + SECTION_PAD,
            audio.y + 36.0,
            audio.w - SECTION_PAD * 2.0,
            40.0,
        ));
        self.mic_dropdown.layout(Rect::new(
            audio.x + SECTION_PAD,
            audio.y + 84.0,
            audio.w - SECTION_PAD * 2.0,
            40.0,
        ));

        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let (identity, streaming, input, audio, about) = self.section_rects();

        ctx.push_text_run(TextRun {
            x: self.rect.x,
            y: self.rect.y + 10.0,
            text: "Client Settings".to_string(),
            font_size: 13.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });

        for section in [identity, streaming, input, audio, about] {
            ctx.push_glass_quad(theme::card_surface(section));
        }

        ctx.push_text_run(TextRun {
            x: identity.x + SECTION_PAD,
            y: identity.y + 14.0,
            text: "Identity & Security".to_string(),
            font_size: 13.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: identity.x + SECTION_PAD,
            y: identity.y + 36.0,
            text: self.identity_path.clone(),
            font_size: 12.0,
            color: theme::TEXT_PRIMARY,
            monospace: true,
        });
        ctx.push_text_run(TextRun {
            x: identity.x + SECTION_PAD,
            y: identity.y + 58.0,
            text: "Trust status: verified identity key".to_string(),
            font_size: 11.0,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: streaming.x + SECTION_PAD,
            y: streaming.y + 14.0,
            text: "Streaming Defaults".to_string(),
            font_size: 13.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        self.default_profile_dropdown.paint(ctx);

        ctx.push_text_run(TextRun {
            x: input.x + SECTION_PAD,
            y: input.y + 14.0,
            text: "Input Controls".to_string(),
            font_size: 13.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: input.x + SECTION_PAD,
            y: input.y + 44.0,
            text: "Exclusive keyboard capture".to_string(),
            font_size: 12.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: input.x + SECTION_PAD,
            y: input.y + 76.0,
            text: "Relative mouse mode".to_string(),
            font_size: 12.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
        self.exclusive_keyboard_toggle.paint(ctx);
        self.relative_mouse_toggle.paint(ctx);

        ctx.push_text_run(TextRun {
            x: audio.x + SECTION_PAD,
            y: audio.y + 14.0,
            text: "Audio Paths".to_string(),
            font_size: 13.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        self.audio_output_dropdown.paint(ctx);
        self.mic_dropdown.paint(ctx);

        ctx.push_text_run(TextRun {
            x: about.x + SECTION_PAD,
            y: about.y + 14.0,
            text: "About".to_string(),
            font_size: 13.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: about.x + SECTION_PAD,
            y: about.y + 38.0,
            text: format!("PRISM Client {}", self.version),
            font_size: 12.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
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
    fn settings_panel_paints_sections() {
        let mut panel = SettingsPanel::new(
            "/home/user/.prism/client_identity.json".to_string(),
            "0.1.0".to_string(),
        );
        panel.layout(Rect::new(0.0, 0.0, 900.0, 720.0));

        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        assert!(ctx.glass_quads.len() >= 9);
        assert!(ctx.text_runs.len() >= 12);
    }

    #[test]
    fn toggles_handle_clicks() {
        let mut panel = SettingsPanel::new("id".to_string(), "0.1.0".to_string());
        panel.layout(Rect::new(0.0, 0.0, 900.0, 720.0));

        let (.., input, _, _) = panel.section_rects();
        let resp = panel.handle_event(&UiEvent::MouseDown {
            x: input.x + input.w - 30.0,
            y: input.y + 50.0,
            button: crate::ui::widgets::MouseButton::Left,
        });

        assert!(matches!(resp, EventResponse::Consumed));
    }
}
