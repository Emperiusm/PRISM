// SPDX-License-Identifier: AGPL-3.0-or-later
//! Interactive profile editor for launcher mode.

use std::sync::{Arc, Mutex};

use crate::config::profiles::{AudioMode, ProfileConfig, ProfileStore};
use crate::ui::theme;
use crate::ui::widgets::button::{Button, ButtonStyle};
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::segmented::SegmentedControl;
use crate::ui::widgets::slider::Slider;
use crate::ui::widgets::toggle::Toggle;
use crate::ui::widgets::{
    ColorMode, EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};
use prism_session::EncoderPreset;
use uuid::Uuid;

const LIST_W: f32 = 248.0;
const COL_GAP: f32 = 32.0;
const PANEL_PAD: f32 = 18.0;
const ROW_H: f32 = 54.0;
const ROW_GAP: f32 = 8.0;

pub struct ProfilesPanel {
    rect: Rect,
    profile_store: Option<Arc<Mutex<ProfileStore>>>,
    profiles: Vec<ProfileConfig>,
    selected_index: usize,
    draft: Option<ProfileConfig>,
    dirty: bool,
    list_rows: Vec<Rect>,
    bitrate_slider: Slider,
    fps_dropdown: Dropdown,
    encoder_dropdown: SegmentedControl,
    native_scaling_toggle: Toggle,
    audio_mode_dropdown: Dropdown,
    av1_toggle: Toggle,
    exclusive_input_toggle: Toggle,
    touch_mode_toggle: Toggle,
    auto_reconnect_toggle: Toggle,
    save_button: Button,
    discard_button: Button,
}

