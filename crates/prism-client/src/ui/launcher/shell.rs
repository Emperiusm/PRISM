// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher shell - sidebar, content area, header, and modal layer.

use super::{ActiveModal, FormMode, LauncherTab};
use crate::config::servers::SavedServer;
use crate::ui::UiState;
use crate::ui::launcher::card_grid::{CardGrid, GridMode};
use crate::ui::launcher::nav::LauncherNav;
use crate::ui::launcher::profiles::ProfilesPanel;
use crate::ui::launcher::quick_connect::QuickConnect;
use crate::ui::launcher::recent_list::RecentList;
use crate::ui::launcher::server_form::ServerForm;
use crate::ui::launcher::settings::SettingsPanel;
use crate::ui::theme;
use crate::ui::widgets::icon::{ICON_SEARCH, Icon};
use crate::ui::widgets::{
    EventResponse, GlassQuad, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};

const SIDEBAR_W: f32 = 224.0;
const CONTENT_PAD: f32 = 28.0;
const HEADER_H: f32 = 48.0;

pub struct LauncherShell {
    nav: LauncherNav,
    quick_connect: QuickConnect,
    recent_list: RecentList,
    card_grid: CardGrid,
    profiles_panel: ProfilesPanel,
    settings_panel: SettingsPanel,
    server_form: ServerForm,
    active_tab: LauncherTab,
    active_modal: Option<ActiveModal>,
    screen_rect: Rect,
    sidebar_rect: Rect,
    content_rect: Rect,
    home_recent_y: f32,
    home_scroll_y: f32,
    home_max_scroll: f32,
    ui_state: UiState,
    /// Keyboard focus index — cycles through interactive widgets with Tab/Shift+Tab.
    focused_widget: Option<usize>,
}

impl LauncherShell {
    pub fn new(
        nav: LauncherNav,
        quick_connect: QuickConnect,
        recent_list: RecentList,
        card_grid: CardGrid,
        profiles_panel: ProfilesPanel,
        settings_panel: SettingsPanel,
        server_form: ServerForm,
    ) -> Self {
        let mut shell = Self {
            nav,
            quick_connect,
            recent_list,
            card_grid,
            profiles_panel,
            settings_panel,
            server_form,
            active_tab: LauncherTab::Home,
            active_modal: None,
            screen_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            sidebar_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            content_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            home_recent_y: 0.0,
            home_scroll_y: 0.0,
            home_max_scroll: 0.0,
            ui_state: UiState::Launcher,
            focused_widget: None,
        };
        shell.configure_widgets();
        shell.nav.set_active_tab(shell.active_tab);
        shell
    }

    pub fn active_tab(&self) -> LauncherTab {
        self.active_tab
    }

    pub fn set_tab(&mut self, tab: LauncherTab) {
        self.active_tab = tab;
        if tab == LauncherTab::Home {
            self.card_grid.reset_filter();
        }
        self.nav.set_active_tab(tab);
        self.configure_widgets();
        if self.screen_rect.w > 0.0 && self.screen_rect.h > 0.0 {
            self.layout(self.screen_rect);
        }
    }

    pub fn set_ui_state(&mut self, state: UiState) {
        self.ui_state = state;
    }

    pub fn set_servers(&mut self, servers: &[SavedServer]) {
        self.recent_list.set_servers(servers);
        self.card_grid.set_servers(servers);
        if self.screen_rect.w > 0.0 && self.screen_rect.h > 0.0 {
            self.layout(self.screen_rect);
        }
    }

    pub fn show_modal(&mut self, modal: ActiveModal) {
        match &modal {
            ActiveModal::ServerForm { mode } => match mode {
                FormMode::Add => {
                    self.server_form.clear();
                }
                FormMode::Edit { .. } => {}
            },
            ActiveModal::ConfirmDelete { .. } => {}
        }
        if matches!(modal, ActiveModal::ServerForm { .. }) {
            self.server_form.show();
        } else {
            self.server_form.hide();
        }
        self.active_modal = Some(modal);
    }

    pub fn dismiss_modal(&mut self) {
        self.server_form.hide();
        self.active_modal = None;
    }

    pub fn has_modal(&self) -> bool {
        self.active_modal.is_some()
    }

    pub fn active_modal(&self) -> Option<&ActiveModal> {
        self.active_modal.as_ref()
    }

