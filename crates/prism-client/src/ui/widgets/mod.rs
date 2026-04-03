// SPDX-License-Identifier: AGPL-3.0-or-later
//! Widget trait, layout primitives, and draw batching.

pub mod button;
pub mod checkbox;
pub mod dropdown;
pub mod icon;
pub mod label;
pub mod monitor_map;
pub mod segmented;
pub mod separator;
pub mod slider;
pub mod sparkline;
pub mod text_input;
pub mod toggle;

use crate::ui::launcher::LauncherTab;

// ---------------------------------------------------------------------------
// Color mode
// ---------------------------------------------------------------------------

/// Controls whether a widget renders with dark-glass (overlay) or
/// light-Mica (launcher) colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    #[default]
    Dark,
    Light,
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }
}

/// 2-D size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

// ---------------------------------------------------------------------------
// Draw commands
// ---------------------------------------------------------------------------

/// Frosted glass draw command.
#[derive(Debug, Clone)]
pub struct GlassQuad {
    pub rect: Rect,
    pub blur_rect: Rect,
    pub tint: [f32; 4],
    pub border_color: [f32; 4],
    pub corner_radius: f32,
    pub noise_intensity: f32,
}

impl Default for GlassQuad {
    fn default() -> Self {
        Self {
            rect: Rect::ZERO,
            blur_rect: Rect::ZERO,
            tint: [0.0; 4],
            border_color: [0.0; 4],
            corner_radius: 0.0,
            noise_intensity: 0.0,
        }
    }
}

/// Text draw command.
#[derive(Debug, Clone)]
pub struct TextRun {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub monospace: bool,
    pub bold: bool,
    pub icon: bool,
}

impl Default for TextRun {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            text: String::new(),
            font_size: 14.0,
            color: [0.0, 0.0, 0.0, 1.0],
            monospace: false,
            bold: false,
            icon: false,
        }
    }
}

/// Accent glow draw command.
#[derive(Debug, Clone)]
pub struct GlowRect {
    pub rect: Rect,
    pub color: [f32; 4],
    pub spread: f32,
    pub intensity: f32,
}

// ---------------------------------------------------------------------------
// PaintContext
// ---------------------------------------------------------------------------

/// Collects draw commands that get batched into GPU draw calls.
#[derive(Debug, Clone)]
pub struct PaintContext {
    pub glass_quads: Vec<GlassQuad>,
    pub text_runs: Vec<TextRun>,
    pub glow_rects: Vec<GlowRect>,
}

impl PaintContext {
    pub fn new() -> Self {
        Self {
            glass_quads: Vec::with_capacity(64),
            text_runs: Vec::with_capacity(128),
            glow_rects: Vec::with_capacity(32),
        }
    }

    pub fn clear(&mut self) {
        self.glass_quads.clear();
        self.text_runs.clear();
        self.glow_rects.clear();
    }

    pub fn push_glass_quad(&mut self, quad: GlassQuad) {
        self.glass_quads.push(quad);
    }

    pub fn push_text_run(&mut self, run: TextRun) {
        self.text_runs.push(run);
    }

    pub fn push_glow_rect(&mut self, glow: GlowRect) {
        self.glow_rects.push(glow);
    }
}

impl Default for PaintContext {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone)]
pub enum KeyCode {
    Enter,
    Escape,
    Tab,
    ShiftTab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    CtrlA,
    CtrlC,
    CtrlV,
    CtrlX,
    Other(u32),
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    MouseMove { x: f32, y: f32 },
    MouseDown { x: f32, y: f32, button: MouseButton },
    MouseUp { x: f32, y: f32, button: MouseButton },
    KeyDown { key: KeyCode },
    KeyUp { key: KeyCode },
    Scroll { dx: f32, dy: f32 },
    TextInput { ch: char },
}

// ---------------------------------------------------------------------------
// Event response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum EventResponse {
    Ignored,
    Consumed,
    Action(UiAction),
}

#[derive(Debug, Clone)]
pub enum UiAction {
    Connect {
        address: String,
        noise_key: Option<String>,
    },
    OpenLauncherTab(LauncherTab),
    Disconnect,
    SwitchServer {
        address: String,
    },
    SwitchProfile(String),
    UpdateQuality {
        preset: Option<String>,
        max_fps: Option<u8>,
        lossless_text: Option<bool>,
        region_detection: Option<bool>,
    },
    SetBandwidthLimit(u64),
    SelectMonitor(u8),
    OpenPanel(String),
    ClosePanel(String),
    CloseOverlay,
    TogglePinStatsBar,
    EditServer(uuid::Uuid),
    DeleteServer(uuid::Uuid),
    AddServer,
    SaveServer,
    CancelModal,
    ConfirmDeleteServer(uuid::Uuid),
    OpenSettingsSection(crate::ui::launcher::SettingsSection),
}