impl ProfilesPanel {
    pub fn new() -> Self {
        let profiles = Self::fallback_profiles();
        let draft = profiles.first().cloned();
        let initial = draft.clone().unwrap_or_else(Self::default_profile);

        let mut panel = Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            profile_store: None,
            profiles,
            selected_index: 0,
            draft,
            dirty: false,
            list_rows: Vec::new(),
            bitrate_slider: Slider::new("Bitrate", 5.0, 80.0, 35.0)
                .with_format(|v| format!("{} Mbps", v.round() as u32))
                .with_color_mode(ColorMode::Light),
            fps_dropdown: Dropdown::new(Self::fps_options(), 2).with_color_mode(ColorMode::Light),
            encoder_dropdown: SegmentedControl::new(Self::encoder_options(), 0)
                .with_color_mode(ColorMode::Light),
            native_scaling_toggle: Toggle::new(true).with_color_mode(ColorMode::Light),
            audio_mode_dropdown: Dropdown::new(Self::audio_options(), 0)
                .with_color_mode(ColorMode::Light),
            av1_toggle: Toggle::new(true).with_color_mode(ColorMode::Light),
            exclusive_input_toggle: Toggle::new(false).with_color_mode(ColorMode::Light),
            touch_mode_toggle: Toggle::new(false).with_color_mode(ColorMode::Light),
            auto_reconnect_toggle: Toggle::new(true).with_color_mode(ColorMode::Light),
            save_button: Button::new("Save", UiAction::SaveServer)
                .with_style(ButtonStyle::Primary)
                .with_color_mode(ColorMode::Light),
            discard_button: Button::new("Discard", UiAction::CancelModal)
                .with_style(ButtonStyle::Secondary)
                .with_color_mode(ColorMode::Light),
        };
        panel.sync_controls_from_profile(&initial);
        panel
    }

    pub fn set_profile_store(&mut self, store: Arc<Mutex<ProfileStore>>) {
        self.profile_store = Some(store);
        self.reload_profiles();
    }

    fn default_profile() -> ProfileConfig {
        ProfileConfig {
            id: Uuid::parse_str("33333333-3333-7333-8333-333333333333")
                .expect("valid default profile uuid"),
            name: "Balanced".to_string(),
            builtin: true,
            bitrate_bps: 25_000_000,
            max_fps: 90,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: true,
        }
    }

    fn fallback_profiles() -> Vec<ProfileConfig> {
        vec![
            ProfileConfig {
                id: Uuid::parse_str("11111111-1111-7111-8111-111111111111")
                    .expect("valid builtin profile uuid"),
                name: "Gaming".to_string(),
                builtin: true,
                bitrate_bps: 45_000_000,
                max_fps: 120,
                encoder_preset: EncoderPreset::UltraLowLatency,
                prefer_native_scaling: true,
                audio_mode: AudioMode::Stereo,
                prefer_av1: true,
                exclusive_input: true,
                touch_mode: false,
                auto_reconnect: true,
            },
            ProfileConfig {
                id: Uuid::parse_str("22222222-2222-7222-8222-222222222222")
                    .expect("valid builtin profile uuid"),
                name: "Coding".to_string(),
                builtin: true,
                bitrate_bps: 30_000_000,
                max_fps: 60,
                encoder_preset: EncoderPreset::Quality,
                prefer_native_scaling: true,
                audio_mode: AudioMode::Stereo,
                prefer_av1: true,
                exclusive_input: true,
                touch_mode: false,
                auto_reconnect: true,
            },
            Self::default_profile(),
            ProfileConfig {
                id: Uuid::parse_str("44444444-4444-7444-8444-444444444444")
                    .expect("valid builtin profile uuid"),
                name: "Low Bandwidth".to_string(),
                builtin: true,
                bitrate_bps: 8_000_000,
                max_fps: 45,
                encoder_preset: EncoderPreset::Balanced,
                prefer_native_scaling: false,
                audio_mode: AudioMode::VoiceOptimized,
                prefer_av1: false,
                exclusive_input: false,
                touch_mode: false,
                auto_reconnect: false,
            },
        ]
    }

    fn fps_options() -> Vec<String> {
        vec![
            "30".into(),
            "45".into(),
            "60".into(),
            "90".into(),
            "120".into(),
        ]
    }

    fn encoder_options() -> Vec<String> {
        vec![
            "Lowest Latency".into(),
            "Balanced".into(),
            "Highest Quality".into(),
        ]
    }

    fn audio_options() -> Vec<String> {
        vec!["Stereo".into(), "VoiceOptimized".into()]
    }

    fn list_rect(&self) -> Rect {
        Rect::new(self.rect.x, self.rect.y + 58.0, LIST_W, self.rect.h - 58.0)
    }

    fn editor_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + LIST_W + COL_GAP,
            self.rect.y,
            (self.rect.w - LIST_W - COL_GAP).max(0.0),
            self.rect.h,
        )
    }

    fn encoder_to_index(preset: &EncoderPreset) -> usize {
        match preset {
            EncoderPreset::UltraLowLatency => 0,
            EncoderPreset::Balanced => 1,
            EncoderPreset::Quality => 2,
        }
    }

    fn encoder_from_index(index: usize) -> EncoderPreset {
        match index {
            0 => EncoderPreset::UltraLowLatency,
            2 => EncoderPreset::Quality,
            _ => EncoderPreset::Balanced,
        }
    }

    fn audio_to_index(mode: &AudioMode) -> usize {
        match mode {
            AudioMode::Stereo => 0,
            AudioMode::VoiceOptimized => 1,
        }
    }

    fn audio_from_index(index: usize) -> AudioMode {
        match index {
            1 => AudioMode::VoiceOptimized,
            _ => AudioMode::Stereo,
        }
    }

    fn fps_to_index(max_fps: u8) -> usize {
        Self::fps_options()
            .iter()
            .position(|v| v.parse::<u8>().ok() == Some(max_fps))
            .unwrap_or(2)
    }

    fn selected_profile(&self) -> Option<&ProfileConfig> {
        self.profiles.get(self.selected_index)
    }

    fn reload_profiles(&mut self) {
        if let Some(store) = &self.profile_store
            && let Ok(guard) = store.lock()
        {
            self.profiles = guard.list().to_vec();
        }

        if self.profiles.is_empty() {
            self.profiles = Self::fallback_profiles();
        }
        if self.selected_index >= self.profiles.len() {
            self.selected_index = 0;
        }
        self.load_selected_profile();
    }

    fn load_selected_profile(&mut self) {
        if let Some(selected) = self.selected_profile().cloned() {
            self.draft = Some(selected.clone());
            self.sync_controls_from_profile(&selected);
            self.dirty = false;
        } else {
            self.draft = None;
            self.dirty = false;
        }
    }

    fn sync_controls_from_profile(&mut self, profile: &ProfileConfig) {
        self.bitrate_slider
            .set_value((profile.bitrate_bps as f32 / 1_000_000.0).clamp(5.0, 80.0));
        self.fps_dropdown
            .set_selected(Self::fps_to_index(profile.max_fps));
        self.encoder_dropdown
            .set_selected(Self::encoder_to_index(&profile.encoder_preset));
        self.native_scaling_toggle
            .set_on(profile.prefer_native_scaling);
        self.audio_mode_dropdown
            .set_selected(Self::audio_to_index(&profile.audio_mode));
        self.av1_toggle.set_on(profile.prefer_av1);
        self.exclusive_input_toggle.set_on(profile.exclusive_input);
        self.touch_mode_toggle.set_on(profile.touch_mode);
        self.auto_reconnect_toggle.set_on(profile.auto_reconnect);
    }

    fn apply_controls_to_draft(&mut self) {
        if let Some(draft) = self.draft.as_mut() {
            draft.bitrate_bps = (self.bitrate_slider.value().round().max(1.0) as u64) * 1_000_000;
            draft.max_fps = self
                .fps_dropdown
                .selected_text()
                .parse::<u8>()
                .unwrap_or(draft.max_fps);
            draft.encoder_preset = Self::encoder_from_index(self.encoder_dropdown.selected_index());
            draft.prefer_native_scaling = self.native_scaling_toggle.is_on();
            draft.audio_mode = Self::audio_from_index(self.audio_mode_dropdown.selected_index());
            draft.prefer_av1 = self.av1_toggle.is_on();
            draft.exclusive_input = self.exclusive_input_toggle.is_on();
            draft.touch_mode = self.touch_mode_toggle.is_on();
            draft.auto_reconnect = self.auto_reconnect_toggle.is_on();
        }

        self.dirty = matches!(
            (&self.draft, self.selected_profile()),
            (Some(draft), Some(selected)) if draft != selected
        );
    }

    fn save_draft(&mut self) {
        let Some(draft) = self.draft.clone() else {
            return;
        };

        if let Some(store) = &self.profile_store
            && let Ok(mut guard) = store.lock()
        {
            let result = if guard.get(draft.id).is_some() {
                guard.update(draft.id, draft.clone())
            } else {
                guard.add(draft.clone()).map(|_| ())
            };
            if result.is_ok() {
                self.profiles = guard.list().to_vec();
            }
        }

        if let Some(existing) = self.profiles.iter_mut().find(|p| p.id == draft.id) {
            *existing = draft.clone();
        }
        self.draft = Some(draft);
        self.dirty = false;
    }

    fn discard_draft(&mut self) {
        self.load_selected_profile();
    }
}

