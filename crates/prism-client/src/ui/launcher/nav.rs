// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sidebar navigation for launcher tabs.

use super::LauncherTab;
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

const ITEM_H: f32 = 40.0;
const ITEM_GAP: f32 = 8.0;
const SIDE_PADDING: f32 = 14.0;

pub struct LauncherNav {
    rect: Rect,
    active_tab: LauncherTab,
    hovered_tab: Option<LauncherTab>,
    primary_items: Vec<(LauncherTab, Rect)>,
    settings_item: Rect,
}

impl LauncherNav {
    pub fn new() -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            active_tab: LauncherTab::Home,
            hovered_tab: None,
            primary_items: Vec::new(),
            settings_item: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn set_active_tab(&mut self, tab: LauncherTab) {
        self.active_tab = tab;
    }

    fn item_rect(&self, index: usize) -> Rect {
        Rect::new(
            self.rect.x + SIDE_PADDING,
            self.rect.y + 94.0 + index as f32 * (ITEM_H + ITEM_GAP),
            self.rect.w - SIDE_PADDING * 2.0,
            ITEM_H,
        )
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
        self.settings_item = Rect::new(
            self.rect.x + SIDE_PADDING,
            self.rect.y + self.rect.h - 54.0,
            self.rect.w - SIDE_PADDING * 2.0,
            ITEM_H,
        );
        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        ctx.push_glass_quad(theme::sidebar_surface(self.rect));

        ctx.push_text_run(TextRun {
            x: self.rect.x + 18.0,
            y: self.rect.y + 22.0,
            text: "PRISM".into(),
            font_size: 20.0,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: self.rect.x + 18.0,
            y: self.rect.y + 48.0,
            text: "Remote client".into(),
            font_size: 11.0,
            color: theme::TEXT_MUTED,
            monospace: false,
        });

        for (tab, rect) in &self.primary_items {
            let hovered = self.hovered_tab == Some(*tab);
            if *tab == self.active_tab || hovered {
                ctx.push_glass_quad(theme::nav_item_surface(
                    *rect,
                    *tab == self.active_tab,
                    hovered,
                ));
            }

            ctx.push_text_run(TextRun {
                x: rect.x + 16.0,
                y: rect.y + 11.0,
                text: tab.label().to_string(),
                font_size: 13.0,
                color: if *tab == self.active_tab {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                },
                monospace: false,
            });
        }

        let hovered = self.hovered_tab == Some(LauncherTab::Settings);
        if self.active_tab == LauncherTab::Settings || hovered {
            ctx.push_glass_quad(theme::nav_item_surface(
                self.settings_item,
                self.active_tab == LauncherTab::Settings,
                hovered,
            ));
        }
        ctx.push_text_run(TextRun {
            x: self.settings_item.x + 16.0,
            y: self.settings_item.y + 11.0,
            text: "Settings".into(),
            font_size: 13.0,
            color: if self.active_tab == LauncherTab::Settings {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            },
            monospace: false,
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
                EventResponse::Ignored
            }
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                if let Some((tab, _)) = self
                    .primary_items
                    .iter()
                    .find(|(_, rect)| rect.contains(*x, *y))
                {
                    return EventResponse::Action(UiAction::OpenLauncherTab(*tab));
                }

                if self.settings_item.contains(*x, *y) {
                    return EventResponse::Action(UiAction::OpenLauncherTab(
                        LauncherTab::Settings,
                    ));
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