// ---------------------------------------------------------------------------
// Widget trait
// ---------------------------------------------------------------------------

pub trait Widget {
    fn layout(&mut self, available: Rect) -> Size;
    fn paint(&self, ctx: &mut PaintContext);
    fn handle_event(&mut self, event: &UiEvent) -> EventResponse;
    fn animate(&mut self, dt_ms: f32);
}

// ---------------------------------------------------------------------------
// SpatialHash — O(1) hit testing
// ---------------------------------------------------------------------------

/// Grid-based spatial hash for fast mouse-hit testing.
pub struct SpatialHash {
    cells: Vec<Vec<usize>>,
    cell_w: f32,
    cell_h: f32,
    cols: usize,
    rows: usize,
}

impl SpatialHash {
    /// Create a new grid covering `width × height` logical pixels, divided
    /// into `divisions × divisions` cells.
    pub fn new(width: f32, height: f32, divisions: usize) -> Self {
        let cols = divisions.max(1);
        let rows = divisions.max(1);
        let cell_w = width / cols as f32;
        let cell_h = height / rows as f32;
        let cells = vec![Vec::new(); cols * rows];
        Self {
            cells,
            cell_w,
            cell_h,
            cols,
            rows,
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    /// Insert widget `id` into every cell that overlaps `rect`.
    pub fn insert(&mut self, id: usize, rect: Rect) {
        let col0 = ((rect.x / self.cell_w) as usize).min(self.cols.saturating_sub(1));
        let col1 = (((rect.x + rect.w) / self.cell_w) as usize).min(self.cols.saturating_sub(1));
        let row0 = ((rect.y / self.cell_h) as usize).min(self.rows.saturating_sub(1));
        let row1 = (((rect.y + rect.h) / self.cell_h) as usize).min(self.rows.saturating_sub(1));

        for row in row0..=row1 {
            for col in col0..=col1 {
                self.cells[row * self.cols + col].push(id);
            }
        }
    }

    /// Return the slice of widget IDs whose cell contains point `(x, y)`.
    pub fn query(&self, x: f32, y: f32) -> &[usize] {
        let col = ((x / self.cell_w) as usize).min(self.cols.saturating_sub(1));
        let row = ((y / self.cell_h) as usize).min(self.rows.saturating_sub(1));
        &self.cells[row * self.cols + col]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_point() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0);
        assert!(r.contains(15.0, 15.0));
        assert!(!r.contains(5.0, 15.0));
        assert!(!r.contains(15.0, 35.0));
    }

    #[test]
    fn rect_intersects() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        let c = Rect::new(20.0, 20.0, 10.0, 10.0);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn paint_context_collects_quads() {
        let mut ctx = PaintContext::new();
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        ctx.push_glass_quad(GlassQuad {
            rect: r,
            blur_rect: r,
            tint: [0.0; 4],
            border_color: [1.0; 4],
            corner_radius: 8.0,
            noise_intensity: 0.05,
        });
        assert_eq!(ctx.glass_quads.len(), 1);
    }

    #[test]
    fn paint_context_collects_text() {
        let mut ctx = PaintContext::new();
        ctx.push_text_run(TextRun {
            x: 0.0,
            y: 0.0,
            text: "hello".into(),
            font_size: 14.0,
            color: [1.0; 4],
            ..Default::default()
        });
        assert_eq!(ctx.text_runs.len(), 1);
    }

    #[test]
    fn spatial_hash_lookup() {
        let mut sh = SpatialHash::new(800.0, 600.0, 8);
        // Widget 0 lives in the top-left quadrant.
        sh.insert(0, Rect::new(10.0, 10.0, 50.0, 50.0));
        // Widget 1 lives in the bottom-right quadrant.
        sh.insert(1, Rect::new(700.0, 500.0, 50.0, 50.0));

        let hits_tl = sh.query(30.0, 30.0);
        assert!(hits_tl.contains(&0), "expected widget 0 at (30,30)");
        assert!(!hits_tl.contains(&1), "widget 1 should not be at (30,30)");

        let hits_br = sh.query(720.0, 520.0);
        assert!(hits_br.contains(&1), "expected widget 1 at (720,520)");
        assert!(!hits_br.contains(&0), "widget 0 should not be at (720,520)");
    }
}
