// SPDX-License-Identifier: AGPL-3.0-or-later
//! Horizontal value slider widget.

use super::{
    EventResponse, GlassQuad, GlowRect, MouseButton, PaintContext, Rect, Size, TextRun, UiEvent,
    Widget,
};

pub struct Slider {
    label: String,
    value: f32,
    min: f32,
    max: f32,
    rect: Rect,
    dragging: bool,
    display_format: Box<dyn Fn(f32) -> String + Send>,
}

impl Slider {
    pub fn new(label: &str, min: f32, max: f32, value: f32) -> Self {
        let clamped = value.clamp(min, max);
        Self {
            label: label.to_owned(),
            value: clamped,
            min,
            max,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            dragging: false,
            display_format: Box::new(|v| format!("{v:.0}")),
        }
    }

    pub fn with_format(mut self, f: impl Fn(f32) -> String + Send + 'static) -> Self {
        self.display_format = Box::new(f);
        self
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn set_value(&mut self, v: f32) {
        self.value = v.clamp(self.min, self.max);
    }

    fn track_rect(&self) -> Rect {
        Rect::new(self.rect.x, self.rect.y + 20.0, self.rect.w, 6.0)
    }

    fn value_to_x(&self, v: f32) -> f32 {
        let track = self.track_rect();
        let norm = if (self.max - self.min).abs() < f32::EPSILON {
            0.0
        } else {
            (v - self.min) / (self.max - self.min)
        };
        track.x + norm * track.w
    }

    fn x_to_value(&self, x: f32) -> f32 {
        let track = self.track_rect();
        let norm = ((x - track.x) / track.w).clamp(0.0, 1.0);
        (self.min + norm * (self.max - self.min)).clamp(self.min, self.max)
    }
}

impl Widget for Slider {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, available.w, 32.0);
        Size {
            w: available.w,
            h: 32.0,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let track = self.track_rect();
        let thumb_x = self.value_to_x(self.value);
        const THUMB_SIZE: f32 = 12.0;

        // Label (left) and value (right)
        ctx.push_text_run(TextRun {
            x: self.rect.x,
            y: self.rect.y,
            text: self.label.clone(),
            font_size: 12.0,
            color: [1.0, 1.0, 1.0, 0.8],
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: self.rect.x + self.rect.w,
            y: self.rect.y,
            text: (self.display_format)(self.value),
            font_size: 12.0,
            color: [1.0, 1.0, 1.0, 0.9],
            monospace: true,
        });

        // Track background
        ctx.push_glass_quad(GlassQuad {
            rect: track,
            blur_rect: track,
            tint: [1.0, 1.0, 1.0, 0.08],
            border_color: [1.0, 1.0, 1.0, 0.1],
            corner_radius: 3.0,
            noise_intensity: 0.0,
        });

        // Fill from left to thumb
        let fill_w = (thumb_x - track.x).max(0.0);
        if fill_w > 0.0 {
            let fill_rect = Rect::new(track.x, track.y, fill_w, track.h);
            ctx.push_glow_rect(GlowRect {
                rect: fill_rect,
                color: [0.55, 0.36, 0.96, 0.8],
                spread: 2.0,
                intensity: 0.8,
            });
        }

        // Thumb
        let thumb_rect = Rect::new(
            thumb_x - THUMB_SIZE * 0.5,
            track.y + (track.h - THUMB_SIZE) * 0.5,
            THUMB_SIZE,
            THUMB_SIZE,
        );
        ctx.push_glow_rect(GlowRect {
            rect: thumb_rect,
            color: [0.55, 0.36, 0.96, 1.0],
            spread: 4.0,
            intensity: 1.0,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                let track = self.track_rect();
                // Expand track hit area by ±8px vertically
                let expanded = Rect::new(track.x, track.y - 8.0, track.w, track.h + 16.0);
                if expanded.contains(*x, *y) {
                    self.dragging = true;
                    self.value = self.x_to_value(*x);
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::MouseMove { x, .. } => {
                if self.dragging {
                    self.value = self.x_to_value(*x);
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if self.dragging {
                    self.dragging = false;
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
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
        Rect::new(0.0, 0.0, 200.0, 100.0)
    }

    #[test]
    fn slider_clamps_value() {
        let s = Slider::new("Volume", 0.0, 100.0, 150.0);
        assert!((s.value() - 100.0).abs() < 0.01, "value was {}", s.value());
    }

    #[test]
    fn slider_drag_updates_value() {
        let mut s = Slider::new("Volume", 0.0, 100.0, 0.0);
        s.layout(available());

        // The track is at y+20, width=200. Clicking at x=100 (midpoint) should give ~50.
        let resp = s.handle_event(&UiEvent::MouseDown {
            x: 100.0,
            y: 24.0, // within ±8 of track y=20
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Consumed));
        assert!(
            (s.value() - 50.0).abs() < 1.0,
            "expected ~50, got {}",
            s.value()
        );

        // Drag to x=200 (right edge) → should be 100
        let resp2 = s.handle_event(&UiEvent::MouseMove { x: 200.0, y: 24.0 });
        assert!(matches!(resp2, EventResponse::Consumed));
        assert!(
            (s.value() - 100.0).abs() < 0.01,
            "expected 100, got {}",
            s.value()
        );

        // MouseUp stops dragging
        let resp3 = s.handle_event(&UiEvent::MouseUp {
            x: 200.0,
            y: 24.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp3, EventResponse::Consumed));
    }
}
