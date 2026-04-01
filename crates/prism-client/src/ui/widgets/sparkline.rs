// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sparkline chart widget for time-series metrics.

use super::{EventResponse, GlowRect, PaintContext, Rect, Size, UiEvent, Widget};

pub struct Sparkline {
    values: Vec<f32>,
    capacity: usize,
    head: usize,
    count: usize,
    min_val: f32,
    max_val: f32,
    rect: Rect,
    accent_color: [f32; 4],
}

impl Sparkline {
    pub fn new(capacity: usize) -> Self {
        Self {
            values: vec![0.0; capacity],
            capacity,
            head: 0,
            count: 0,
            min_val: 0.0,
            max_val: 1.0,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            accent_color: [0.55, 0.36, 0.96, 0.8],
        }
    }

    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.min_val = min;
        self.max_val = max;
        self
    }

    pub fn push(&mut self, value: f32) {
        self.values[self.head] = value;
        self.head = (self.head + 1) % self.capacity;
        if self.count < self.capacity {
            self.count += 1;
        }
    }

    pub fn set_from_slice(&mut self, data: &[f32]) {
        for &v in data {
            self.push(v);
        }
    }

    /// Get value at logical index 0=oldest.
    pub fn get(&self, logical_index: usize) -> f32 {
        let start = if self.count < self.capacity {
            0
        } else {
            self.head
        };
        self.values[(start + logical_index) % self.capacity]
    }

    fn value_to_y(&self, value: f32) -> f32 {
        let range = self.max_val - self.min_val;
        let norm = if range.abs() < f32::EPSILON {
            0.0
        } else {
            ((value - self.min_val) / range).clamp(0.0, 1.0)
        };
        // Inverted: high value = low y
        self.rect.y + self.rect.h - norm * self.rect.h
    }
}

impl Widget for Sparkline {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 24.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        if self.count < 2 {
            return;
        }

        let step = self.rect.w / (self.count - 1) as f32;

        for i in 0..self.count {
            let value = self.get(i);
            let y = self.value_to_y(value);
            let x = self.rect.x + i as f32 * step;

            // Normalized index: 0=oldest, 1=newest
            let normalized = i as f32 / (self.count - 1) as f32;
            let alpha = 0.4 + 0.4 * normalized;
            let spread = if i == self.count - 1 { 3.0 } else { 1.0 };
            let point_alpha = if i == self.count - 1 { 1.0 } else { alpha };

            ctx.push_glow_rect(GlowRect {
                rect: Rect::new(x, y - 1.0, step, 2.0),
                color: [
                    self.accent_color[0],
                    self.accent_color[1],
                    self.accent_color[2],
                    point_alpha,
                ],
                spread,
                intensity: point_alpha,
            });
        }
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

    #[test]
    fn push_and_retrieve() {
        let mut s = Sparkline::new(10);
        s.push(1.0);
        s.push(9.0);
        s.push(2.0);
        assert!((s.get(0) - 1.0).abs() < 0.001);
        assert!((s.get(1) - 9.0).abs() < 0.001);
        assert!((s.get(2) - 2.0).abs() < 0.001);
    }

    #[test]
    fn ring_buffer_wraps() {
        let mut s = Sparkline::new(3);
        s.push(10.0);
        s.push(20.0);
        s.push(30.0);
        // Buffer full: [10, 20, 30], head=0, count=3
        s.push(40.0);
        // Now oldest is 20, buffer is [40, 20, 30], head=1
        // get(0)=oldest=20, get(1)=30, get(2)=40
        assert!(
            (s.get(0) - 20.0).abs() < 0.001,
            "oldest should be 20, got {}",
            s.get(0)
        );
        assert!(
            (s.get(1) - 30.0).abs() < 0.001,
            "middle should be 30, got {}",
            s.get(1)
        );
        assert!(
            (s.get(2) - 40.0).abs() < 0.001,
            "newest should be 40, got {}",
            s.get(2)
        );
    }

    #[test]
    fn renders_glow_rects_for_points() {
        let mut s = Sparkline::new(10).with_range(0.0, 100.0);
        s.layout(Rect::new(0.0, 0.0, 200.0, 24.0));
        for i in 0..5 {
            s.push(i as f32 * 20.0);
        }
        let mut ctx = PaintContext::new();
        s.paint(&mut ctx);
        assert_eq!(
            ctx.glow_rects.len(),
            5,
            "expected 5 glow rects, got {}",
            ctx.glow_rects.len()
        );
    }
}
