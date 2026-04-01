// SPDX-License-Identifier: AGPL-3.0-or-later
//! Responsive flow grid of server cards plus an "+ Add Server" card.

use super::server_card::ServerCard;
use crate::config::servers::SavedServer;
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CARD_WIDTH: f32 = ServerCard::WIDTH;
const CARD_HEIGHT: f32 = ServerCard::HEIGHT;
const CARD_GAP: f32 = 20.0;

// ---------------------------------------------------------------------------
// CardGrid
// ---------------------------------------------------------------------------

pub struct CardGrid {
    cards: Vec<ServerCard>,
    positions: Vec<Rect>,
    grid_width: f32,
    rect: Rect,
}

impl CardGrid {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            positions: Vec::new(),
            grid_width: 800.0,
            rect: Rect::new(0.0, 0.0, 800.0, 600.0),
        }
    }

    pub fn set_servers(&mut self, servers: &[SavedServer]) {
        self.cards = servers.iter().map(ServerCard::from_saved).collect();
        self.positions.clear(); // invalidate
    }

    pub fn cards(&self) -> &[ServerCard] {
        &self.cards
    }

    // -----------------------------------------------------------------------
    // Private
    // -----------------------------------------------------------------------

    fn recompute_layout(&mut self) {
        self.positions.clear();

        // How many cards fit per row
        let cards_per_row = ((self.grid_width + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
            .floor()
            .max(1.0) as usize;

        // Total items = cards + 1 add-card
        let total = self.cards.len() + 1;

        for idx in 0..total {
            let col = idx % cards_per_row;
            let row = idx / cards_per_row;

            // Number of cards in this row (to center it)
            let items_in_row = if row == total / cards_per_row {
                // last (possibly partial) row
                let remainder = total % cards_per_row;
                if remainder == 0 {
                    cards_per_row
                } else {
                    remainder
                }
            } else {
                cards_per_row
            };

            // Row width and x offset to center
            let row_pixel_w = items_in_row as f32 * CARD_WIDTH
                + (items_in_row.saturating_sub(1)) as f32 * CARD_GAP;
            let x_offset = ((self.grid_width - row_pixel_w) / 2.0).max(0.0);

            let x = self.rect.x + x_offset + col as f32 * (CARD_WIDTH + CARD_GAP);
            let y = self.rect.y + row as f32 * (CARD_HEIGHT + CARD_GAP);

            self.positions
                .push(Rect::new(x, y, CARD_WIDTH, CARD_HEIGHT));
        }
    }

    /// Total height occupied by all rows.
    fn total_height(&self) -> f32 {
        let total = self.cards.len() + 1;
        let cards_per_row = ((self.grid_width + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
            .floor()
            .max(1.0) as usize;
        let rows = total.div_ceil(cards_per_row);
        rows as f32 * CARD_HEIGHT + (rows.saturating_sub(1)) as f32 * CARD_GAP
    }

    /// Rect for the "+Add" card (last position).
    fn add_card_rect(&self) -> Option<Rect> {
        self.positions.last().copied()
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

        let h = self.total_height();
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Paint each server card at its cached position
        for (card, &pos) in self.cards.iter().zip(self.positions.iter()) {
            // Cards are painted via their own paint method; we forward the
            // positioned rect by temporarily reading their stored rect.
            // Since ServerCard::paint reads self.rect set during layout, and
            // layout was called per-card via handle_event routing, we need to
            // paint at the grid position. We call paint after the card's rect
            // was stored during layout. Here we do a local context trick:
            // push offset glass quad matching card position, then call paint.
            // Actually, the cleaner approach: we set a translated sub-context.
            // Since PaintContext just collects commands, we call paint directly —
            // the card's internal rect was set during layout; our grid layout
            // stores positions separately. We need to align them.
            //
            // The simplest correct approach: draw the card commands at the
            // grid-assigned position by directly pushing the draw commands
            // instead of calling card.paint(). But that duplicates logic.
            //
            // Instead, we use a sub-context and translate draw commands.
            let mut sub = PaintContext::new();
            card.paint(&mut sub);

            // Compute translation from card's own rect origin to grid position.
            // card.rect is set during layout(available) where available = grid pos.
            // Since we don't call layout on cards here (only set_servers does not
            // call layout), we need to reconcile. The paint approach below
            // renders at `pos` by translating all sub-context items.
            let card_rect = {
                // Read the card's painted origin from the first glass_quad if available
                if let Some(gq) = sub.glass_quads.first() {
                    gq.rect
                } else {
                    Rect::new(pos.x, pos.y, CARD_WIDTH, CARD_HEIGHT)
                }
            };
            let dx = pos.x - card_rect.x;
            let dy = pos.y - card_rect.y;

            for mut gq in sub.glass_quads {
                gq.rect.x += dx;
                gq.rect.y += dy;
                gq.blur_rect.x += dx;
                gq.blur_rect.y += dy;
                ctx.push_glass_quad(gq);
            }
            for mut gr in sub.glow_rects {
                gr.rect.x += dx;
                gr.rect.y += dy;
                ctx.push_glow_rect(gr);
            }
            for mut tr in sub.text_runs {
                tr.x += dx;
                tr.y += dy;
                ctx.push_text_run(tr);
            }
        }

        // Paint the "+ Add Server" card
        if let Some(add_rect) = self.add_card_rect() {
            ctx.push_glass_quad(theme::card_surface(add_rect));

            let plus = "+";
            ctx.push_text_run(TextRun {
                x: add_rect.x + (CARD_WIDTH - theme::text_width(plus, 32.0)) * 0.5,
                y: add_rect.y + 50.0,
                text: plus.to_string(),
                font_size: 32.0,
                color: theme::TEXT_SECONDARY,
                monospace: false,
            });

            let title = "Add server";
            ctx.push_text_run(TextRun {
                x: add_rect.x + (CARD_WIDTH - theme::text_width(title, 14.0)) * 0.5,
                y: add_rect.y + 102.0,
                text: title.to_string(),
                font_size: 14.0,
                color: theme::TEXT_PRIMARY,
                monospace: false,
            });

            let body = "Save a new desktop";
            ctx.push_text_run(TextRun {
                x: add_rect.x + (CARD_WIDTH - theme::text_width(body, 12.0)) * 0.5,
                y: add_rect.y + 124.0,
                text: body.to_string(),
                font_size: 12.0,
                color: theme::TEXT_MUTED,
                monospace: false,
            });
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Check the Add card first for click events
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

        // Route to individual cards based on cached position
        let n = self.cards.len();
        for i in 0..n {
            let pos = self.positions.get(i).copied();
            if let Some(pos) = pos {
                // Translate the event to the card's local space
                let translated = match event {
                    UiEvent::MouseMove { x, y } => {
                        // Always propagate MouseMove to all cards for hover tracking
                        let resp = self.cards[i].handle_event(&UiEvent::MouseMove { x: *x, y: *y });
                        // We've already handled it inline, continue loop
                        let _ = resp;
                        continue;
                    }
                    UiEvent::MouseDown { x, y, button } => {
                        if pos.contains(*x, *y) {
                            UiEvent::MouseDown {
                                x: *x,
                                y: *y,
                                button: button.clone(),
                            }
                        } else {
                            continue;
                        }
                    }
                    UiEvent::MouseUp { x, y, button } => {
                        if pos.contains(*x, *y) {
                            UiEvent::MouseUp {
                                x: *x,
                                y: *y,
                                button: button.clone(),
                            }
                        } else {
                            continue;
                        }
                    }
                    other => {
                        let resp = self.cards[i].handle_event(other);
                        if !matches!(resp, EventResponse::Ignored) {
                            return resp;
                        }
                        continue;
                    }
                };

                let resp = self.cards[i].handle_event(&translated);
                if !matches!(resp, EventResponse::Ignored) {
                    return resp;
                }
            }
        }

        // MouseMove for all cards (handled inline above via continue)
        if let UiEvent::MouseMove { .. } = event {
            // Already propagated above
        }

        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        for card in &mut self.cards {
            card.animate(dt_ms);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::servers::SavedServer;

    fn make_servers(n: usize) -> Vec<SavedServer> {
        (0..n)
            .map(|i| SavedServer::new(format!("Server {}", i), format!("10.0.0.{}:4000", i)))
            .collect()
    }

    #[test]
    fn empty_grid_has_add_card() {
        let mut grid = CardGrid::new();
        grid.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let mut ctx = PaintContext::new();
        grid.paint(&mut ctx);

        assert!(
            ctx.glass_quads.len() >= 1,
            "expected at least 1 glass quad (the add card)"
        );
    }

    #[test]
    fn grid_layout_wraps() {
        let mut grid = CardGrid::new();
        let servers = make_servers(5);
        grid.set_servers(&servers);
        grid.layout(Rect::new(0.0, 0.0, 600.0, 800.0));

        // cards_per_row = floor((600 + 16) / (240 + 16)) = floor(616 / 256) = 2
        // total items = 5 cards + 1 add = 6
        // rows = ceil(6 / 2) = 3
        assert_eq!(
            grid.positions.len(),
            6,
            "expected 6 positions (5 cards + add)"
        );

        // Verify row wrapping: item at index 2 should be in row 1
        let row_1_item = grid.positions[2];
        let row_0_item = grid.positions[0];
        assert!(
            row_1_item.y > row_0_item.y,
            "item 2 should be in a lower row than item 0"
        );

        // Item at index 4 should be in row 2
        let row_2_item = grid.positions[4];
        assert!(
            row_2_item.y > row_1_item.y,
            "item 4 should be in a lower row than item 2"
        );
    }

    #[test]
    fn grid_click_add() {
        let mut grid = CardGrid::new();
        grid.layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        // The add card is the last (and only) position
        let add_rect = grid.add_card_rect().expect("add card rect exists");

        // Click in the center of the add card
        let resp = grid.handle_event(&UiEvent::MouseDown {
            x: add_rect.x + CARD_WIDTH / 2.0,
            y: add_rect.y + CARD_HEIGHT / 2.0,
            button: MouseButton::Left,
        });

        assert!(
            matches!(resp, EventResponse::Action(UiAction::AddServer)),
            "expected AddServer action, got {:?}",
            resp
        );
    }
}