impl Default for ProfilesPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ProfilesPanel {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        if self.profiles.is_empty() {
            self.reload_profiles();
        }

        let list = self.list_rect();
        self.list_rows.clear();
        for i in 0..self.profiles.len() {
            self.list_rows.push(Rect::new(
                list.x + 12.0,
                list.y + 42.0 + i as f32 * (ROW_H + ROW_GAP),
                list.w - 24.0,
                ROW_H,
            ));
        }

        let editor = self.editor_rect();
        let x = editor.x + PANEL_PAD;
        let w = (editor.w - PANEL_PAD * 2.0).max(260.0);

        let header_h = 90.0;
        let right_edge = editor.x + w;
        let buttons_y = editor.y + 24.0;

        self.discard_button
            .layout(Rect::new(right_edge - 264.0, buttons_y, 120.0, 36.0));
        self.save_button
            .layout(Rect::new(right_edge - 132.0, buttons_y, 132.0, 36.0));

        let y_start = editor.y + header_h + 32.0;
        let col_w = ((w - 40.0) / 2.0).max(180.0);
        let col1_x = x;
        let col2_x = x + col_w + 40.0;

        let mut y = y_start;
        self.bitrate_slider
            .layout(Rect::new(col1_x, y + 20.0, col_w, 32.0));
        y += 70.0;
        self.encoder_dropdown
            .layout(Rect::new(col1_x, y + 20.0, col_w, 36.0));
        y += 70.0;
        self.native_scaling_toggle
            .layout(Rect::new(col1_x, y + 20.0, col_w, 22.0));
        y += 70.0;
        self.av1_toggle
            .layout(Rect::new(col1_x, y + 20.0, col_w, 22.0));

