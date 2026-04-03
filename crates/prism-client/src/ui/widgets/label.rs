// SPDX-License-Identifier: AGPL-3.0-or-later
//! Static text label widget.

use super::{EventResponse, PaintContext, Rect, Size, TextRun, UiEvent, Widget};
use crate::ui::theme;

pub struct Label {
    text: String,
    font_size: f32,
    color: [f32; 4],
    monospace: bool,
    rect: Rect,
}

impl Label {
    pub fn new(text: &str, font_size: f32) -> Self {
        Self {
            text: text.to_owned(),
            font_size,
            color: theme::TEXT_SECONDARY,
            monospace: false,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    pub fn with_monospace(mut self, mono: bool) -> Self {
        self.monospace = mono;
        self
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_owned();
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        self.color = color;
    }
}

impl Widget for Label {
    fn layout(&mut self, available: Rect) -> Size {
        let natural_w = self.text.len() as f32 * self.font_size * 0.6;
        let w = natural_w.min(available.w);
        let h = self.font_size * 1.4;
        self.rect = Rect::new(available.x, available.y, w, h);
        Size { w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        ctx.push_text_run(TextRun {
            x: self.rect.x,
            y: self.rect.y,
            text: self.text.clone(),
            font_size: self.font_size,
            color: self.color,
            monospace: self.monospace,
            ..Default::default()
        });
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 400.0, 100.0)
    }

    #[test]
    fn label_reports_size() {
        let mut label = Label::new("Hello", 14.0);
        let size = label.layout(available());
        // w = 5 chars * 14.0 * 0.6 = 42.0, h = 14.0 * 1.4 = 19.6
        assert!((size.w - 42.0).abs() < 0.01, "w was {}", size.w);
        assert!((size.h - 19.6).abs() < 0.01, "h was {}", size.h);
    }

    #[test]
    fn label_emits_text_run() {
        let mut label = Label::new("Hello", 14.0);
        label.layout(available());
        let mut ctx = PaintContext::new();
        label.paint(&mut ctx);
        assert_eq!(ctx.text_runs.len(), 1);
        assert_eq!(ctx.text_runs[0].text, "Hello");
        assert!((ctx.text_runs[0].x - 0.0).abs() < 0.01);
        assert!((ctx.text_runs[0].y - 0.0).abs() < 0.01);
    }

    #[test]
    fn label_update_text() {
        let mut label = Label::new("old", 12.0);
        label.set_text("new");
        label.layout(available());
        let mut ctx = PaintContext::new();
        label.paint(&mut ctx);
        assert_eq!(ctx.text_runs[0].text, "new");
    }

    #[test]
    fn label_monospace_flag() {
        let mut label = Label::new("code", 12.0).with_monospace(true);
        label.layout(available());
        let mut ctx = PaintContext::new();
        label.paint(&mut ctx);
        assert!(ctx.text_runs[0].monospace);
    }

    #[test]
    fn label_ignores_events() {
        let mut label = Label::new("test", 12.0);
        let resp = label.handle_event(&UiEvent::MouseMove { x: 0.0, y: 0.0 });
        assert!(matches!(resp, EventResponse::Ignored));
    }
}
