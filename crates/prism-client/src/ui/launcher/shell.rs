// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher shell - sidebar, content area, header, and modal layer.

use super::{ActiveModal, FormMode, LauncherTab};
use crate::config::servers::SavedServer;
use crate::ui::launcher::card_grid::CardGrid;
use crate::ui::launcher::nav::LauncherNav;
use crate::ui::launcher::profiles::ProfilesPanel;
use crate::ui::launcher::quick_connect::QuickConnect;
use crate::ui::launcher::server_form::ServerForm;
use crate::ui::launcher::settings::SettingsPanel;
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};
use crate::ui::UiState;

const SIDEBAR_W: f32 = 224.0;
const SIDEBAR_PAD: f32 = 28.0;
const CONTENT_PAD: f32 = 28.0;
const HEADER_OFFSET: f32 = 92.0;

pub struct LauncherShell {
    nav: LauncherNav,
    quick_connect: QuickConnect,
    card_grid: CardGrid,
    profiles_panel: ProfilesPanel,
    settings_panel: SettingsPanel,
    server_form: ServerForm,
    active_tab: LauncherTab,
    active_modal: Option<ActiveModal>,
    screen_rect: Rect,
    sidebar_rect: Rect,
    content_rect: Rect,
    ui_state: UiState,
}

impl LauncherShell {
    pub fn new(
        nav: LauncherNav,
        quick_connect: QuickConnect,
        card_grid: CardGrid,
        profiles_panel: ProfilesPanel,
        settings_panel: SettingsPanel,
        server_form: ServerForm,
    ) -> Self {
        let mut shell = Self {
            nav,
            quick_connect,
            card_grid,
            profiles_panel,
            settings_panel,
            server_form,
            active_tab: LauncherTab::Home,
            active_modal: None,
            screen_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            sidebar_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            content_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            ui_state: UiState::Launcher,
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

    fn configure_widgets(&mut self) {
        match self.active_tab {
            LauncherTab::Home => {
                self.card_grid.set_visible_limit(Some(3));
                self.card_grid.set_show_add_card(false);
                self.card_grid.set_show_filters(false);
            }
            LauncherTab::SavedConnections => {
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
        self.sidebar_rect = Rect::new(
            SIDEBAR_PAD,
            SIDEBAR_PAD,
            SIDEBAR_W,
            (screen_h - SIDEBAR_PAD * 2.0).max(280.0),
        );
        let content_x = self.sidebar_rect.x + self.sidebar_rect.w + CONTENT_PAD;
        self.content_rect = Rect::new(
            content_x,
            42.0,
            (screen_w - content_x - CONTENT_PAD).max(320.0),
            (screen_h - 70.0).max(320.0),
        );
    }

    fn content_body_rect(&self) -> Rect {
        Rect::new(
            self.content_rect.x,
            self.content_rect.y + HEADER_OFFSET,
            self.content_rect.w,
            (self.content_rect.h - HEADER_OFFSET).max(0.0),
        )
    }

    fn layout_active_tab(&mut self) {
        let body = self.content_body_rect();
        match self.active_tab {
            LauncherTab::Home => {
                let quick_y = self.content_rect.y + HEADER_OFFSET;
                let section_y = quick_y + 132.0;
                let card_y = section_y + 34.0;

                self.quick_connect.layout(Rect::new(
                    self.content_rect.x,
                    quick_y,
                    self.content_rect.w,
                    94.0,
                ));
                self.card_grid.layout(Rect::new(
                    self.content_rect.x,
                    card_y,
                    self.content_rect.w,
                    (self.content_rect.y + self.content_rect.h - card_y).max(0.0),
                ));
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
        let title_x = self.content_rect.x;
        let title_y = self.content_rect.y + 6.0;
        ctx.push_text_run(TextRun {
            x: title_x,
            y: title_y,
            text: self.active_tab.title().to_string(),
            font_size: theme::FONT_DISPLAY,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: title_x,
            y: title_y + 40.0,
            text: self.active_tab.subtitle().to_string(),
            font_size: theme::FONT_BODY,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        if self.ui_state == UiState::Connecting {
            let chip_text = "Connecting...";
            let chip_w = theme::text_width(chip_text, 12.0) + 28.0;
            let status_rect = Rect::new(
                self.content_rect.x + self.content_rect.w - chip_w,
                title_y + 2.0,
                chip_w,
                28.0,
            );
            ctx.push_glass_quad(theme::status_chip(status_rect, theme::ChipTone::Accent));
            ctx.push_text_run(TextRun {
                x: status_rect.x + 14.0,
                y: status_rect.y + 6.0,
                text: chip_text.to_string(),
                font_size: 12.0,
                color: theme::TEXT_PRIMARY,
                monospace: false,
            });
        }
    }

    fn paint_active_tab(&self, ctx: &mut PaintContext) {
        match self.active_tab {
            LauncherTab::Home => {
                let section_y = self.content_rect.y + HEADER_OFFSET + 132.0;
                self.quick_connect.paint(ctx);
                ctx.push_text_run(TextRun {
                    x: self.content_rect.x,
                    y: section_y,
                    text: "Recent connections".to_string(),
                    font_size: 12.0,
                    color: theme::TEXT_MUTED,
                    monospace: false,
                });
                ctx.push_glass_quad(theme::separator(Rect::new(
                    self.content_rect.x,
                    section_y + 20.0,
                    self.content_rect.w,
                    1.0,
                )));
                self.card_grid.paint(ctx);
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

    fn paint_modal_layer(&self, ctx: &mut PaintContext) {
        if let Some(modal) = &self.active_modal {
            ctx.push_glass_quad(theme::modal_scrim(self.screen_rect));
            match modal {
                ActiveModal::ServerForm { .. } => {
                    self.server_form.paint(ctx);
                }
                ActiveModal::ConfirmDelete { name, .. } => {
                    let panel = self.modal_panel_rect();
                    ctx.push_glass_quad(theme::modal_surface(panel));
                    ctx.push_text_run(TextRun {
                        x: panel.x + 20.0,
                        y: panel.y + 24.0,
                        text: "Delete connection".to_string(),
                        font_size: theme::FONT_HEADLINE,
                        color: theme::TEXT_PRIMARY,
                        monospace: false,
                    });
                    ctx.push_text_run(TextRun {
                        x: panel.x + 20.0,
                        y: panel.y + 58.0,
                        text: format!("Are you sure you want to remove \"{name}\"?"),
                        font_size: theme::FONT_BODY,
                        color: theme::TEXT_SECONDARY,
                        monospace: false,
                    });
                }
            }
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

        let nav_resp = self.nav.handle_event(event);
        if let EventResponse::Action(UiAction::OpenLauncherTab(tab)) = &nav_resp {
            self.set_tab(*tab);
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
                self.card_grid.handle_event(event)
            }
            LauncherTab::SavedConnections => self.card_grid.handle_event(event),
            LauncherTab::Profiles => self.profiles_panel.handle_event(event),
            LauncherTab::Settings => self.settings_panel.handle_event(event),
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.nav.animate(dt_ms);
        self.quick_connect.animate(dt_ms);
        self.card_grid.animate(dt_ms);
        self.profiles_panel.animate(dt_ms);
        self.settings_panel.animate(dt_ms);
        self.server_form.animate(dt_ms);
    }
}