        let mut y = y_start;
        self.fps_dropdown
            .layout(Rect::new(col2_x, y + 20.0, col_w, 40.0));
        y += 70.0;
        self.audio_mode_dropdown
            .layout(Rect::new(col2_x, y + 20.0, col_w, 40.0));
        y += 70.0;
        self.exclusive_input_toggle
            .layout(Rect::new(col2_x, y + 20.0, col_w, 22.0));
        y += 70.0;
        self.touch_mode_toggle
            .layout(Rect::new(col2_x, y + 20.0, col_w, 22.0));
        y += 70.0;
        self.auto_reconnect_toggle
            .layout(Rect::new(col2_x, y + 20.0, col_w, 22.0));

        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let list = self.list_rect();
        let editor = self.editor_rect();
        ctx.push_glass_quad(theme::launcher_list_surface(list));
        ctx.push_glass_quad(theme::launcher_hero_surface(editor));

        ctx.push_text_run(TextRun {
            x: list.x + 18.0,
            y: list.y + 16.0,
            text: "Presets".into(),
            font_size: theme::FONT_LABEL,
            color: theme::LT_TEXT_MUTED,
            monospace: false,
        });

        for (idx, row) in self.list_rows.iter().enumerate() {
            let selected = idx == self.selected_index;
            let profile = &self.profiles[idx];
            ctx.push_glass_quad(theme::launcher_nav_item_surface(*row, selected, false));

            if selected {
                ctx.push_glass_quad(theme::glass_quad(
                    Rect::new(row.x, row.y, 4.0, row.h),
                    theme::PRIMARY_BLUE,
                    [
                        theme::PRIMARY_BLUE[0],
                        theme::PRIMARY_BLUE[1],
                        theme::PRIMARY_BLUE[2],
                        0.80,
                    ],
                    2.0,
                ));
            }

            let name_y = row.y + 14.0;
            ctx.push_text_run(TextRun {
                x: row.x + 16.0,
                y: name_y,
                text: profile.name.clone(),
                font_size: 14.0,
                color: if selected {
                    theme::LT_TEXT_PRIMARY
                } else {
                    theme::LT_TEXT_SECONDARY
                },
                monospace: false,
            });

            let subtitle = if profile.builtin {
                format!(
                    "Built-in • {} FPS • {} Mbps",
                    profile.max_fps,
                    profile.bitrate_bps / 1_000_000
                )
            } else {
                format!(
                    "{} FPS • {} Mbps",
                    profile.max_fps,
                    profile.bitrate_bps / 1_000_000
                )
            };

            ctx.push_text_run(TextRun {
                x: row.x + 16.0,
                y: row.y + 34.0,
                text: subtitle,
                font_size: 11.0,
                color: theme::LT_TEXT_MUTED,
                monospace: false,
            });
        }

