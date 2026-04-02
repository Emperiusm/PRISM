// SPDX-License-Identifier: AGPL-3.0-or-later
//! Responsive saved-connections grid with filter chips.

use super::server_card::{CardFilter, ServerCard};
use crate::config::servers::SavedServer;
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

const CARD_WIDTH: f32 = ServerCard::WIDTH;
const CARD_HEIGHT: f32 = ServerCard::HEIGHT;
const CARD_GAP: f32 = 20.0;
const FILTER_H: f32 = 32.0;
const FILTER_GAP: f32 = 10.0;
const TOOLBAR_H: f32 = 52.0;

pub struct CardGrid {
    cards: Vec<ServerCard>,
    visible_indices: Vec<usize>,
    positions: Vec<Rect>,
    filter_chip_rects: Vec<(CardFilter, Rect)>,
    active_filter: CardFilter,
    hovered_filter: Option<CardFilter>,
    grid_width: f32,
    rect: Rect,
    visible_limit: Option<usize>,
    show_add_card: bool,
    show_filters: bool,
}

impl CardGrid {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            visible_indices: Vec::new(),
            positions: Vec::new(),
            filter_chip_rects: Vec::new(),
            active_filter: CardFilter::All,
            hovered_filter: None,
            grid_width: 800.0,
            rect: Rect::new(0.0, 0.0, 800.0, 600.0),
            visible_limit: None,
            show_add_card: true,
            show_filters: false,
        }
    }

    pub fn set_servers(&mut self, servers: &[SavedServer]) {
        let mut ordered = servers.to_vec();
        ordered.sort_by(|a, b| {
            b.last_connected
                .unwrap_or(b.created_at)
                .cmp(&a.last_connected.unwrap_or(a.created_at))
        });
        self.cards = ordered.iter().map(ServerCard::from_saved).collect();
        self.positions.clear();
    }

    pub fn set_visible_limit(&mut self, limit: Option<usize>) {
        if self.visible_limit != limit {
            self.visible_limit = limit;
            self.positions.clear();
        }
    }

    pub fn set_show_add_card(&mut self, show_add_card: bool) {
        if self.show_add_card != show_add_card {
            self.show_add_card = show_add_card;
            self.positions.clear();
        }
    }

    pub fn set_show_filters(&mut self, show_filters: bool) {
        if self.show_filters != show_filters {
            self.show_filters = show_filters;
            self.positions.clear();
            self.filter_chip_rects.clear();
        }
    }

    fn toolbar_height(&self) -> f32 {
        if self.show_filters {
            TOOLBAR_H
        } else {
            0.0
        }
    }

    fn visible_card_count(&self) -> usize {
        self.visible_limit
            .map(|limit| limit.min(self.visible_indices.len()))
            .unwrap_or(self.visible_indices.len())
    }

    fn total_items(&self) -> usize {
        self.visible_card_count() + usize::from(self.show_add_card)
    }

    fn recompute_visible_indices(&mut self) {
        self.visible_indices = self
            .cards
            .iter()
            .enumerate()
            .filter_map(|(index, card)| card.matches_filter(self.active_filter).then_some(index))
            .collect();
    }

    fn recompute_filter_chip_rects(&mut self) {
        self.filter_chip_rects.clear();
        if !self.show_filters {
            return;
        }

        let mut x = self.rect.x;
        let y = self.rect.y;
        for filter in [
            CardFilter::All,
            CardFilter::Recent,
            CardFilter::Dormant,
            CardFilter::New,
        ] {
            let label = filter.label(self.cards.len());
            let w = theme::text_width(&label, 11.0) + 28.0;
            let rect = Rect::new(x, y, w, FILTER_H);
            self.filter_chip_rects.push((filter, rect));
            x += w + FILTER_GAP;
        }
    }

    fn recompute_layout(&mut self) {
        self.positions.clear();
        self.recompute_visible_indices();
        self.recompute_filter_chip_rects();

        let cards_per_row = ((self.grid_width + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
            .floor()
            .max(1.0) as usize;
        let total = self.total_items();
        if total == 0 {
            return;
        }

        for idx in 0..total {
            let col = idx % cards_per_row;
            let row = idx / cards_per_row;
            let items_in_row = if row == total / cards_per_row {
                let remainder = total % cards_per_row;
                if remainder == 0 {
                    cards_per_row
                } else {
                    remainder
                }
            } else {
                cards_per_row
            };

            let row_pixel_w =
                items_in_row as f32 * CARD_WIDTH + (items_in_row.saturating_sub(1)) as f32 * CARD_GAP;
            let x_offset = ((self.grid_width - row_pixel_w) / 2.0).max(0.0);

            self.positions.push(Rect::new(
                self.rect.x + x_offset + col as f32 * (CARD_WIDTH + CARD_GAP),
                self.rect.y + self.toolbar_height() + row as f32 * (CARD_HEIGHT + CARD_GAP),
                CARD_WIDTH,
                CARD_HEIGHT,
            ));
        }

        let visible = self.visible_card_count();
        for (card_index, pos) in self
            .visible_indices
            .iter()
            .take(visible)
            .zip(self.positions.iter().take(visible))
        {
            self.cards[*card_index].layout(*pos);
        }
    }

    fn total_height(&self) -> f32 {
        let total = self.total_items();
        if total == 0 {
            return self.toolbar_height();
        }

        let cards_per_row = ((self.grid_width + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
            .floor()
            .max(1.0) as usize;
        let rows = total.div_ceil(cards_per_row);
        self.toolbar_height()
            + rows as f32 * CARD_HEIGHT
            + (rows.saturating_sub(1)) as f32 * CARD_GAP
    }

    fn add_card_rect(&self) -> Option<Rect> {
        self.show_add_card
            .then(|| self.positions.last().copied())
            .flatten()
    }

    fn empty_state_rect(&self) -> Rect {
        Rect::new(
            self.rect.x,
            self.rect.y + self.toolbar_height() + 18.0,
            self.rect.w,
            48.0,
        )
    }
}

impl Default for CardGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for CardGrid {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        self.grid_width = available.w;
        self.recompute_layout();
        Size {
            w: available.w,
            h: self.total_height(),
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        if self.show_filters {
            for (filter, rect) in &self.filter_chip_rects {
                let active = *filter == self.active_filter;
                let hovered = self.hovered_filter == Some(*filter);
                ctx.push_glass_quad(theme::glass_quad(
                    *rect,
                    if active {
                        [theme::ACCENT[0], theme::ACCENT[1], theme::ACCENT[2], 0.20]
                    } else if hovered {
                        [1.0, 1.0, 1.0, 0.08]
                    } else {
                        [1.0, 1.0, 1.0, 0.04]
                    },
                    if active {
                        [theme::ACCENT[0], theme::ACCENT[1], theme::ACCENT[2], 0.26]
                    } else {
                        [1.0, 1.0, 1.0, 0.10]
                    },
                    theme::CHIP_RADIUS,
                ));
                ctx.push_text_run(TextRun {
                    x: rect.x + 14.0,
                    y: rect.y + 8.0,
                    text: filter.label(self.cards.len()),
                    font_size: 11.0,
                    color: if active {
                        theme::TEXT_PRIMARY
                    } else {
                        theme::TEXT_SECONDARY
                    },
                    monospace: false,
                });
            }
        }

        for card_index in self.visible_indices.iter().take(self.visible_card_count()) {
            self.cards[*card_index].paint(ctx);
        }

        if self.visible_indices.is_empty() {
            let empty = self.empty_state_rect();
            ctx.push_text_run(TextRun {
                x: empty.x,
                y: empty.y,
                text: "No saved desktops match this filter.".to_string(),
                font_size: 12.0,
                color: theme::TEXT_MUTED,
                monospace: false,
            });
        }

        if let Some(add_rect) = self.add_card_rect() {
            ctx.push_glass_quad(theme::glass_quad(
                add_rect,
                [1.0, 1.0, 1.0, 0.05],
                [1.0, 1.0, 1.0, 0.16],
                theme::CARD_RADIUS,
            ));

            let plus = "+";
            ctx.push_text_run(TextRun {
                x: add_rect.x + (CARD_WIDTH - theme::text_width(plus, 32.0)) * 0.5,
                y: add_rect.y + 58.0,
                text: plus.to_string(),
                font_size: 32.0,
                color: theme::TEXT_SECONDARY,
                monospace: false,
            });

            let title = "Add Connection";
            ctx.push_text_run(TextRun {
                x: add_rect.x + (CARD_WIDTH - theme::text_width(title, 15.0)) * 0.5,
                y: add_rect.y + 114.0,
                text: title.to_string(),
                font_size: 15.0,
                color: theme::TEXT_PRIMARY,
                monospace: false,
            });

            let body = "Manual IP or quick setup";
            ctx.push_text_run(TextRun {
                x: add_rect.x + (CARD_WIDTH - theme::text_width(body, 12.0)) * 0.5,
                y: add_rect.y + 138.0,
                text: body.to_string(),
                font_size: 12.0,
                color: theme::TEXT_MUTED,
                monospace: false,
            });
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                self.hovered_filter = self
                    .filter_chip_rects
                    .iter()
                    .find_map(|(filter, rect)| rect.contains(*x, *y).then_some(*filter));
            }
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                if let Some((filter, _)) = self
                    .filter_chip_rects
                    .iter()
                    .find(|(_, rect)| rect.contains(*x, *y))
                {
                    if self.active_filter != *filter {
                        self.active_filter = *filter;
                        self.recompute_layout();
                    }
                    return EventResponse::Consumed;
                }
            }
            _ => {}
        }

        if let UiEvent::MouseDown {
            x,
            y,
            button: MouseButton::Left,
        } = event
            && let Some(add_rect) = self.add_card_rect()
            && add_rect.contains(*x, *y)
        {
            return EventResponse::Action(UiAction::AddServer);
        }

        let visible = self.visible_card_count();
        for (card_index, pos) in self
            .visible_indices
            .iter()
            .take(visible)
            .zip(self.positions.iter().take(visible))
        {
            let card = &mut self.cards[*card_index];
            match event {
                UiEvent::MouseMove { x, y } => {
                    let _ = card.handle_event(&UiEvent::MouseMove { x: *x, y: *y });
                }
                UiEvent::MouseDown { x, y, button } => {
                    if pos.contains(*x, *y) {
                        let resp = card.handle_event(&UiEvent::MouseDown {
                            x: *x,
                            y: *y,
                            button: button.clone(),
                        });
                        if !matches!(resp, EventResponse::Ignored) {
                            return resp;
                        }
                    }
                }
                UiEvent::MouseUp { x, y, button } => {
                    if pos.contains(*x, *y) {
                        let resp = card.handle_event(&UiEvent::MouseUp {
                            x: *x,
                            y: *y,
                            button: button.clone(),
                        });
                        if !matches!(resp, EventResponse::Ignored) {
                            return resp;
                        }
                    }
                }
                other => {
                    let resp = card.handle_event(other);
                    if !matches!(resp, EventResponse::Ignored) {
                        return resp;
                    }
                }
            }
        }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        for card in &mut self.cards {
            card.animate(dt_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_server(name: &str, address: &str, connected_offset: Option<u64>) -> SavedServer {
        let mut server = SavedServer::new(name, address);
        server.last_connected = connected_offset.map(|offset| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(offset)
        });
        server
    }

    fn make_servers() -> Vec<SavedServer> {
        vec![
            make_server("Recent", "10.0.0.1:4000", Some(60 * 30)),
            make_server("Dormant", "10.0.0.2:4000", Some(10 * 24 * 60 * 60)),
            make_server("New", "10.0.0.3:4000", None),
        ]
    }

    #[test]
    fn empty_grid_has_add_card() {
        let mut grid = CardGrid::new();
        grid.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let mut ctx = PaintContext::new();
        grid.paint(&mut ctx);

        assert!(ctx.glass_quads.len() >= 1);
    }

    #[test]
    fn grid_layout_wraps() {
        let mut grid = CardGrid::new();
        grid.set_servers(&make_servers());
        grid.layout(Rect::new(0.0, 0.0, 600.0, 800.0));

        assert_eq!(grid.positions.len(), 4);
        assert!(grid.positions[2].y > grid.positions[0].y);
    }

    #[test]
    fn grid_click_add() {
        let mut grid = CardGrid::new();
        grid.layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        let add_rect = grid.add_card_rect().expect("add card rect exists");

        let resp = grid.handle_event(&UiEvent::MouseDown {
            x: add_rect.x + CARD_WIDTH / 2.0,
            y: add_rect.y + CARD_HEIGHT / 2.0,
            button: MouseButton::Left,
        });

        assert!(matches!(resp, EventResponse::Action(UiAction::AddServer)));
    }

    #[test]
    fn grid_can_limit_cards_and_hide_add_card() {
        let mut grid = CardGrid::new();
        grid.set_servers(&make_servers());
        grid.set_visible_limit(Some(2));
        grid.set_show_add_card(false);
        grid.layout(Rect::new(0.0, 0.0, 900.0, 800.0));

        assert_eq!(grid.positions.len(), 2);
        assert!(grid.add_card_rect().is_none());
    }

    #[test]
    fn clicking_filter_updates_active_filter() {
        let mut grid = CardGrid::new();
        grid.set_servers(&make_servers());
        grid.set_show_filters(true);
        grid.layout(Rect::new(0.0, 0.0, 900.0, 800.0));
        let recent_rect = grid.filter_chip_rects[1].1;

        let resp = grid.handle_event(&UiEvent::MouseDown {
            x: recent_rect.x + 4.0,
            y: recent_rect.y + 4.0,
            button: MouseButton::Left,
        });

        assert!(matches!(resp, EventResponse::Consumed));
        assert_eq!(grid.active_filter, CardFilter::Recent);
        assert_eq!(grid.visible_card_count(), 1);
    }
}