    pub fn set_server_form_editing(&mut self, server: &SavedServer) {
        self.server_form.set_editing(server);
    }

    pub fn server_form_data(&self) -> Option<(String, String, Option<String>, String)> {
        matches!(
            self.active_modal,
            Some(ActiveModal::ServerForm {
                mode: FormMode::Add | FormMode::Edit { .. }
            })
        )
        .then(|| self.server_form.form_data())
    }

    pub fn server_form_editing_id(&self) -> Option<uuid::Uuid> {
        self.server_form.editing_id()
    }

    fn configure_widgets(&mut self) {
        // Reset focus when switching tabs
        self.focused_widget = None;
        self.apply_focus();

        match self.active_tab {
            LauncherTab::Home => {
                // Home uses RecentList, not CardGrid
            }
            LauncherTab::SavedConnections => {
                self.card_grid.set_layout_mode(GridMode::Grid);
                self.card_grid.set_visible_limit(None);
                self.card_grid.set_show_add_card(true);
                self.card_grid.set_show_filters(true);
            }
            LauncherTab::Profiles | LauncherTab::Settings => {
                self.card_grid.set_visible_limit(None);
                self.card_grid.set_show_add_card(false);
                self.card_grid.set_show_filters(false);
            }
        }
    }

    fn compute_layout(&mut self, screen_w: f32, screen_h: f32) {
        self.screen_rect = Rect::new(0.0, 0.0, screen_w, screen_h);
        self.sidebar_rect = Rect::new(0.0, 0.0, SIDEBAR_W, screen_h);
        let content_x = SIDEBAR_W + CONTENT_PAD;
        self.content_rect = Rect::new(
            content_x,
            0.0,
            (screen_w - content_x - CONTENT_PAD).max(320.0),
            screen_h.max(320.0),
        );
    }

    fn content_body_rect(&self) -> Rect {
        Rect::new(
            self.content_rect.x,
            self.content_rect.y + HEADER_H + 16.0,
            self.content_rect.w,
            (self.content_rect.h - HEADER_H - 16.0).max(0.0),
        )
    }

    fn layout_active_tab(&mut self) {
        let body = self.content_body_rect();
        match self.active_tab {
            LauncherTab::Home => {
                let quick_y = body.y;

                let quick_size = self
                    .quick_connect
                    .layout(Rect::new(body.x, quick_y, body.w, 300.0));

                let section_y = quick_y + quick_size.h + 38.0;
                self.home_recent_y = section_y;
                let list_y = section_y + 34.0 - self.home_scroll_y;
                let list_h = (body.y + body.h - list_y).max(0.0);

                let list_size = self
                    .recent_list
                    .layout(Rect::new(body.x, list_y, body.w, list_h));

                // TASK-064: Compute scroll bounds for Home
                let total_h = quick_size.h + 38.0 + 34.0 + list_size.h + 40.0;
                let visible_h = body.h;
                self.home_max_scroll = (total_h - visible_h).max(0.0);
            }
            LauncherTab::SavedConnections => {
                self.card_grid.layout(body);
            }
            LauncherTab::Profiles => {
                self.profiles_panel.layout(body);
            }
            LauncherTab::Settings => {
                self.settings_panel.layout(body);
            }
        }
    }