        if let Some(draft) = &self.draft {
            ctx.push_glass_quad(theme::glass_quad(
                Rect::new(editor.x, editor.y, editor.w, 90.0),
                [1.0, 1.0, 1.0, 0.40],
                [0.0, 0.0, 0.0, 0.06],
                0.0,
            ));

            let tw = theme::text_width(&draft.name, theme::FONT_HERO);
            ctx.push_text_run(TextRun {
                x: editor.x + PANEL_PAD,
                y: editor.y + 20.0,
                text: draft.name.clone(),
                font_size: theme::FONT_HERO,
                color: theme::LT_TEXT_PRIMARY,
                monospace: false,
            });

            if draft.builtin {
                let badge = Rect::new(
                    editor.x + PANEL_PAD + tw + 16.0,
                    editor.y + 24.0,
                    60.0,
                    20.0,
                );
                ctx.push_glass_quad(theme::launcher_status_chip(badge, theme::ChipTone::Success));
                ctx.push_text_run(TextRun {
                    x: badge.x + 10.0,
                    y: badge.y + 3.0,
                    text: "SYSTEM".to_string(),
                    font_size: 10.0,
                    color: theme::launcher_chip_text_color(theme::ChipTone::Success),
                    monospace: false,
                });
            }

            if self.dirty {
                let badge_x =
                    editor.x + PANEL_PAD + tw + 16.0 + if draft.builtin { 70.0 } else { 0.0 };
                let chip = Rect::new(badge_x, editor.y + 24.0, 80.0, 20.0);
                ctx.push_glass_quad(theme::launcher_status_chip(chip, theme::ChipTone::Warning));
                ctx.push_text_run(TextRun {
                    x: chip.x + 10.0,
                    y: chip.y + 3.0,
                    text: "UNSAVED".to_string(),
                    font_size: 10.0,
                    color: theme::launcher_chip_text_color(theme::ChipTone::Warning),
                    monospace: false,
                });
            }

            ctx.push_text_run(TextRun {
                x: editor.x + PANEL_PAD,
                y: editor.y + 60.0,
                text: "Optimized for high-performance interaction".to_string(),
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_SECONDARY,
                monospace: false,
            });
        }

        let header_h = 90.0;
        let y_start = editor.y + header_h + 32.0;
        let col_w = ((editor.w - PANEL_PAD * 2.0 - 40.0) / 2.0).max(180.0);
        let col1_x = editor.x + PANEL_PAD;
        let col2_x = editor.x + PANEL_PAD + col_w + 40.0;

        let mut draw_label = |x, y, text: &str| {
            ctx.push_text_run(TextRun {
                x,
                y,
                text: text.to_string(),
                font_size: theme::FONT_CAPTION,
                color: theme::LT_TEXT_MUTED,
                monospace: false,
            });
        };

        let mut y = y_start;
        draw_label(col1_x, y, "Bitrate Preference");
        y += 70.0;
        draw_label(col1_x, y, "Latency vs Quality");
        y += 70.0;
        draw_label(col1_x, y, "Native Scaling");
        y += 70.0;
        draw_label(col1_x, y, "Prefer AV1");

        let mut y = y_start;
        draw_label(col2_x, y, "Max FPS");
        y += 70.0;
        draw_label(col2_x, y, "Audio Mode");
        y += 70.0;
        draw_label(col2_x, y, "Exclusive Input");
        y += 70.0;
        draw_label(col2_x, y, "Touch Mode");
        y += 70.0;
        draw_label(col2_x, y, "Auto Reconnect");

