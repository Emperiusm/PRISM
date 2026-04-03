// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sidebar navigation for launcher tabs.

use super::{LauncherTab, SettingsSection};
use crate::ui::theme;
use crate::ui::widgets::icon::{
    ICON_DEVICES, ICON_HOME, ICON_KEYBOARD, ICON_MENU, ICON_SETTINGS, ICON_SHIELD, ICON_SPEAKER,
    ICON_STREAMING, ICON_TUNE, Icon,
};
use crate::ui::widgets::{
    EventResponse, GlassQuad, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};

const ITEM_H: f32 = 40.0;
const ITEM_GAP: f32 = 8.0;
const NAV_TOP_Y: f32 = 94.0;
const NAV_ITEM_PAD_LEFT: f32 = 16.0;
const FOOTER_H: f32 = 44.0;
const FOOTER_BOTTOM_PAD: f32 = 16.0;
const SETTINGS_GAP: f32 = 12.0;

pub struct LauncherNav {
    rect: Rect,
    active_tab: LauncherTab,
    active_section: SettingsSection,
    hovered_tab: Option<LauncherTab>,
    hovered_section: Option<SettingsSection>,
    primary_items: Vec<(LauncherTab, Rect)>,
    settings_item: Rect,
    footer_rect: Rect,
    sub_nav_items: Vec<(SettingsSection, Rect)>,
}

impl LauncherNav {
    pub fn new() -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            active_tab: LauncherTab::Home,
            active_section: SettingsSection::IdentitySecurity,
            hovered_tab: None,
            hovered_section: None,
            primary_items: Vec::new(),
            settings_item: Rect::new(0.0, 0.0, 0.0, 0.0),
            footer_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            sub_nav_items: Vec::new(),
        }
    }

    pub fn set_active_tab(&mut self, tab: LauncherTab) {
        self.active_tab = tab;
    }

    pub fn set_active_section(&mut self, section: SettingsSection) {
        self.active_section = section;
    }

    pub fn active_section(&self) -> SettingsSection {
        self.active_section
    }

    fn item_rect(&self, index: usize) -> Rect {
        Rect::new(
            self.rect.x,
            self.rect.y + NAV_TOP_Y + index as f32 * (ITEM_H + ITEM_GAP),
            self.rect.w,
            ITEM_H,
        )
    }

    fn paint_hover_band(ctx: &mut PaintContext, rect: Rect) {
        ctx.push_glass_quad(GlassQuad {
            rect,
            tint: [0.898, 0.898, 0.898, 0.30],
            corner_radius: 0.0,
            ..Default::default()
        });
    }
}

impl Default for LauncherNav {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for LauncherNav {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        self.primary_items = LauncherTab::PRIMARY
            .iter()
            .enumerate()
            .map(|(index, tab)| (*tab, self.item_rect(index)))
            .collect();

        self.footer_rect = Rect::new(
            self.rect.x,
            self.rect.y + self.rect.h - FOOTER_BOTTOM_PAD - FOOTER_H,
            self.rect.w,
            FOOTER_H,
        );
        self.settings_item = Rect::new(
            self.rect.x,
            self.footer_rect.y - SETTINGS_GAP - ITEM_H,
            self.rect.w,
            ITEM_H,
        );

        // When Settings is active, compute sub-nav items after the primary ones
        self.sub_nav_items.clear();
        if self.active_tab == LauncherTab::Settings {
            let main_nav_bottom_y =
                self.rect.y + NAV_TOP_Y + LauncherTab::PRIMARY.len() as f32 * (ITEM_H + ITEM_GAP);
            let sub_header_y = main_nav_bottom_y + 16.0;
            for (i, section) in SettingsSection::ALL.iter().enumerate() {
                let item_y = sub_header_y + 24.0 + i as f32 * (ITEM_H + 4.0);
                let item_rect = Rect::new(self.rect.x, item_y, self.rect.w, ITEM_H);
                if item_rect.y + item_rect.h > self.settings_item.y - 8.0 {
                    break;
                }
                self.sub_nav_items.push((*section, item_rect));
            }
        }
        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        ctx.push_glass_quad(theme::launcher_sidebar_surface(self.rect));

