// SPDX-License-Identifier: AGPL-3.0-or-later
//! Material Symbols icon rendered through the text pipeline.

use super::{PaintContext, TextRun};
use crate::ui::theme;

// ---------------------------------------------------------------------------
// Icon codepoint constants (Material Symbols Outlined)
// ---------------------------------------------------------------------------

pub const ICON_HOME: char = '\u{E88A}';
pub const ICON_DEVICES: char = '\u{E1B1}';
pub const ICON_TUNE: char = '\u{E429}';
pub const ICON_SETTINGS: char = '\u{E8B8}';
pub const ICON_SEARCH: char = '\u{E8B6}';
pub const ICON_ADD: char = '\u{E145}';
pub const ICON_MENU: char = '\u{E5D2}';
pub const ICON_SYNC: char = '\u{E627}';
pub const ICON_HEART: char = '\u{E87D}';
pub const ICON_EDIT: char = '\u{E3C9}';
pub const ICON_FILTER: char = '\u{EF4F}';
pub const ICON_SORT: char = '\u{E164}';
pub const ICON_GAMEPAD: char = '\u{E30F}';
pub const ICON_CODE: char = '\u{E86F}';
pub const ICON_BALANCE: char = '\u{E8F1}';
pub const ICON_SPEED: char = '\u{E9E4}';
pub const ICON_CHEVRON_DOWN: char = '\u{E5CF}';
pub const ICON_DIAL: char = '\u{E9E1}';
pub const ICON_MORE_VERT: char = '\u{E5D4}';
pub const ICON_CLOCK: char = '\u{E8B5}';
pub const ICON_HEADPHONES: char = '\u{F01D}';
pub const ICON_MIC: char = '\u{E029}';
pub const ICON_KEYBOARD: char = '\u{E312}';
pub const ICON_MONITOR: char = '\u{E30B}';
pub const ICON_BOLT: char = '\u{EA0B}';
pub const ICON_SHIELD: char = '\u{E8E8}';
pub const ICON_SPEAKER: char = '\u{E32D}';
pub const ICON_STREAMING: char = '\u{E1B2}';

// ---------------------------------------------------------------------------
// Icon widget
// ---------------------------------------------------------------------------

pub struct Icon {
    codepoint: char,
    size: f32,
    color: [f32; 4],
    x: f32,
    y: f32,
}

impl Icon {
    pub fn new(codepoint: char) -> Self {
        Self {
            codepoint,
            size: 20.0,
            color: theme::LT_TEXT_PRIMARY,
            x: 0.0,
            y: 0.0,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn paint(&self, ctx: &mut PaintContext) {
        ctx.text_runs.push(TextRun {
            x: self.x,
            y: self.y,
            text: self.codepoint.to_string(),
            font_size: self.size,
            color: self.color,
            icon: true,
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_emits_text_run() {
        let mut ctx = PaintContext::new();
        Icon::new(ICON_HOME)
            .with_size(24.0)
            .with_color([1.0, 0.0, 0.0, 1.0])
            .at(10.0, 20.0)
            .paint(&mut ctx);

        assert_eq!(ctx.text_runs.len(), 1);
        let run = &ctx.text_runs[0];
        assert_eq!(run.x, 10.0);
        assert_eq!(run.y, 20.0);
        assert_eq!(run.font_size, 24.0);
        assert!(run.icon);
        assert_eq!(run.text, ICON_HOME.to_string());
    }
}