    fn paint_header(&self, ctx: &mut PaintContext) {
        let bar_rect = Rect::new(self.content_rect.x, 0.0, self.content_rect.w, HEADER_H);

        // Page title — breadcrumb for Settings, plain title for others
        if self.active_tab == LauncherTab::Settings {
            let section_label = self.nav.active_section().label();
            let prefix = "Settings / ";
            ctx.push_text_run(TextRun {
                x: bar_rect.x + 16.0,
                y: bar_rect.y + 14.0,
                text: prefix.to_string(),
                font_size: theme::FONT_HEADLINE,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });
            let crumb_x = bar_rect.x + 16.0 + theme::text_width(prefix, theme::FONT_HEADLINE);
            ctx.push_text_run(TextRun {
                x: crumb_x,
                y: bar_rect.y + 14.0,
                text: section_label.to_string(),
                font_size: theme::FONT_HEADLINE,
                color: theme::LT_TEXT_PRIMARY,
                bold: true,
                ..Default::default()
            });
        } else {
            let title = self.active_tab.title();
            ctx.push_text_run(TextRun {
                x: bar_rect.x + 16.0,
                y: bar_rect.y + 14.0,
                text: title.to_string(),
                font_size: theme::FONT_HEADLINE,
                color: theme::LT_TEXT_PRIMARY,
                bold: true,
                ..Default::default()
            });
        }

        // Profiles tab: search input placeholder
        if self.active_tab == LauncherTab::Profiles {
            let search_w = 200.0;
            let search_x = bar_rect.x + bar_rect.w - search_w - 100.0;
            let search_rect = Rect::new(search_x, 8.0, search_w, 32.0);
            ctx.push_glass_quad(theme::launcher_control_surface(search_rect, false));
            Icon::new(ICON_SEARCH)
                .with_size(16.0)
                .with_color(theme::LT_TEXT_MUTED)
                .at(search_rect.x + 8.0, search_rect.y + 8.0)
                .paint(ctx);
            ctx.push_text_run(TextRun {
                x: search_rect.x + 28.0,
                y: search_rect.y + 9.0,
                text: "Search profiles...".into(),
                font_size: theme::FONT_LABEL,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
        }

        // Connecting status chip
        if self.ui_state == UiState::Connecting {
            let chip_text = "Connecting...";
            let chip_w = theme::text_width(chip_text, 12.0) + 28.0;
            let status_rect = Rect::new(
                bar_rect.x + bar_rect.w - chip_w - 100.0,
                bar_rect.y + 10.0,
                chip_w,
                28.0,
            );
            ctx.push_glass_quad(theme::launcher_status_chip(
                status_rect,
                theme::ChipTone::Accent,
            ));
            ctx.push_text_run(TextRun {
                x: status_rect.x + 14.0,
                y: status_rect.y + 6.0,
                text: chip_text.to_string(),
                font_size: 12.0,
                color: theme::launcher_chip_text_color(theme::ChipTone::Accent),
                ..Default::default()
            });
        }

        // Right side: PRISM text + avatar placeholder
        let prism_x = bar_rect.x + bar_rect.w - 120.0;
        ctx.push_text_run(TextRun {
            x: prism_x,
            y: bar_rect.y + 16.0,
            text: "PRISM".into(),
            font_size: theme::FONT_LABEL,
            color: theme::LT_TEXT_MUTED,
            ..Default::default()
        });

        // Avatar circle (32px)
        let avatar_x = bar_rect.x + bar_rect.w - 56.0;
        ctx.push_glass_quad(GlassQuad {
            rect: Rect::new(avatar_x, 8.0, 32.0, 32.0),
            tint: [0.8, 0.85, 0.9, 1.0],
            corner_radius: 16.0,
            ..Default::default()
        });
    }

    fn paint_active_tab(&self, ctx: &mut PaintContext) {
        let body = self.content_body_rect();
        match self.active_tab {
            LauncherTab::Home => {
                let section_y = self.home_recent_y;
                self.quick_connect.paint(ctx);
                ctx.push_text_run(TextRun {
                    x: body.x,
                    y: section_y,
                    text: "Recent Connections".to_string(),
                    font_size: 13.0,
                    color: theme::LT_TEXT_SECONDARY,
                    bold: true,
                    ..Default::default()
                });
                ctx.push_glass_quad(theme::launcher_separator(Rect::new(
                    body.x,
                    section_y + 20.0,
                    body.w,
                    1.0,
                )));
                self.recent_list.paint(ctx);
            }
            LauncherTab::SavedConnections => {
                self.card_grid.paint(ctx);
            }
            LauncherTab::Profiles => {
                self.profiles_panel.paint(ctx);
            }
            LauncherTab::Settings => {
                self.settings_panel.paint(ctx);
            }
        }
    }

    fn modal_panel_rect(&self) -> Rect {
        let panel_w = 360.0;
        let panel_h = 320.0;
        Rect::new(
            self.screen_rect.x + (self.screen_rect.w - panel_w) * 0.5,
            self.screen_rect.y + (self.screen_rect.h - panel_h) * 0.5,
            panel_w,
            panel_h,
        )
    }

    fn delete_modal_buttons(panel: Rect) -> (Rect, Rect) {
        let button_y = panel.y + panel.h - 58.0;
        let button_w = 128.0;
        let gap = 14.0;
        let total_w = button_w * 2.0 + gap;
        let start_x = panel.x + (panel.w - total_w) * 0.5;
        let cancel = Rect::new(start_x, button_y, button_w, 34.0);
        let confirm = Rect::new(start_x + button_w + gap, button_y, button_w, 34.0);
        (cancel, confirm)
    }

    fn paint_modal_layer(&self, ctx: &mut PaintContext) {
        if let Some(modal) = &self.active_modal {
            ctx.push_glass_quad(theme::modal_scrim(self.screen_rect));
            match modal {
                ActiveModal::ServerForm { .. } => {
                    self.server_form.paint(ctx);
                }
                ActiveModal::ConfirmDelete { name, .. } => {
                    let panel = self.modal_panel_rect();
                    ctx.push_glass_quad(theme::launcher_modal_surface(panel));
                    ctx.push_text_run(TextRun {
                        x: panel.x + 20.0,
                        y: panel.y + 24.0,
                        text: "Delete connection".to_string(),
                        font_size: theme::FONT_HEADLINE,
                        color: theme::LT_TEXT_PRIMARY,
                        ..Default::default()
                    });
                    ctx.push_text_run(TextRun {
                        x: panel.x + 20.0,
                        y: panel.y + 58.0,
                        text: format!("Are you sure you want to remove \"{name}\"?"),
                        font_size: theme::FONT_BODY,
                        color: theme::LT_TEXT_SECONDARY,
                        ..Default::default()
                    });
                    let (cancel_rect, confirm_rect) = Self::delete_modal_buttons(panel);
                    ctx.push_glass_quad(theme::glass_quad(
                        cancel_rect,
                        [1.0, 1.0, 1.0, 0.70],
                        [0.0, 0.0, 0.0, 0.08],
                        theme::CHIP_RADIUS,
                    ));
                    ctx.push_glass_quad(theme::glass_quad(
                        confirm_rect,
                        [theme::DANGER[0], theme::DANGER[1], theme::DANGER[2], 0.18],
                        [theme::DANGER[0], theme::DANGER[1], theme::DANGER[2], 0.24],
                        theme::CHIP_RADIUS,
                    ));
                    let cancel_label = "Cancel";
                    let delete_label = "Delete";
                    ctx.push_text_run(TextRun {
                        x: cancel_rect.x
                            + (cancel_rect.w - theme::text_width(cancel_label, 12.0)) * 0.5,
                        y: cancel_rect.y + 9.0,
                        text: cancel_label.to_string(),
                        font_size: 12.0,
                        color: theme::LT_TEXT_SECONDARY,
                        ..Default::default()
                    });
                    ctx.push_text_run(TextRun {
                        x: confirm_rect.x
                            + (confirm_rect.w - theme::text_width(delete_label, 12.0)) * 0.5,
                        y: confirm_rect.y + 9.0,
                        text: delete_label.to_string(),
                        font_size: 12.0,
                        color: [1.0, 1.0, 1.0, 1.0],
                        ..Default::default()
                    });
                }
            }
        }
    }

    /// Number of focusable widgets on the current tab.
    fn focusable_count(&self) -> usize {
        match self.active_tab {
            LauncherTab::Home => {
                self.quick_connect.focusable_count() + self.recent_list.focusable_count()
            }
            // Other tabs have no focusable widgets wired up yet
            _ => 0,
        }
    }

    /// Apply the current `focused_widget` index to the actual widget focus state.
    fn apply_focus(&mut self) {
        if self.active_tab == LauncherTab::Home {
            let qc_count = self.quick_connect.focusable_count();
            let idx = self.focused_widget;
            if let Some(i) = idx {
                if i < qc_count {
                    self.quick_connect.set_focus(Some(i));
                    self.recent_list.set_focus(None);
                } else {
                    self.quick_connect.set_focus(None);
                    self.recent_list.set_focus(Some(i - qc_count));
                }
            } else {
                self.quick_connect.set_focus(None);
                self.recent_list.set_focus(None);
            }
        } else {
            self.quick_connect.set_focus(None);
            self.recent_list.set_focus(None);
        }
    }
}

impl Widget for LauncherShell {
    fn layout(&mut self, available: Rect) -> Size {
        self.compute_layout(available.w, available.h);
        self.nav.layout(self.sidebar_rect);
        self.layout_active_tab();

        if matches!(self.active_modal, Some(ActiveModal::ServerForm { .. })) {
            self.server_form.layout(self.modal_panel_rect());
        }

        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        self.nav.paint(ctx);
        self.paint_header(ctx);
        self.paint_active_tab(ctx);
        self.paint_modal_layer(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        if self.has_modal() {
            if matches!(self.active_modal, Some(ActiveModal::ServerForm { .. })) {
                let resp = self.server_form.handle_event(event);
                if !matches!(resp, EventResponse::Ignored) {
                    return resp;
                }
            }

            if let Some(ActiveModal::ConfirmDelete { server_id, .. }) = &self.active_modal
                && let UiEvent::MouseDown {
                    x,
                    y,
                    button: MouseButton::Left,
                } = event
            {
                let panel = self.modal_panel_rect();
                let (cancel_rect, confirm_rect) = Self::delete_modal_buttons(panel);
                if cancel_rect.contains(*x, *y) {
                    return EventResponse::Action(UiAction::CancelModal);
                }
                if confirm_rect.contains(*x, *y) {
                    return EventResponse::Action(UiAction::ConfirmDeleteServer(*server_id));
                }
            }

            if let UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } = event
            {
                let panel = self.modal_panel_rect();
                if !panel.contains(*x, *y) {
                    self.dismiss_modal();
                    return EventResponse::Consumed;
                }
            }
            return EventResponse::Ignored;
        }

        // TASK-076: Keyboard focus — Tab/Shift+Tab cycles through interactive widgets
        if let UiEvent::KeyDown { key } = event {
            use crate::ui::widgets::KeyCode;
            match key {
                KeyCode::Tab => {
                    let max = self.focusable_count();
                    if max > 0 {
                        self.focused_widget =
                            Some(self.focused_widget.map_or(0, |i| (i + 1) % max));
                        self.apply_focus();
                    }
                    return EventResponse::Consumed;
                }
                KeyCode::ShiftTab => {
                    let max = self.focusable_count();
                    if max > 0 {
                        self.focused_widget = Some(
                            self.focused_widget
                                .map_or(max - 1, |i| if i == 0 { max - 1 } else { i - 1 }),
                        );
                        self.apply_focus();
                    }
                    return EventResponse::Consumed;
                }
                KeyCode::Escape => {
                    if self.has_modal() {
                        self.dismiss_modal();
                        return EventResponse::Consumed;
                    }
                    self.focused_widget = None;
                    self.apply_focus();
                    return EventResponse::Consumed;
                }
                _ => {}
            }
        }

        // Mouse click clears keyboard focus
        if matches!(event, UiEvent::MouseDown { .. }) && self.focused_widget.is_some() {
            self.focused_widget = None;
            self.apply_focus();
        }

        let nav_resp = self.nav.handle_event(event);
        if let EventResponse::Action(UiAction::OpenLauncherTab(tab)) = &nav_resp {
            self.set_tab(*tab);
        }
        if let EventResponse::Action(UiAction::OpenSettingsSection(section)) = &nav_resp {
            self.nav.set_active_section(*section);
            if self.screen_rect.w > 0.0 && self.screen_rect.h > 0.0 {
                self.layout(self.screen_rect);
            }
        }
        if !matches!(nav_resp, EventResponse::Ignored) {
            return nav_resp;
        }

        match self.active_tab {
            LauncherTab::Home => {
                let quick_resp = self.quick_connect.handle_event(event);
                if !matches!(quick_resp, EventResponse::Ignored) {
                    return quick_resp;
                }
                let list_resp = self.recent_list.handle_event(event);
                if !matches!(list_resp, EventResponse::Ignored) {
                    return list_resp;
                }
                // TASK-064: Scroll for Home content
                if let UiEvent::Scroll { dy, .. } = event
                    && self.home_max_scroll > 0.0
                {
                    self.home_scroll_y = (self.home_scroll_y - dy).clamp(0.0, self.home_max_scroll);
                    self.layout_active_tab();
                    return EventResponse::Consumed;
                }
                EventResponse::Ignored
            }
            LauncherTab::SavedConnections => self.card_grid.handle_event(event),
            LauncherTab::Profiles => self.profiles_panel.handle_event(event),
            LauncherTab::Settings => self.settings_panel.handle_event(event),
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.nav.animate(dt_ms);
        self.quick_connect.animate(dt_ms);
        self.recent_list.animate(dt_ms);
        self.card_grid.animate(dt_ms);
        self.profiles_panel.animate(dt_ms);
        self.settings_panel.animate(dt_ms);
        self.server_form.animate(dt_ms);
    }
}