        // Hamburger menu icon
        Icon::new(ICON_MENU)
            .with_size(24.0)
            .with_color(theme::LT_TEXT_SECONDARY)
            .at(self.rect.x + 18.0, self.rect.y + 30.0)
            .paint(ctx);

        // Settings tab: additional PRISM branding next to hamburger
        if self.active_tab == LauncherTab::Settings {
            ctx.push_text_run(TextRun {
                x: self.rect.x + 50.0,
                y: self.rect.y + 32.0,
                text: "PRISM".into(),
                font_size: theme::FONT_LABEL,
                color: theme::LT_TEXT_PRIMARY,
                bold: true,
                ..Default::default()
            });
        }

        for (tab, rect) in &self.primary_items {
            let hovered = self.hovered_tab == Some(*tab);
            let is_active = *tab == self.active_tab;
            if is_active {
                theme::paint_active_list_indicator(
                    &mut ctx.glass_quads,
                    *rect,
                    theme::PRIMARY_BLUE,
                );
            } else if hovered {
                Self::paint_hover_band(ctx, *rect);
            }

            let icon_codepoint = match tab {
                LauncherTab::Home => ICON_HOME,
                LauncherTab::SavedConnections => ICON_DEVICES,
                LauncherTab::Profiles => ICON_TUNE,
                LauncherTab::Settings => ICON_SETTINGS,
            };
            let icon_color = if is_active {
                theme::LT_TEXT_PRIMARY
            } else {
                theme::LT_TEXT_SECONDARY
            };

            Icon::new(icon_codepoint)
                .with_size(20.0)
                .with_color(icon_color)
                .at(rect.x + NAV_ITEM_PAD_LEFT, rect.y + 10.0)
                .paint(ctx);

            ctx.push_text_run(TextRun {
                x: rect.x + NAV_ITEM_PAD_LEFT + 28.0,
                y: rect.y + 11.0,
                text: tab.label().to_string(),
                font_size: 13.0,
                color: if is_active {
                    theme::LT_TEXT_PRIMARY
                } else {
                    theme::LT_TEXT_SECONDARY
                },
                ..Default::default()
            });
        }

        // Settings sub-nav (TASK-066, TASK-067)
        if self.active_tab == LauncherTab::Settings && !self.sub_nav_items.is_empty() {
            // "SETTINGS" header
            let header_y = self.sub_nav_items[0].1.y - 24.0;
            ctx.push_text_run(TextRun {
                x: self.rect.x + NAV_ITEM_PAD_LEFT,
                y: header_y,
                text: "SETTINGS".into(),
                font_size: theme::FONT_LABEL,
                color: theme::LT_TEXT_MUTED,
                bold: true,
                ..Default::default()
            });

            for (section, rect) in &self.sub_nav_items {
                let is_active = *section == self.active_section;
                let hovered = self.hovered_section == Some(*section);

                if is_active {
                    theme::paint_active_list_indicator(
                        &mut ctx.glass_quads,
                        *rect,
                        theme::PRIMARY_BLUE,
                    );
                } else if hovered {
                    Self::paint_hover_band(ctx, *rect);
                }

                let icon_codepoint = match section {
                    SettingsSection::General => ICON_SETTINGS,
                    SettingsSection::IdentitySecurity => ICON_SHIELD,
                    SettingsSection::Streaming => ICON_STREAMING,
                    SettingsSection::Input => ICON_KEYBOARD,
                    SettingsSection::Audio => ICON_SPEAKER,
                };
                let color = if is_active {
                    theme::LT_TEXT_PRIMARY
                } else {
                    theme::LT_TEXT_SECONDARY
                };

                Icon::new(icon_codepoint)
                    .with_size(18.0)
                    .with_color(color)
                    .at(rect.x + NAV_ITEM_PAD_LEFT, rect.y + 11.0)
                    .paint(ctx);

                ctx.push_text_run(TextRun {
                    x: rect.x + NAV_ITEM_PAD_LEFT + 24.0,
                    y: rect.y + 12.0,
                    text: section.label().into(),
                    font_size: theme::FONT_LABEL,
                    color,
                    ..Default::default()
                });
            }
        }

