// SPDX-License-Identifier: AGPL-3.0-or-later
//! Responsive saved-connections grid with filter chips.

use super::server_card::{CardFilter, ServerCard};
use crate::config::servers::SavedServer;
use crate::ui::theme;
use crate::ui::widgets::icon::{Icon, ICON_ADD, ICON_FILTER, ICON_SORT};
use crate::ui::widgets::{
    EventResponse, GlassQuad, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent,
    Widget,
};

const CARD_WIDTH: f32 = ServerCard::WIDTH;
const CARD_HEIGHT: f32 = ServerCard::HEIGHT;
const CARD_GAP: f32 = 20.0;
const FILTER_H: f32 = 32.0;
const FILTER_GAP: f32 = 10.0;
const TOOLBAR_H: f32 = 52.0;
const SUBTITLE_H: f32 = 28.0;
const FAB_SIZE: f32 = 56.0;
const FAB_PAD: f32 = 24.0;

const PAGE_SUBTITLE: &str =
    "Browse saved desktops, reconnect quickly, and keep your machines organized.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridMode {
    Grid,
    Rows,
}

pub struct CardGrid {
    cards: Vec<ServerCard>,
    visible_indices: Vec<usize>,
    positions: Vec<Rect>,
    filter_chip_rects: Vec<(CardFilter, Rect)>,
    filter_btn_rect: Rect,
    sort_btn_rect: Rect,
    active_filter: CardFilter,
    hovered_filter: Option<CardFilter>,
    grid_width: f32,
    rect: Rect,
    visible_limit: Option<usize>,
    show_add_card: bool,
    show_filters: bool,
    layout_mode: GridMode,
    scroll_y: f32,
    max_scroll: f32,
}