        self.bitrate_slider.paint(ctx);
        self.fps_dropdown.paint(ctx);
        self.encoder_dropdown.paint(ctx);
        self.native_scaling_toggle.paint(ctx);
        self.audio_mode_dropdown.paint(ctx);
        self.av1_toggle.paint(ctx);
        self.exclusive_input_toggle.paint(ctx);
        self.touch_mode_toggle.paint(ctx);
        self.auto_reconnect_toggle.paint(ctx);
        self.discard_button.paint(ctx);
        self.save_button.paint(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        if let UiEvent::MouseDown {
            x,
            y,
            button: MouseButton::Left,
        } = event
        {
            for (idx, row) in self.list_rows.iter().enumerate() {
                if row.contains(*x, *y) {
                    if self.selected_index != idx {
                        self.selected_index = idx;
                        self.load_selected_profile();
                    }
                    return EventResponse::Consumed;
                }
            }
        }

        let old_bitrate = self.bitrate_slider.value();
        let bitrate_resp = self.bitrate_slider.handle_event(event);
        if (self.bitrate_slider.value() - old_bitrate).abs() > f32::EPSILON {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(bitrate_resp, EventResponse::Ignored) {
            return bitrate_resp;
        }

        let old_fps = self.fps_dropdown.selected_index();
        let fps_resp = self.fps_dropdown.handle_event(event);
        if self.fps_dropdown.selected_index() != old_fps {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(fps_resp, EventResponse::Ignored) {
            return fps_resp;
        }

        let old_encoder = self.encoder_dropdown.selected_index();
        let encoder_resp = self.encoder_dropdown.handle_event(event);
        if self.encoder_dropdown.selected_index() != old_encoder {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(encoder_resp, EventResponse::Ignored) {
            return encoder_resp;
        }

        let old_native = self.native_scaling_toggle.is_on();
        let native_resp = self.native_scaling_toggle.handle_event(event);
        if self.native_scaling_toggle.is_on() != old_native {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(native_resp, EventResponse::Ignored) {
            return native_resp;
        }

        let old_audio = self.audio_mode_dropdown.selected_index();
        let audio_resp = self.audio_mode_dropdown.handle_event(event);
        if self.audio_mode_dropdown.selected_index() != old_audio {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(audio_resp, EventResponse::Ignored) {
            return audio_resp;
        }

        let old_av1 = self.av1_toggle.is_on();
        let av1_resp = self.av1_toggle.handle_event(event);
        if self.av1_toggle.is_on() != old_av1 {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(av1_resp, EventResponse::Ignored) {
            return av1_resp;
        }

        let old_exclusive = self.exclusive_input_toggle.is_on();
        let exclusive_resp = self.exclusive_input_toggle.handle_event(event);
        if self.exclusive_input_toggle.is_on() != old_exclusive {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(exclusive_resp, EventResponse::Ignored) {
            return exclusive_resp;
        }

        let old_touch = self.touch_mode_toggle.is_on();
        let touch_resp = self.touch_mode_toggle.handle_event(event);
        if self.touch_mode_toggle.is_on() != old_touch {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(touch_resp, EventResponse::Ignored) {
            return touch_resp;
        }

        let old_reconnect = self.auto_reconnect_toggle.is_on();
        let reconnect_resp = self.auto_reconnect_toggle.handle_event(event);
        if self.auto_reconnect_toggle.is_on() != old_reconnect {
            self.apply_controls_to_draft();
            return EventResponse::Consumed;
        }
        if !matches!(reconnect_resp, EventResponse::Ignored) {
            return reconnect_resp;
        }

        let save_resp = self.save_button.handle_event(event);
        if matches!(save_resp, EventResponse::Action(UiAction::SaveServer)) {
            self.save_draft();
            return EventResponse::Consumed;
        }
        if !matches!(save_resp, EventResponse::Ignored) {
            return save_resp;
        }

        let discard_resp = self.discard_button.handle_event(event);
        if matches!(discard_resp, EventResponse::Action(UiAction::CancelModal)) {
            self.discard_draft();
            return EventResponse::Consumed;
        }
        if !matches!(discard_resp, EventResponse::Ignored) {
            return discard_resp;
        }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        self.bitrate_slider.animate(dt_ms);
        self.fps_dropdown.animate(dt_ms);
        self.encoder_dropdown.animate(dt_ms);
        self.native_scaling_toggle.animate(dt_ms);
        self.audio_mode_dropdown.animate(dt_ms);
        self.av1_toggle.animate(dt_ms);
        self.exclusive_input_toggle.animate(dt_ms);
        self.touch_mode_toggle.animate(dt_ms);
        self.auto_reconnect_toggle.animate(dt_ms);
        self.save_button.animate(dt_ms);
        self.discard_button.animate(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_panel_paints_structure() {
        let mut panel = ProfilesPanel::new();
        panel.layout(Rect::new(0.0, 0.0, 900.0, 720.0));
        let mut ctx = PaintContext::new();
        panel.paint(&mut ctx);

        assert!(ctx.glass_quads.len() >= 10);
        assert!(ctx.text_runs.len() >= 12);
    }

    #[test]
    fn clicking_profile_row_changes_selection() {
        let mut panel = ProfilesPanel::new();
        panel.layout(Rect::new(0.0, 0.0, 900.0, 720.0));
        assert_eq!(panel.selected_index, 0);

        let second_row = panel.list_rows[1];
        let resp = panel.handle_event(&UiEvent::MouseDown {
            x: second_row.x + 8.0,
            y: second_row.y + 8.0,
            button: MouseButton::Left,
        });

        assert!(matches!(resp, EventResponse::Consumed));
        assert_eq!(panel.selected_index, 1);
    }
}
