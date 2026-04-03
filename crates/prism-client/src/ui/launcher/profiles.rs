// SPDX-License-Identifier: AGPL-3.0-or-later
//! Interactive profile editor for launcher mode.

use std::sync::{Arc, Mutex};

use crate::config::profiles::{AudioMode, ProfileConfig, ProfileStore};
use crate::ui::theme;
use crate::ui::widgets::button::{Button, ButtonStyle};
use crate::ui::widgets::dropdown::Dropdown;
use crate::ui::widgets::icon::{
    Icon, ICON_ADD, ICON_BALANCE, ICON_CODE, ICON_DIAL, ICON_GAMEPAD, ICON_KEYBOARD, ICON_MONITOR,
    ICON_SPEED,
};
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
    scroll_y: f32,
    max_scroll: f32,
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
            save_button: Button::new("Save Changes", UiAction::SaveServer)
                .with_style(ButtonStyle::Primary)
                .with_color_mode(ColorMode::Light)
                .with_radius(4.0),
            discard_button: Button::new("Discard", UiAction::CancelModal)
                .with_style(ButtonStyle::Text)
                .with_color_mode(ColorMode::Light),
            scroll_y: 0.0,
            max_scroll: 0.0,
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
        // Presets header sits above the card at y+10..y+40; card starts at y+48
        Rect::new(self.rect.x, self.rect.y + 48.0, LIST_W, self.rect.h - 48.0)
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

    /// Map profile name to an appropriate icon codepoint.
    fn profile_icon(name: &str) -> char {
        match name.to_lowercase().as_str() {
            "gaming" => ICON_GAMEPAD,
            "coding" => ICON_CODE,
            "balanced" => ICON_BALANCE,
            "low bandwidth" => ICON_SPEED,
            _ => ICON_DIAL,
        }
    }

    /// Computes rects for items in a 2-column grid layout.
    fn two_column_grid(
        items: usize,
        area_x: f32,
        area_y: f32,
        area_w: f32,
        row_h: f32,
        gap: f32,
    ) -> Vec<Rect> {
        let col_w = (area_w - gap) / 2.0;
        let mut rects = Vec::with_capacity(items);
        for i in 0..items {
            let col = i % 2;
            let row = i / 2;
            let x = area_x + col as f32 * (col_w + gap);
            let y = area_y + row as f32 * (row_h + gap);
            rects.push(Rect::new(x, y, col_w, row_h));
        }
        rects
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
                list.x,
                list.y + 8.0 + i as f32 * (ROW_H + ROW_GAP),
                list.w,
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

        // Section layout: PERFORMANCE SETTINGS, DISPLAY & AUDIO, INPUT & CONNECTIVITY
        let y_start = editor.y + header_h + 32.0 - self.scroll_y;

        // PERFORMANCE SETTINGS (single-column controls)
        let mut y = y_start + 32.0; // room for section header
        self.bitrate_slider
            .layout(Rect::new(x, y + 50.0, w, 32.0));
        y += 110.0;
        self.encoder_dropdown
            .layout(Rect::new(x, y + 20.0, w, 36.0));

        // DISPLAY & AUDIO - 2-column grid
        y += 90.0 + 32.0;
        let display_grid = Self::two_column_grid(4, x, y + 20.0, w, 40.0, 16.0);
        if display_grid.len() >= 4 {
            self.fps_dropdown.layout(display_grid[0]);
            self.audio_mode_dropdown.layout(display_grid[1]);
            self.native_scaling_toggle.layout(Rect::new(
                display_grid[2].x + 16.0,
                display_grid[2].y + 9.0,
                48.0,
                22.0,
            ));
            self.av1_toggle.layout(Rect::new(
                display_grid[3].x + 16.0,
                display_grid[3].y + 9.0,
                48.0,
                22.0,
            ));
        }

        // INPUT & CONNECTIVITY - 2-column toggle cards
        y += 20.0 + (2.0 * 40.0 + 16.0) + 40.0;
        let toggle_cards = Self::two_column_grid(3, x, y + 20.0, w, 80.0, 16.0);
        if toggle_cards.len() >= 3 {
            self.exclusive_input_toggle.layout(Rect::new(
                toggle_cards[0].x + toggle_cards[0].w - 64.0,
                toggle_cards[0].y + 16.0,
                48.0,
                22.0,
            ));
            self.touch_mode_toggle.layout(Rect::new(
                toggle_cards[1].x + toggle_cards[1].w - 64.0,
                toggle_cards[1].y + 16.0,
                48.0,
                22.0,
            ));
            self.auto_reconnect_toggle.layout(Rect::new(
                toggle_cards[2].x + toggle_cards[2].w - 64.0,
                toggle_cards[2].y + 16.0,
                48.0,
                22.0,
            ));
        }

        // Content height for scroll
        let total_content_h =
            (y + 20.0 + (2.0 * 80.0 + 16.0) + 40.0 + self.scroll_y) - (editor.y + header_h);
        let visible_h = editor.h - header_h;
        self.max_scroll = (total_content_h - visible_h).max(0.0);

        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let list = self.list_rect();
        let editor = self.editor_rect();

        // --- TASK-048: Presets header on gradient background (above list card) ---
        ctx.push_text_run(TextRun {
            x: self.rect.x + 4.0,
            y: self.rect.y + 10.0,
            text: "Presets".into(),
            font_size: theme::FONT_HEADLINE,
            color: theme::LT_TEXT_PRIMARY,
            bold: true,
            ..Default::default()
        });
        // "+" add button — blue circle
        let add_btn_x = self.rect.x + theme::text_width("Presets", theme::FONT_HEADLINE) + 16.0;
        let add_btn_y = self.rect.y + 8.0;
        ctx.push_glass_quad(theme::glass_quad(
            Rect::new(add_btn_x, add_btn_y, 28.0, 28.0),
            theme::PRIMARY_BLUE,
            theme::PRIMARY_BLUE,
            14.0,
        ));
        Icon::new(ICON_ADD)
            .with_size(16.0)
            .with_color([1.0, 1.0, 1.0, 1.0])
            .at(add_btn_x + 6.0, add_btn_y + 6.0)
            .paint(ctx);

        // List card surface — starts below header
        ctx.push_glass_quad(theme::launcher_list_surface(list));

        // Editor surface
        ctx.push_glass_quad(theme::launcher_hero_surface(editor));

        // --- TASK-049 + 050: List items with icons and active styling ---
        for (idx, row) in self.list_rows.iter().enumerate() {
            let selected = idx == self.selected_index;
            let profile = &self.profiles[idx];

            if selected {
                // TASK-050: White background + 4px PRIMARY_BLUE left bar
                ctx.push_glass_quad(theme::glass_quad(
                    *row,
                    [1.0, 1.0, 1.0, 1.0],
                    [0.0, 0.0, 0.0, 0.0],
                    0.0,
                ));
                ctx.push_glass_quad(theme::glass_quad(
                    Rect::new(row.x, row.y, 4.0, row.h),
                    theme::PRIMARY_BLUE,
                    theme::PRIMARY_BLUE,
                    0.0,
                ));
            }

            // TASK-049: Profile icon
            let icon_cp = Self::profile_icon(&profile.name);
            let icon_color = if selected {
                theme::PRIMARY_BLUE
            } else {
                theme::LT_TEXT_SECONDARY
            };
            Icon::new(icon_cp)
                .with_size(20.0)
                .with_color(icon_color)
                .at(row.x + 12.0, row.y + 10.0)
                .paint(ctx);

            // Profile name (offset for icon)
            let name_x = row.x + 40.0;
            ctx.push_text_run(TextRun {
                x: name_x,
                y: row.y + 10.0,
                text: profile.name.clone(),
                font_size: 14.0,
                color: if selected {
                    theme::LT_TEXT_PRIMARY
                } else {
                    theme::LT_TEXT_SECONDARY
                },
                ..Default::default()
            });

            // Subtitle line
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
                x: name_x,
                y: row.y + 28.0,
                text: subtitle,
                font_size: 11.0,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });

            // TASK-050: "Active" dot + text for selected item
            if selected {
                let dot_x = name_x;
                let dot_y = row.y + 42.0;
                ctx.push_glass_quad(theme::glass_quad(
                    Rect::new(dot_x, dot_y, 6.0, 6.0),
                    theme::SUCCESS,
                    theme::SUCCESS,
                    3.0,
                ));
                ctx.push_text_run(TextRun {
                    x: dot_x + 10.0,
                    y: dot_y - 2.0,
                    text: "Active".into(),
                    font_size: theme::FONT_LABEL,
                    color: theme::LT_TEXT_MUTED,
                    ..Default::default()
                });
            }
        }

        // --- TASK-051: Editor header with profile icon and chip ---
        if let Some(draft) = &self.draft {
            ctx.push_glass_quad(theme::glass_quad(
                Rect::new(editor.x, editor.y, editor.w, 90.0),
                [1.0, 1.0, 1.0, 0.40],
                [0.0, 0.0, 0.0, 0.06],
                0.0,
            ));

            // Profile icon (32px, blue)
            let icon_cp = Self::profile_icon(&draft.name);
            Icon::new(icon_cp)
                .with_size(32.0)
                .with_color(theme::PRIMARY_BLUE)
                .at(editor.x + PANEL_PAD, editor.y + 18.0)
                .paint(ctx);

            let name_x = editor.x + PANEL_PAD + 42.0;
            let tw = theme::text_width(&draft.name, theme::FONT_HERO);
            ctx.push_text_run(TextRun {
                x: name_x,
                y: editor.y + 20.0,
                text: draft.name.clone(),
                font_size: theme::FONT_HERO,
                color: theme::LT_TEXT_PRIMARY,
                ..Default::default()
            });

            // TASK-051: "ACTIVE" chip (green), not "SYSTEM" (blue)
            if draft.builtin {
                let badge = Rect::new(name_x + tw + 16.0, editor.y + 24.0, 64.0, 20.0);
                ctx.push_glass_quad(theme::launcher_status_chip(badge, theme::ChipTone::Success));
                ctx.push_text_run(TextRun {
                    x: badge.x + 10.0,
                    y: badge.y + 3.0,
                    text: "ACTIVE".to_string(),
                    font_size: 10.0,
                    color: theme::launcher_chip_text_color(theme::ChipTone::Success),
                    ..Default::default()
                });
            }

            if self.dirty {
                let badge_x =
                    name_x + tw + 16.0 + if draft.builtin { 74.0 } else { 0.0 };
                let chip = Rect::new(badge_x, editor.y + 24.0, 80.0, 20.0);
                ctx.push_glass_quad(theme::launcher_status_chip(chip, theme::ChipTone::Warning));
                ctx.push_text_run(TextRun {
                    x: chip.x + 10.0,
                    y: chip.y + 3.0,
                    text: "UNSAVED".to_string(),
                    font_size: 10.0,
                    color: theme::launcher_chip_text_color(theme::ChipTone::Warning),
                    ..Default::default()
                });
            }

            ctx.push_text_run(TextRun {
                x: name_x,
                y: editor.y + 60.0,
                text: "Optimized for high-performance interaction".to_string(),
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });
        }

        // --- TASK-053: Section grouping with icons ---
        let header_h = 90.0;
        let x = editor.x + PANEL_PAD;
        let w = (editor.w - PANEL_PAD * 2.0).max(260.0);
        let y_start = editor.y + header_h + 32.0 - self.scroll_y;

        // Section helper
        let paint_section_header = |ctx: &mut PaintContext, y: f32, icon: char, title: &str| {
            ctx.push_glass_quad(theme::launcher_inner_separator(Rect::new(
                x, y, w, 1.0,
            )));
            Icon::new(icon)
                .with_size(16.0)
                .with_color(theme::PRIMARY_BLUE)
                .at(x, y + 10.0)
                .paint(ctx);
            ctx.push_text_run(TextRun {
                x: x + 22.0,
                y: y + 10.0,
                text: title.to_string(),
                font_size: theme::FONT_LABEL,
                color: theme::LT_TEXT_SECONDARY,
                bold: true,
                ..Default::default()
            });
        };

        // ── PERFORMANCE SETTINGS ──
        paint_section_header(ctx, y_start, ICON_DIAL, "PERFORMANCE SETTINGS");

        let mut y = y_start + 32.0;

        // TASK-059: Bitrate slider display value
        let bitrate_mbps = self.bitrate_slider.value().round() as u32;
        let value_str = format!("{}", bitrate_mbps);
        ctx.push_text_run(TextRun {
            x: x,
            y: y,
            text: "Bitrate Preference".into(),
            font_size: theme::FONT_CAPTION,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });
        // Large value + MBPS suffix, right-aligned
        let val_w = theme::text_width(&value_str, theme::FONT_DISPLAY);
        ctx.push_text_run(TextRun {
            x: x + w - val_w - 50.0,
            y: y,
            text: value_str,
            font_size: theme::FONT_DISPLAY,
            color: theme::LT_TEXT_PRIMARY,
            bold: true,
            ..Default::default()
        });
        ctx.push_text_run(TextRun {
            x: x + w - 45.0,
            y: y + 10.0,
            text: "MBPS".into(),
            font_size: theme::FONT_LABEL,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });

        self.bitrate_slider.paint(ctx);

        // Range labels below slider
        let slider_rect = self.bitrate_slider.rect();
        ctx.push_text_run(TextRun {
            x: slider_rect.x,
            y: slider_rect.y + slider_rect.h + 4.0,
            text: "5 MBPS".into(),
            font_size: theme::FONT_CAPTION,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });
        ctx.push_text_run(TextRun {
            x: slider_rect.x + slider_rect.w - 50.0,
            y: slider_rect.y + slider_rect.h + 4.0,
            text: "80 MBPS".into(),
            font_size: theme::FONT_CAPTION,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });

        y += 110.0;
        ctx.push_text_run(TextRun {
            x: x,
            y: y,
            text: "Latency vs Quality".into(),
            font_size: theme::FONT_CAPTION,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });
        self.encoder_dropdown.paint(ctx);

        // ── DISPLAY & AUDIO ──
        y += 90.0;
        paint_section_header(ctx, y, ICON_MONITOR, "DISPLAY & AUDIO");
        y += 32.0;

        // TASK-055: 2-column grid for dropdowns + toggle controls
        let display_grid = Self::two_column_grid(4, x, y + 20.0, w, 40.0, 16.0);
        if display_grid.len() >= 4 {
            // Labels
            ctx.push_text_run(TextRun {
                x: display_grid[0].x,
                y: display_grid[0].y - 14.0,
                text: "Max FPS".into(),
                font_size: theme::FONT_CAPTION,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
            ctx.push_text_run(TextRun {
                x: display_grid[1].x,
                y: display_grid[1].y - 14.0,
                text: "Audio Mode".into(),
                font_size: theme::FONT_CAPTION,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
            ctx.push_text_run(TextRun {
                x: display_grid[2].x,
                y: display_grid[2].y - 14.0,
                text: "Native Scaling".into(),
                font_size: theme::FONT_CAPTION,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
            ctx.push_text_run(TextRun {
                x: display_grid[3].x,
                y: display_grid[3].y - 14.0,
                text: "Prefer AV1".into(),
                font_size: theme::FONT_CAPTION,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
        }
        self.fps_dropdown.paint(ctx);
        self.audio_mode_dropdown.paint(ctx);
        self.native_scaling_toggle.paint(ctx);
        self.av1_toggle.paint(ctx);

        // ── INPUT & CONNECTIVITY ──
        y += 20.0 + (2.0 * 40.0 + 16.0) + 8.0;
        paint_section_header(ctx, y, ICON_KEYBOARD, "INPUT & CONNECTIVITY");
        y += 32.0;

        // TASK-056: 2-column toggle cards
        let toggle_cards = Self::two_column_grid(3, x, y + 20.0, w, 80.0, 16.0);
        let toggle_items: [(&str, &str); 3] = [
            ("Exclusive Input", "Lock input to remote session"),
            ("Touch Mode", "Enable touch-based interaction"),
            ("Auto-Reconnect", "Rejoin on connection loss"),
        ];
        for (i, card_rect) in toggle_cards.iter().enumerate() {
            if i >= toggle_items.len() {
                break;
            }
            ctx.push_glass_quad(theme::launcher_toggle_card_surface(*card_rect, 0.60));
            ctx.push_text_run(TextRun {
                x: card_rect.x + 20.0,
                y: card_rect.y + 16.0,
                text: toggle_items[i].0.into(),
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_PRIMARY,
                bold: true,
                ..Default::default()
            });
            ctx.push_text_run(TextRun {
                x: card_rect.x + 20.0,
                y: card_rect.y + 36.0,
                text: toggle_items[i].1.into(),
                font_size: theme::FONT_LABEL,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
        }
        self.exclusive_input_toggle.paint(ctx);
        self.touch_mode_toggle.paint(ctx);
        self.auto_reconnect_toggle.paint(ctx);

        // Buttons
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

        // TASK-060: Scroll support for editor panel
        if let UiEvent::Scroll { dy, .. } = event {
            let editor = self.editor_rect();
            if editor.w > 0.0 && self.max_scroll > 0.0 {
                self.scroll_y = (self.scroll_y - dy).clamp(0.0, self.max_scroll);
                return EventResponse::Consumed;
            }
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