impl CardGrid {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            visible_indices: Vec::new(),
            positions: Vec::new(),
            filter_chip_rects: Vec::new(),
            filter_btn_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            sort_btn_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            active_filter: CardFilter::All,
            hovered_filter: None,
            grid_width: 800.0,
            rect: Rect::new(0.0, 0.0, 800.0, 600.0),
            visible_limit: None,
            show_add_card: true,
            show_filters: false,
            layout_mode: GridMode::Grid,
            scroll_y: 0.0,
            max_scroll: 0.0,
        }
    }

    pub fn set_servers(&mut self, servers: &[SavedServer]) {
        let mut ordered = servers.to_vec();
        ordered.sort_by(|a, b| {
            b.last_connected
                .unwrap_or(b.created_at)
                .cmp(&a.last_connected.unwrap_or(a.created_at))
        });
        let card_mode = match self.layout_mode {
            GridMode::Grid => super::server_card::CardLayoutMode::Card,
            GridMode::Rows => super::server_card::CardLayoutMode::Row,
        };
        self.cards = ordered
            .iter()
            .enumerate()
            .map(|(i, s)| {
                ServerCard::from_saved(s)
                    .with_layout_mode(card_mode)
                    .with_index(i + 1)
            })
            .collect();
        self.positions.clear();
    }

    pub fn set_layout_mode(&mut self, mode: GridMode) {
        if self.layout_mode != mode {
            self.layout_mode = mode;
            let card_mode = match mode {
                GridMode::Grid => super::server_card::CardLayoutMode::Card,
                GridMode::Rows => super::server_card::CardLayoutMode::Row,
            };
            for card in &mut self.cards {
                card.set_layout_mode(card_mode);
            }
            self.positions.clear();
        }
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

    pub fn reset_filter(&mut self) {
        self.active_filter = CardFilter::All;
        self.recompute_layout();
    }

    fn toolbar_height(&self) -> f32 {
        if self.show_filters {
            SUBTITLE_H + TOOLBAR_H
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
            .filter_map(|(index, card)| card.matches_filter(&self.active_filter).then_some(index))
            .collect();
    }

    fn recompute_filter_chip_rects(&mut self) {
        self.filter_chip_rects.clear();
        if !self.show_filters {
            return;
        }

        let mut x = self.rect.x;
        let y = self.rect.y + SUBTITLE_H;
        for filter in [
            CardFilter::All,
            CardFilter::Recent,
            CardFilter::Dormant,
            CardFilter::New,
        ] {
            let label = filter.label();
            let w = theme::text_width(&label, 11.0) + 28.0;
            let rect = Rect::new(x, y, w, FILTER_H);
            self.filter_chip_rects.push((filter, rect));
            x += w + FILTER_GAP;
        }

        // Append tag-based pills from visible cards
        let mut seen_tags = std::collections::HashSet::new();
        for card in &self.cards {
            for tag in card.tags() {
                if seen_tags.insert(tag.clone()) {
                    let w = theme::text_width(&tag, 11.0) + 28.0;
                    let rect = Rect::new(x, y, w, FILTER_H);
                    self.filter_chip_rects.push((CardFilter::Tag(tag.to_string()), rect));
                    x += w + FILTER_GAP;
                }
            }
        }

        // Right-aligned Filter and Sort buttons
        let sort_label = "Last Connected";
        let filter_label = "Filter";
        let sort_w = theme::text_width(sort_label, 11.0) + 36.0;
        let filter_w = theme::text_width(filter_label, 11.0) + 36.0;
        let right_edge = self.rect.x + self.grid_width;
        self.sort_btn_rect = Rect::new(right_edge - sort_w, y, sort_w, FILTER_H);
        self.filter_btn_rect =
            Rect::new(right_edge - sort_w - 8.0 - filter_w, y, filter_w, FILTER_H);
    }

    fn recompute_layout(&mut self) {
        self.positions.clear();
        self.recompute_visible_indices();
        self.recompute_filter_chip_rects();

        let total = self.total_items();
        if total == 0 {
            self.max_scroll = 0.0;
            return;
        }

        match self.layout_mode {
            GridMode::Grid => {
                let cards_per_row = ((self.grid_width + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
                    .floor()
                    .max(1.0) as usize;

                for idx in 0..total {
                    let col = idx % cards_per_row;
                    let row = idx / cards_per_row;

                    self.positions.push(Rect::new(
                        self.rect.x + col as f32 * (CARD_WIDTH + CARD_GAP),
                        self.rect.y + self.toolbar_height() + row as f32 * (CARD_HEIGHT + CARD_GAP),
                        CARD_WIDTH,
                        CARD_HEIGHT,
                    ));
                }
            }
            GridMode::Rows => {
                let row_height = 64.0;
                let row_gap = 12.0;
                for idx in 0..total {
                    self.positions.push(Rect::new(
                        self.rect.x,
                        self.rect.y + self.toolbar_height() + idx as f32 * (row_height + row_gap),
                        self.grid_width,
                        row_height,
                    ));
                }
            }
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

        // Compute max scroll
        let content_h = self.total_height();
        let view_h = self.rect.h;
        self.max_scroll = (content_h - view_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll);
    }

    fn total_height(&self) -> f32 {
        let total = self.total_items();
        if total == 0 {
            return self.toolbar_height();
        }

        match self.layout_mode {
            GridMode::Grid => {
                let cards_per_row = ((self.grid_width + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
                    .floor()
                    .max(1.0) as usize;
                let rows = total.div_ceil(cards_per_row);
                self.toolbar_height()
                    + rows as f32 * CARD_HEIGHT
                    + (rows.saturating_sub(1)) as f32 * CARD_GAP
            }
            GridMode::Rows => {
                let row_height = 64.0;
                let row_gap = 12.0;
                self.toolbar_height()
                    + total as f32 * row_height
                    + (total.saturating_sub(1)) as f32 * row_gap
            }
        }
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
        // Page subtitle (above filter bar)
        if self.show_filters {
            ctx.push_text_run(TextRun {
                x: self.rect.x,
                y: self.rect.y + 4.0,
                text: PAGE_SUBTITLE.to_string(),
                font_size: theme::FONT_BODY,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });
        }

        // Filter chips
        if self.show_filters {
            for (filter, rect) in &self.filter_chip_rects {
                let active = filter == &self.active_filter;
                let hovered = self.hovered_filter.as_ref() == Some(filter);

                let pill_radius = rect.h / 2.0;
                if active {
                    // Solid PRIMARY_BLUE pill
                    ctx.push_glass_quad(GlassQuad {
                        rect: *rect,
                        tint: theme::PRIMARY_BLUE,
                        corner_radius: pill_radius,
                        ..Default::default()
                    });
                    ctx.push_text_run(TextRun {
                        x: rect.x + 14.0,
                        y: rect.y + 10.0,
                        text: filter.label(),
                        font_size: 11.0,
                        color: [1.0, 1.0, 1.0, 1.0],
                        ..Default::default()
                    });
                } else {
                    // Outlined chip
                    ctx.push_glass_quad(theme::glass_quad(
                        *rect,
                        [1.0, 1.0, 1.0, 0.50],
                        [1.0, 1.0, 1.0, 0.60],
                        pill_radius,
                    ));
                    if hovered {
                        ctx.push_glass_quad(theme::glass_quad(
                            *rect,
                            [0.0, 0.0, 0.0, 0.04],
                            [0.0, 0.0, 0.0, 0.0],
                            pill_radius,
                        ));
                    }
                    ctx.push_text_run(TextRun {
                        x: rect.x + 14.0,
                        y: rect.y + 10.0,
                        text: filter.label(),
                        font_size: 11.0,
                        color: theme::LT_TEXT_SECONDARY,
                        ..Default::default()
                    });
                }
            }

            // Right-aligned Filter button
            let fb = self.filter_btn_rect;
            ctx.push_glass_quad(theme::launcher_control_surface(fb, false));
            Icon::new(ICON_FILTER)
                .with_size(14.0)
                .with_color(theme::LT_TEXT_SECONDARY)
                .at(fb.x + 8.0, fb.y + 9.0)
                .paint(ctx);
            ctx.push_text_run(TextRun {
                x: fb.x + 26.0,
                y: fb.y + 10.0,
                text: "Filter".to_string(),
                font_size: 11.0,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });

            // Right-aligned Sort button
            let sb = self.sort_btn_rect;
            ctx.push_glass_quad(theme::launcher_control_surface(sb, false));
            Icon::new(ICON_SORT)
                .with_size(14.0)
                .with_color(theme::LT_TEXT_SECONDARY)
                .at(sb.x + 8.0, sb.y + 9.0)
                .paint(ctx);
            ctx.push_text_run(TextRun {
                x: sb.x + 26.0,
                y: sb.y + 10.0,
                text: "Last Connected".to_string(),
                font_size: 11.0,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });
        }

        // Cards (with scroll offset applied)
        let scroll = self.scroll_y;
        for card_index in self.visible_indices.iter().take(self.visible_card_count()) {
            let card = &self.cards[*card_index];
            // Offset card painting by scroll
            if scroll > 0.0 {
                let mut offset_ctx = PaintContext::new();
                card.paint(&mut offset_ctx);
                for mut quad in offset_ctx.glass_quads {
                    quad.rect.y -= scroll;
                    quad.blur_rect.y -= scroll;
                    ctx.push_glass_quad(quad);
                }
                for mut run in offset_ctx.text_runs {
                    run.y -= scroll;
                    ctx.push_text_run(run);
                }
            } else {
                card.paint(ctx);
            }
        }

        if self.visible_indices.is_empty() {
            let empty = self.empty_state_rect();
            ctx.push_text_run(TextRun {
                x: empty.x,
                y: empty.y - scroll,
                text: "No saved desktops match this filter.".to_string(),
                font_size: 12.0,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
        }

        // Add New Connection card (last item, dashed border)
        if let Some(add_rect) = self.add_card_rect() {
            let ar = Rect::new(add_rect.x, add_rect.y - scroll, add_rect.w, add_rect.h);

            // Dashed-style border (approximate with reduced-opacity white border)
            ctx.push_glass_quad(theme::glass_quad(
                ar,
                [1.0, 1.0, 1.0, 0.30],
                [1.0, 1.0, 1.0, 0.60],
                theme::CARD_RADIUS,
            ));

            // White filled circle with ICON_ADD
            let circle_r = 24.0;
            let cx = ar.x + ar.w * 0.5;
            let cy = ar.y + ar.h * 0.38;
            ctx.push_glass_quad(GlassQuad {
                rect: Rect::new(cx - circle_r, cy - circle_r, circle_r * 2.0, circle_r * 2.0),
                tint: [1.0, 1.0, 1.0, 0.90],
                corner_radius: circle_r,
                ..Default::default()
            });
            Icon::new(ICON_ADD)
                .with_size(24.0)
                .with_color(theme::PRIMARY_BLUE)
                .at(cx - 12.0, cy - 12.0)
                .paint(ctx);

            let title = "Add New Connection";
            ctx.push_text_run(TextRun {
                x: ar.x + (ar.w - theme::text_width(title, 14.0)) * 0.5,
                y: cy + 36.0,
                text: title.to_string(),
                font_size: 14.0,
                color: theme::LT_TEXT_SECONDARY,
                ..Default::default()
            });

            let body = "Manual IP or Network Discovery";
            ctx.push_text_run(TextRun {
                x: ar.x + (ar.w - theme::text_width(body, 11.0)) * 0.5,
                y: cy + 56.0,
                text: body.to_string(),
                font_size: 11.0,
                color: theme::LT_TEXT_MUTED,
                ..Default::default()
            });
        }

        // FAB (Floating Action Button) — only when showing filters (Connections tab)
        if self.show_filters {
            let fab_rect = Rect::new(
                self.rect.x + self.rect.w - FAB_SIZE - FAB_PAD,
                self.rect.y + self.rect.h - FAB_SIZE - FAB_PAD,
                FAB_SIZE,
                FAB_SIZE,
            );
            ctx.push_glass_quad(GlassQuad {
                rect: fab_rect,
                tint: theme::PRIMARY_BLUE,
                corner_radius: FAB_SIZE / 2.0,
                ..Default::default()
            });
            Icon::new(ICON_ADD)
                .with_size(24.0)
                .with_color([1.0, 1.0, 1.0, 1.0])
                .at(fab_rect.x + 16.0, fab_rect.y + 16.0)
                .paint(ctx);
        }

        // Scroll indicator
        if self.max_scroll > 0.0 {
            let track_h = self.rect.h - self.toolbar_height();
            let thumb_h = (track_h * track_h / (track_h + self.max_scroll)).max(24.0);
            let thumb_y = self.rect.y + self.toolbar_height()
                + (track_h - thumb_h) * (self.scroll_y / self.max_scroll);
            let track_x = self.rect.x + self.rect.w - 4.0;
            ctx.push_glass_quad(GlassQuad {
                rect: Rect::new(track_x, thumb_y, 4.0, thumb_h),
                tint: [0.0, 0.0, 0.0, 0.15],
                corner_radius: 2.0,
                ..Default::default()
            });
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Scroll handling
        if let UiEvent::Scroll { dy, .. } = event {
            if self.max_scroll > 0.0 {
                self.scroll_y = (self.scroll_y - dy).clamp(0.0, self.max_scroll);
                return EventResponse::Consumed;
            }
        }

        // FAB click
        if self.show_filters {
            if let UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } = event
            {
                let fab_rect = Rect::new(
                    self.rect.x + self.rect.w - FAB_SIZE - FAB_PAD,
                    self.rect.y + self.rect.h - FAB_SIZE - FAB_PAD,
                    FAB_SIZE,
                    FAB_SIZE,
                );
                if fab_rect.contains(*x, *y) {
                    return EventResponse::Action(UiAction::AddServer);
                }
            }
        }

        match event {
            UiEvent::MouseMove { x, y } => {
                self.hovered_filter = self
                    .filter_chip_rects
                    .iter()
                    .find_map(|(filter, rect)| rect.contains(*x, *y).then_some(filter.clone()));
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
                        self.active_filter = filter.clone();
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

        assert!(!ctx.glass_quads.is_empty());
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