        let hovered = self.hovered_tab == Some(LauncherTab::Settings);
        let settings_active = self.active_tab == LauncherTab::Settings;
        if settings_active {
            theme::paint_active_list_indicator(
                &mut ctx.glass_quads,
                self.settings_item,
                theme::PRIMARY_BLUE,
            );
        } else if hovered {
            Self::paint_hover_band(ctx, self.settings_item);
        }

        let settings_icon_color = if settings_active {
            theme::LT_TEXT_PRIMARY
        } else {
            theme::LT_TEXT_SECONDARY
        };
        Icon::new(ICON_SETTINGS)
            .with_size(20.0)
            .with_color(settings_icon_color)
            .at(
                self.settings_item.x + NAV_ITEM_PAD_LEFT,
                self.settings_item.y + 10.0,
            )
            .paint(ctx);

        ctx.push_text_run(TextRun {
            x: self.settings_item.x + NAV_ITEM_PAD_LEFT + 28.0,
            y: self.settings_item.y + 11.0,
            text: "Settings".into(),
            font_size: 13.0,
            color: if settings_active {
                theme::LT_TEXT_PRIMARY
            } else {
                theme::LT_TEXT_SECONDARY
            },
            ..Default::default()
        });

        // Sidebar footer avatar (TASK-074) — rendered on all tabs
        ctx.push_glass_quad(GlassQuad {
            rect: Rect::new(
                self.footer_rect.x + NAV_ITEM_PAD_LEFT,
                self.footer_rect.y + 4.0,
                36.0,
                36.0,
            ),
            tint: [0.75, 0.82, 0.90, 1.0],
            corner_radius: 18.0,
            ..Default::default()
        });
        ctx.push_text_run(TextRun {
            x: self.footer_rect.x + NAV_ITEM_PAD_LEFT + 44.0,
            y: self.footer_rect.y + 14.0,
            text: "Verified Dev".into(),
            font_size: theme::FONT_LABEL,
            color: theme::LT_TEXT_PRIMARY,
            bold: true,
            ..Default::default()
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                self.hovered_tab = self
                    .primary_items
                    .iter()
                    .find_map(|(tab, rect)| rect.contains(*x, *y).then_some(*tab))
                    .or_else(|| {
                        self.settings_item
                            .contains(*x, *y)
                            .then_some(LauncherTab::Settings)
                    });
                self.hovered_section = self
                    .sub_nav_items
                    .iter()
                    .find_map(|(section, rect)| rect.contains(*x, *y).then_some(*section));
                EventResponse::Ignored
            }
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                // Sub-nav clicks take priority when Settings tab is active
                if let Some((section, _)) = self
                    .sub_nav_items
                    .iter()
                    .find(|(_, rect)| rect.contains(*x, *y))
                {
                    return EventResponse::Action(UiAction::OpenSettingsSection(*section));
                }

                if let Some((tab, _)) = self
                    .primary_items
                    .iter()
                    .find(|(_, rect)| rect.contains(*x, *y))
                {
                    return EventResponse::Action(UiAction::OpenLauncherTab(*tab));
                }

                if self.settings_item.contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenLauncherTab(LauncherTab::Settings));
                }

                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_home_returns_tab_action() {
        let mut nav = LauncherNav::new();
        nav.layout(Rect::new(0.0, 0.0, 220.0, 640.0));

        let resp = nav.handle_event(&UiEvent::MouseDown {
            x: 50.0,
            y: 110.0,
            button: MouseButton::Left,
        });

        assert!(matches!(
            resp,
            EventResponse::Action(UiAction::OpenLauncherTab(LauncherTab::Home))
        ));
    }
}
