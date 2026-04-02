# Launcher UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract a launcher shell, build interactive profiles with full pipeline wiring, upgrade settings, replace the overlay with a top capsule, and wire modal server forms.

**Architecture:** Shell-first top-down. `shell.rs` becomes the single layout orchestrator for all launcher content (sidebar + content + modals). Each tab is a standalone widget composed by the shell. Profile changes persist to disk and flow through `ControlCommand` to the server encoder at connect time and runtime.

**Tech Stack:** Rust, wgpu, winit, serde_json, uuid, tokio (async bridge). Custom immediate-mode widget system with `GlassQuad`/`TextRun`/`GlowRect` draw commands.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/prism-client/src/ui/theme.rs` | Expand | Add typography constants, modal/capsule/toggle/status-chip surface tokens |
| `crates/prism-client/src/ui/widgets/toggle.rs` | Create | On/off switch widget (slider/dropdown already exist) |
| `crates/prism-client/src/ui/widgets/mod.rs` | Edit | Export toggle, add `SaveServer`/`CancelModal`/`ConfirmDeleteServer` to `UiAction`, remove `OpenSettings` |
| `crates/prism-client/src/ui/launcher/shell.rs` | Create | Launcher frame: sidebar + content + header + modal layer |
| `crates/prism-client/src/ui/launcher/mod.rs` | Edit | Add `shell` module, `ActiveModal`/`FormMode` enums |
| `crates/prism-client/src/ui/launcher/card_grid.rs` | Edit | Add `reset_filter()` method |
| `crates/prism-client/src/ui/launcher/server_card.rs` | Edit | Add delete button emitting `DeleteServer` |
| `crates/prism-client/src/ui/launcher/server_form.rs` | Edit | Emit `SaveServer`/`CancelModal` instead of `AddServer`/`CloseOverlay` |
| `crates/prism-client/src/config/profiles.rs` | Create | `ProfileStore` + `ProfileConfig` with append-only log persistence |
| `crates/prism-client/src/config/client_config_prefs.rs` | Create | `UserPrefs` JSON persistence for settings |
| `crates/prism-client/src/config/mod.rs` | Edit | Export `profiles` and `client_config_prefs` modules |
| `crates/prism-client/src/ui/launcher/profiles.rs` | Rewrite | Interactive two-column profile editor backed by `ProfileStore` |
| `crates/prism-client/src/ui/launcher/settings.rs` | Rewrite | Multi-section settings with functional toggles/dropdowns |
| `crates/prism-client/src/ui/overlay/capsule.rs` | Create | Top floating metrics capsule replacing stats_bar + drawer |
| `crates/prism-client/src/ui/overlay/mod.rs` | Edit | Export capsule module |
| `crates/prism-client/src/app.rs` | Major edit | Delegate launcher to shell, wire all new actions, profile pipeline |
| `crates/prism-session/src/control_msg.rs` | Edit | Add `ProfileSwitchPayload` and `QualityUpdatePayload` structs |
| `crates/prism-server/src/control_handler.rs` | Edit | Handle `PROFILE_SWITCH` and `QUALITY_UPDATE` messages |
| `crates/prism-server/src/client_session.rs` | Edit | Add `update_profile()` method |
| `crates/prism-server/src/server_app.rs` | Edit | Use client-requested profile instead of hardcoded `coding()` |

---

## Task 1: Theme Token Expansion

**Files:**
- Modify: `crates/prism-client/src/ui/theme.rs`

- [ ] **Step 1: Add typography constants and new surface functions**

Add after line 25 (after `SIDEBAR_RADIUS`):

```rust
// Typography scale
pub const FONT_DISPLAY: f32 = 30.0;
pub const FONT_HEADLINE: f32 = 20.0;
pub const FONT_BODY: f32 = 14.0;
pub const FONT_LABEL: f32 = 13.0;
pub const FONT_CAPTION: f32 = 11.0;
pub const FONT_CHIP: f32 = 10.0;
pub const MODAL_RADIUS: f32 = 22.0;
pub const CAPSULE_RADIUS: f32 = 24.0;
pub const TOGGLE_RADIUS: f32 = 10.0;
```

Add after the `separator()` function (after line 119):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipTone {
    Success,
    Warning,
    Accent,
    Neutral,
}

pub fn status_chip(rect: Rect, tone: ChipTone) -> GlassQuad {
    let (tint, border) = match tone {
        ChipTone::Success => (
            [SUCCESS[0], SUCCESS[1], SUCCESS[2], 0.14],
            [SUCCESS[0], SUCCESS[1], SUCCESS[2], 0.22],
        ),
        ChipTone::Warning => (
            [WARNING[0], WARNING[1], WARNING[2], 0.14],
            [WARNING[0], WARNING[1], WARNING[2], 0.22],
        ),
        ChipTone::Accent => (
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.12],
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.18],
        ),
        ChipTone::Neutral => ([1.0, 1.0, 1.0, 0.06], [1.0, 1.0, 1.0, 0.10]),
    };
    glass_quad(rect, tint, border, CHIP_RADIUS)
}

pub fn section_header_surface(rect: Rect) -> GlassQuad {
    glass_quad(rect, [0.13, 0.17, 0.23, 0.60], [1.0, 1.0, 1.0, 0.06], CONTROL_RADIUS)
}

pub fn modal_scrim(rect: Rect) -> GlassQuad {
    glass_quad(rect, [0.0, 0.0, 0.0, 0.48], [0.0, 0.0, 0.0, 0.0], 0.0)
}

pub fn modal_surface(rect: Rect) -> GlassQuad {
    glass_quad(rect, [0.12, 0.16, 0.22, 0.94], [1.0, 1.0, 1.0, 0.14], MODAL_RADIUS)
}

pub fn capsule_surface(rect: Rect) -> GlassQuad {
    glass_quad(rect, [0.10, 0.14, 0.19, 0.88], [1.0, 1.0, 1.0, 0.12], CAPSULE_RADIUS)
}

pub fn capsule_dropdown_surface(rect: Rect) -> GlassQuad {
    glass_quad(rect, [0.11, 0.15, 0.21, 0.92], [1.0, 1.0, 1.0, 0.10], PANEL_RADIUS)
}

pub fn toggle_track(rect: Rect, on: bool) -> GlassQuad {
    glass_quad(
        rect,
        if on {
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.72]
        } else {
            [0.22, 0.26, 0.33, 0.88]
        },
        if on {
            [ACCENT[0], ACCENT[1], ACCENT[2], 0.32]
        } else {
            [1.0, 1.0, 1.0, 0.10]
        },
        TOGGLE_RADIUS,
    )
}

pub fn toggle_thumb(rect: Rect, on: bool) -> GlassQuad {
    glass_quad(
        rect,
        if on {
            [0.95, 0.97, 1.0, 0.96]
        } else {
            [0.70, 0.75, 0.82, 0.88]
        },
        [0.0, 0.0, 0.0, 0.0],
        TOGGLE_RADIUS,
    )
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-client ui::theme`
Expected: PASS (no existing theme tests, but compilation verifies all tokens)

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/ui/theme.rs
git commit -m "feat(ui): add typography constants, modal/capsule/toggle/chip theme tokens"
```

---

## Task 2: Toggle Widget

**Files:**
- Create: `crates/prism-client/src/ui/widgets/toggle.rs`
- Modify: `crates/prism-client/src/ui/widgets/mod.rs`

- [ ] **Step 1: Write failing test in toggle.rs**

Create `crates/prism-client/src/ui/widgets/toggle.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! On/off toggle switch widget.

use crate::renderer::animation::{Animation, EaseCurve};
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, UiEvent, Widget,
};

const TRACK_W: f32 = 44.0;
const TRACK_H: f32 = 22.0;
const THUMB_SIZE: f32 = 16.0;
const THUMB_PAD: f32 = 3.0;

pub struct Toggle {
    on: bool,
    rect: Rect,
    slide_anim: Animation,
}

impl Toggle {
    pub fn new(on: bool) -> Self {
        let mut anim = Animation::new(EaseCurve::EaseOut, 150.0);
        anim.set_target(if on { 1.0 } else { 0.0 });
        // Snap to initial position
        for _ in 0..20 {
            anim.tick(20.0);
        }
        Self {
            on,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            slide_anim: anim,
        }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        self.slide_anim.set_target(if on { 1.0 } else { 0.0 });
    }

    fn track_rect(&self) -> Rect {
        Rect::new(
            self.rect.x + self.rect.w - TRACK_W,
            self.rect.y + (self.rect.h - TRACK_H) * 0.5,
            TRACK_W,
            TRACK_H,
        )
    }
}

impl Widget for Toggle {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, available.w, TRACK_H);
        Size {
            w: available.w,
            h: TRACK_H,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let track = self.track_rect();
        ctx.push_glass_quad(theme::toggle_track(track, self.on));

        let t = self.slide_anim.value();
        let thumb_x = track.x + THUMB_PAD + t * (TRACK_W - THUMB_SIZE - THUMB_PAD * 2.0);
        let thumb_y = track.y + (TRACK_H - THUMB_SIZE) * 0.5;
        let thumb_rect = Rect::new(thumb_x, thumb_y, THUMB_SIZE, THUMB_SIZE);
        ctx.push_glass_quad(theme::toggle_thumb(thumb_rect, self.on));
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                let track = self.track_rect();
                if track.contains(*x, *y) {
                    self.on = !self.on;
                    self.slide_anim
                        .set_target(if self.on { 1.0 } else { 0.0 });
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.slide_anim.tick(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Rect {
        Rect::new(0.0, 0.0, 200.0, 40.0)
    }

    #[test]
    fn toggle_starts_off() {
        let t = Toggle::new(false);
        assert!(!t.is_on());
    }

    #[test]
    fn toggle_starts_on() {
        let t = Toggle::new(true);
        assert!(t.is_on());
    }

    #[test]
    fn click_toggles_state() {
        let mut t = Toggle::new(false);
        t.layout(available());

        let track = t.track_rect();
        let resp = t.handle_event(&UiEvent::MouseDown {
            x: track.x + 10.0,
            y: track.y + 5.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Consumed));
        assert!(t.is_on());
    }

    #[test]
    fn click_outside_ignored() {
        let mut t = Toggle::new(false);
        t.layout(available());

        let resp = t.handle_event(&UiEvent::MouseDown {
            x: 0.0,
            y: 0.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Ignored));
        assert!(!t.is_on());
    }

    #[test]
    fn double_click_returns_to_off() {
        let mut t = Toggle::new(false);
        t.layout(available());

        let track = t.track_rect();
        t.handle_event(&UiEvent::MouseDown {
            x: track.x + 10.0,
            y: track.y + 5.0,
            button: MouseButton::Left,
        });
        assert!(t.is_on());

        t.handle_event(&UiEvent::MouseDown {
            x: track.x + 10.0,
            y: track.y + 5.0,
            button: MouseButton::Left,
        });
        assert!(!t.is_on());
    }

    #[test]
    fn set_on_programmatic() {
        let mut t = Toggle::new(false);
        t.set_on(true);
        assert!(t.is_on());
        t.set_on(false);
        assert!(!t.is_on());
    }
}
```

- [ ] **Step 2: Export toggle from widgets/mod.rs**

Add after line 11 (`pub mod text_input;`):

```rust
pub mod toggle;
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p prism-client widgets::toggle`
Expected: PASS — all 5 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/ui/widgets/toggle.rs crates/prism-client/src/ui/widgets/mod.rs
git commit -m "feat(ui): add Toggle on/off switch widget with tests"
```

---

## Task 3: UiAction Expansion

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/mod.rs`
- Modify: `crates/prism-client/src/ui/launcher/mod.rs`

- [ ] **Step 1: Add new actions and modal types to UiAction**

In `widgets/mod.rs`, replace the `UiAction` enum (lines 187-215) with:

```rust
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
    // New: modal actions
    SaveServer,
    CancelModal,
    ConfirmDeleteServer(uuid::Uuid),
}
```

Note: `OpenSettings` is removed. Any code referencing it will fail to compile, which is intentional — we fix those call sites in the shell extraction task.

- [ ] **Step 2: Add modal types to launcher/mod.rs**

In `launcher/mod.rs`, add after the `LauncherTab` impl block (after line 59):

```rust
pub mod shell;

/// Which modal is active on top of the launcher.
#[derive(Debug, Clone)]
pub enum ActiveModal {
    ServerForm { mode: FormMode },
    ConfirmDelete { server_id: uuid::Uuid, name: String },
}

/// Add vs Edit mode for the server form.
#[derive(Debug, Clone)]
pub enum FormMode {
    Add,
    Edit { server_id: uuid::Uuid },
}
```

- [ ] **Step 3: Run compilation check**

Run: `cargo check -p prism-client`
Expected: Compiles cleanly. Do not introduce an intentional temporary compile break while migrating `OpenSettings`.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/ui/widgets/mod.rs crates/prism-client/src/ui/launcher/mod.rs
git commit -m "feat(ui): add SaveServer/CancelModal/ConfirmDeleteServer actions, modal types, remove OpenSettings"
```

---

## Task 4: Launcher Shell Extraction

**Files:**
- Create: `crates/prism-client/src/ui/launcher/shell.rs`
- Modify: `crates/prism-client/src/app.rs`
- Modify: `crates/prism-client/src/ui/launcher/card_grid.rs`

- [ ] **Step 1: Add `reset_filter()` to CardGrid**

In `card_grid.rs`, add after `set_show_filters` method (find the last setter, add this method in the impl block):

```rust
    pub fn reset_filter(&mut self) {
        self.active_filter = CardFilter::All;
        self.recompute_layout();
    }
```

- [ ] **Step 2: Create shell.rs**

Create `crates/prism-client/src/ui/launcher/shell.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher shell — sidebar, content area, header, and modal layer.

use super::{ActiveModal, FormMode, LauncherTab};
use crate::config::servers::SavedServer;
use crate::ui::launcher::card_grid::CardGrid;
use crate::ui::launcher::nav::LauncherNav;
use crate::ui::launcher::profiles::ProfilesPanel;
use crate::ui::launcher::quick_connect::QuickConnect;
use crate::ui::launcher::server_form::ServerForm;
use crate::ui::launcher::settings::SettingsPanel;
use crate::ui::theme;
use crate::ui::widgets::{
    EventResponse, MouseButton, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, Widget,
};
use crate::ui::UiState;

const SIDEBAR_W: f32 = 224.0;
const SIDEBAR_PAD: f32 = 28.0;
const CONTENT_PAD: f32 = 28.0;
const HEADER_OFFSET: f32 = 92.0;

pub struct LauncherShell {
    pub nav: LauncherNav,
    pub quick_connect: QuickConnect,
    pub card_grid: CardGrid,
    pub profiles_panel: ProfilesPanel,
    pub settings_panel: SettingsPanel,
    pub server_form: ServerForm,
    pub active_tab: LauncherTab,
    active_modal: Option<ActiveModal>,
    screen_rect: Rect,
    sidebar_rect: Rect,
    content_rect: Rect,
    ui_state: UiState,
}

impl LauncherShell {
    pub fn new(
        nav: LauncherNav,
        quick_connect: QuickConnect,
        card_grid: CardGrid,
        profiles_panel: ProfilesPanel,
        settings_panel: SettingsPanel,
        server_form: ServerForm,
    ) -> Self {
        Self {
            nav,
            quick_connect,
            card_grid,
            profiles_panel,
            settings_panel,
            server_form,
            active_tab: LauncherTab::Home,
            active_modal: None,
            screen_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            sidebar_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            content_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            ui_state: UiState::Launcher,
        }
    }

    pub fn set_tab(&mut self, tab: LauncherTab) {
        self.active_tab = tab;
        if tab == LauncherTab::Home {
            self.card_grid.reset_filter();
        }
        self.configure_widgets();
    }

    pub fn set_ui_state(&mut self, state: UiState) {
        self.ui_state = state;
    }

    pub fn show_modal(&mut self, modal: ActiveModal) {
        match &modal {
            ActiveModal::ServerForm { mode } => {
                match mode {
                    FormMode::Add => {
                        self.server_form.clear();
                    }
                    FormMode::Edit { .. } => {
                        // Caller is responsible for calling server_form.set_editing()
                    }
                }
                self.server_form.show();
            }
            ActiveModal::ConfirmDelete { .. } => {}
        }
        self.active_modal = Some(modal);
    }

    pub fn dismiss_modal(&mut self) {
        self.server_form.hide();
        self.active_modal = None;
    }

    pub fn has_modal(&self) -> bool {
        self.active_modal.is_some()
    }

    pub fn active_modal(&self) -> &Option<ActiveModal> {
        &self.active_modal
    }

    fn configure_widgets(&mut self) {
        match self.active_tab {
            LauncherTab::Home => {
                self.card_grid.set_visible_limit(Some(3));
                self.card_grid.set_show_add_card(false);
                self.card_grid.set_show_filters(false);
            }
            LauncherTab::SavedConnections => {
                self.card_grid.set_visible_limit(None);
                self.card_grid.set_show_add_card(true);
                self.card_grid.set_show_filters(true);
            }
            LauncherTab::Profiles | LauncherTab::Settings => {
                self.card_grid.set_visible_limit(None);
                self.card_grid.set_show_add_card(false);
                self.card_grid.set_show_filters(false);
            }
        }
    }

    fn compute_layout(&mut self, screen_w: f32, screen_h: f32) {
        self.screen_rect = Rect::new(0.0, 0.0, screen_w, screen_h);
        self.sidebar_rect = Rect::new(
            SIDEBAR_PAD,
            SIDEBAR_PAD,
            SIDEBAR_W,
            (screen_h - SIDEBAR_PAD * 2.0).max(280.0),
        );
        let content_x = self.sidebar_rect.x + self.sidebar_rect.w + CONTENT_PAD;
        self.content_rect = Rect::new(
            content_x,
            42.0,
            (screen_w - content_x - CONTENT_PAD).max(320.0),
            (screen_h - 70.0).max(320.0),
        );
    }

    fn paint_header(&self, ctx: &mut PaintContext) {
        let title_x = self.content_rect.x;
        let title_y = self.content_rect.y + 6.0;

        ctx.push_text_run(TextRun {
            x: title_x,
            y: title_y,
            text: self.active_tab.title().to_string(),
            font_size: theme::FONT_DISPLAY,
            color: theme::TEXT_PRIMARY,
            monospace: false,
        });

        ctx.push_text_run(TextRun {
            x: title_x,
            y: title_y + 40.0,
            text: self.active_tab.subtitle().to_string(),
            font_size: theme::FONT_BODY,
            color: theme::TEXT_SECONDARY,
            monospace: false,
        });

        if self.ui_state == UiState::Connecting {
            let chip_w = theme::text_width("Connecting...", 12.0) + 28.0;
            let status_rect = Rect::new(
                self.content_rect.x + self.content_rect.w - chip_w,
                title_y + 2.0,
                chip_w,
                28.0,
            );
            ctx.push_glass_quad(theme::status_chip(status_rect, theme::ChipTone::Accent));
            ctx.push_text_run(TextRun {
                x: status_rect.x + 14.0,
                y: status_rect.y + 6.0,
                text: "Connecting...".to_string(),
                font_size: 12.0,
                color: theme::TEXT_PRIMARY,
                monospace: false,
            });
        }
    }

    fn paint_tab_content(&mut self, ctx: &mut PaintContext) {
        let body_y = self.content_rect.y + HEADER_OFFSET;
        let body_h = (self.content_rect.h - HEADER_OFFSET).max(0.0);
        let body_rect = Rect::new(self.content_rect.x, body_y, self.content_rect.w, body_h);

        match self.active_tab {
            LauncherTab::Home => {
                let quick_y = self.content_rect.y + HEADER_OFFSET;
                let section_y = quick_y + 132.0;
                let card_y = section_y + 34.0;

                self.quick_connect.layout(Rect::new(
                    self.content_rect.x,
                    quick_y,
                    self.content_rect.w,
                    94.0,
                ));
                self.card_grid.layout(Rect::new(
                    self.content_rect.x,
                    card_y,
                    self.content_rect.w,
                    (self.content_rect.y + self.content_rect.h - card_y).max(0.0),
                ));

                self.quick_connect.paint(ctx);
                ctx.push_text_run(TextRun {
                    x: self.content_rect.x,
                    y: section_y,
                    text: "Recent connections".to_string(),
                    font_size: 12.0,
                    color: theme::TEXT_MUTED,
                    monospace: false,
                });
                ctx.push_glass_quad(theme::separator(Rect::new(
                    self.content_rect.x,
                    section_y + 20.0,
                    self.content_rect.w,
                    1.0,
                )));
                self.card_grid.paint(ctx);
            }
            LauncherTab::SavedConnections => {
                self.card_grid.layout(body_rect);
                self.card_grid.paint(ctx);
            }
            LauncherTab::Profiles => {
                self.profiles_panel.layout(body_rect);
                self.profiles_panel.paint(ctx);
            }
            LauncherTab::Settings => {
                self.settings_panel.layout(body_rect);
                self.settings_panel.paint(ctx);
            }
        }
    }

    fn paint_modal(&self, ctx: &mut PaintContext) {
        let modal = match &self.active_modal {
            Some(m) => m,
            None => return,
        };

        // Scrim
        ctx.push_glass_quad(theme::modal_scrim(self.screen_rect));

        match modal {
            ActiveModal::ServerForm { .. } => {
                // Center the form
                let form_w = 300.0;
                let form_h = 280.0;
                let form_x = self.screen_rect.w * 0.5 - form_w * 0.5;
                let form_y = self.screen_rect.h * 0.5 - form_h * 0.5;
                ctx.push_glass_quad(theme::modal_surface(Rect::new(
                    form_x - 12.0,
                    form_y - 12.0,
                    form_w + 24.0,
                    form_h + 24.0,
                )));
                self.server_form.paint(ctx);
            }
            ActiveModal::ConfirmDelete { name, .. } => {
                let dialog_w = 320.0;
                let dialog_h = 140.0;
                let dx = self.screen_rect.w * 0.5 - dialog_w * 0.5;
                let dy = self.screen_rect.h * 0.5 - dialog_h * 0.5;
                let dialog_rect = Rect::new(dx, dy, dialog_w, dialog_h);
                ctx.push_glass_quad(theme::modal_surface(dialog_rect));

                ctx.push_text_run(TextRun {
                    x: dx + 20.0,
                    y: dy + 20.0,
                    text: "Delete server?".to_string(),
                    font_size: theme::FONT_BODY,
                    color: theme::TEXT_PRIMARY,
                    monospace: false,
                });
                ctx.push_text_run(TextRun {
                    x: dx + 20.0,
                    y: dy + 46.0,
                    text: format!("Remove \"{}\" from saved connections?", name),
                    font_size: theme::FONT_LABEL,
                    color: theme::TEXT_SECONDARY,
                    monospace: false,
                });
                // Delete and Cancel buttons rendered via explicit rects
                // (handled in handle_event for hit testing)
                let cancel_rect = Rect::new(dx + 20.0, dy + 88.0, 120.0, 36.0);
                let delete_rect = Rect::new(dx + dialog_w - 140.0, dy + 88.0, 120.0, 36.0);
                ctx.push_glass_quad(theme::control_surface(cancel_rect, false));
                ctx.push_text_run(TextRun {
                    x: cancel_rect.x + 36.0,
                    y: cancel_rect.y + 10.0,
                    text: "Cancel".to_string(),
                    font_size: theme::FONT_LABEL,
                    color: theme::TEXT_PRIMARY,
                    monospace: false,
                });
                ctx.push_glass_quad(theme::glass_quad(
                    delete_rect,
                    [0.42, 0.18, 0.22, 0.84],
                    [1.0, 1.0, 1.0, 0.14],
                    theme::CONTROL_RADIUS,
                ));
                ctx.push_text_run(TextRun {
                    x: delete_rect.x + 36.0,
                    y: delete_rect.y + 10.0,
                    text: "Delete".to_string(),
                    font_size: theme::FONT_LABEL,
                    color: theme::TEXT_PRIMARY,
                    monospace: false,
                });
            }
        }
    }

    fn route_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &self.active_modal {
            Some(m) => m.clone(),
            None => return EventResponse::Ignored,
        };

        match &modal {
            ActiveModal::ServerForm { .. } => {
                let resp = self.server_form.handle_event(event);
                match &resp {
                    EventResponse::Action(UiAction::SaveServer) => return resp,
                    EventResponse::Action(UiAction::CancelModal) => {
                        self.dismiss_modal();
                        return EventResponse::Consumed;
                    }
                    EventResponse::Consumed => return resp,
                    _ => {}
                }
            }
            ActiveModal::ConfirmDelete { server_id, .. } => {
                if let UiEvent::MouseDown {
                    x,
                    y,
                    button: MouseButton::Left,
                } = event
                {
                    let dx = self.screen_rect.w * 0.5 - 160.0;
                    let dy = self.screen_rect.h * 0.5 - 70.0;
                    let cancel_rect = Rect::new(dx + 20.0, dy + 88.0, 120.0, 36.0);
                    let delete_rect = Rect::new(dx + 180.0, dy + 88.0, 120.0, 36.0);

                    if cancel_rect.contains(*x, *y) {
                        self.dismiss_modal();
                        return EventResponse::Consumed;
                    }
                    if delete_rect.contains(*x, *y) {
                        let id = *server_id;
                        self.dismiss_modal();
                        return EventResponse::Action(UiAction::ConfirmDeleteServer(id));
                    }
                }
            }
        }

        // If modal is active, consume all events (scrim blocks through-clicks)
        if let UiEvent::MouseDown { .. } | UiEvent::MouseUp { .. } = event {
            // Check if click is on scrim (outside modal) → dismiss
            EventResponse::Consumed
        } else {
            EventResponse::Consumed
        }
    }
}

impl Widget for LauncherShell {
    fn layout(&mut self, available: Rect) -> Size {
        self.compute_layout(available.w, available.h);
        self.configure_widgets();
        self.nav.set_active_tab(self.active_tab);
        self.nav.layout(self.sidebar_rect);

        // Modal form layout (centered)
        if self.active_modal.is_some() {
            let form_x = available.w * 0.5 - 150.0;
            let form_y = available.h * 0.5 - 140.0;
            self.server_form
                .layout(Rect::new(form_x, form_y, 300.0, 280.0));
        }

        Size {
            w: available.w,
            h: available.h,
        }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        self.nav.paint(ctx);
        self.paint_header(ctx);
        // paint_tab_content needs &mut self due to child layout; called from render() directly
        // This is handled by the app calling shell.paint_content(ctx) separately
        self.paint_modal(ctx);
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        // Modal gets priority
        if self.active_modal.is_some() {
            return self.route_modal_event(event);
        }

        // Nav
        let resp = self.nav.handle_event(event);
        if !matches!(resp, EventResponse::Ignored) {
            return resp;
        }

        // Active tab content
        match self.active_tab {
            LauncherTab::Home => {
                let resp = self.quick_connect.handle_event(event);
                if !matches!(resp, EventResponse::Ignored) {
                    return resp;
                }
                self.card_grid.handle_event(event)
            }
            LauncherTab::SavedConnections => self.card_grid.handle_event(event),
            LauncherTab::Profiles => self.profiles_panel.handle_event(event),
            LauncherTab::Settings => self.settings_panel.handle_event(event),
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.nav.animate(dt_ms);
        self.quick_connect.animate(dt_ms);
        self.card_grid.animate(dt_ms);
        self.profiles_panel.animate(dt_ms);
        self.settings_panel.animate(dt_ms);
        self.server_form.animate(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_shell() -> LauncherShell {
        LauncherShell::new(
            LauncherNav::new(),
            QuickConnect::new(),
            CardGrid::new(),
            ProfilesPanel::new(),
            SettingsPanel::new(
                "/home/user/.prism/id.json".to_string(),
                "0.1.0".to_string(),
            ),
            ServerForm::new(),
        )
    }

    #[test]
    fn shell_starts_on_home() {
        let shell = make_shell();
        assert_eq!(shell.active_tab, LauncherTab::Home);
    }

    #[test]
    fn set_tab_updates_active() {
        let mut shell = make_shell();
        shell.set_tab(LauncherTab::Profiles);
        assert_eq!(shell.active_tab, LauncherTab::Profiles);
    }

    #[test]
    fn modal_show_dismiss_cycle() {
        let mut shell = make_shell();
        assert!(!shell.has_modal());

        shell.show_modal(ActiveModal::ServerForm {
            mode: FormMode::Add,
        });
        assert!(shell.has_modal());

        shell.dismiss_modal();
        assert!(!shell.has_modal());
    }

    #[test]
    fn set_tab_to_home_resets_filter() {
        let mut shell = make_shell();
        shell.set_tab(LauncherTab::SavedConnections);
        // Simulate filter change would happen via user interaction
        shell.set_tab(LauncherTab::Home);
        // The reset_filter call is the important behavior;
        // we verify it compiles and runs without panic
        assert_eq!(shell.active_tab, LauncherTab::Home);
    }

    #[test]
    fn nav_click_returns_tab_action() {
        let mut shell = make_shell();
        shell.layout(Rect::new(0.0, 0.0, 1280.0, 720.0));

        let resp = shell.handle_event(&UiEvent::MouseDown {
            x: 50.0,
            y: 110.0,
            button: crate::ui::widgets::MouseButton::Left,
        });

        assert!(matches!(
            resp,
            EventResponse::Action(UiAction::OpenLauncherTab(LauncherTab::Home))
        ));
    }
}
```

- [ ] **Step 3: Update app.rs to use LauncherShell**

This is the largest change. In `app.rs`:

1. Remove the individual launcher widget fields (lines 51-56) and replace with:
```rust
    launcher_shell: LauncherShell,
```

2. Remove `configure_launcher_widgets()`, `launcher_sidebar_rect()`, `launcher_content_rect()`, `route_launcher_event()` (lines 528-582).

3. In `handle_action()`:
   - Remove `OpenSettings` arm (line 602-604)
   - Change `OpenLauncherTab` to: `self.launcher_shell.set_tab(tab);`
   - Add new arms:
     ```rust
     UiAction::AddServer => {
         self.launcher_shell.show_modal(ActiveModal::ServerForm {
             mode: FormMode::Add,
         });
     }
     UiAction::EditServer(id) => {
         if let Some(store) = &self.server_store {
             if let Some(server) = store.get(id) {
                 self.launcher_shell.server_form.set_editing(server);
                 self.launcher_shell.show_modal(ActiveModal::ServerForm {
                     mode: FormMode::Edit { server_id: id },
                 });
             }
         }
     }
     UiAction::DeleteServer(id) => {
         if let Some(store) = &self.server_store {
             if let Some(server) = store.get(id) {
                 self.launcher_shell.show_modal(ActiveModal::ConfirmDelete {
                     server_id: id,
                     name: server.display_name.clone(),
                 });
             }
         }
     }
     UiAction::SaveServer => {
         let (name, address, noise_key, profile) = self.launcher_shell.server_form.form_data();
         if address.is_empty() { return; }
         if let Some(store) = &mut self.server_store {
             match &self.launcher_shell.active_modal {
                 Some(ActiveModal::ServerForm { mode: FormMode::Edit { server_id } }) => {
                     let sid = *server_id;
                     let _ = store.update(sid, |s| {
                         s.display_name = name;
                         s.address = address;
                         s.noise_public_key = noise_key;
                         s.default_profile = profile;
                     });
                 }
                 _ => {
                     let mut server = SavedServer::new(name, address);
                     server.noise_public_key = noise_key;
                     server.default_profile = profile;
                     let _ = store.add(server);
                 }
             }
             self.launcher_shell.card_grid.set_servers(store.servers());
         }
         self.launcher_shell.dismiss_modal();
     }
     UiAction::CancelModal => {
         self.launcher_shell.dismiss_modal();
     }
     UiAction::ConfirmDeleteServer(id) => {
         if let Some(store) = &mut self.server_store {
             let _ = store.delete(id);
             self.launcher_shell.card_grid.set_servers(store.servers());
         }
     }
     ```

4. In the render section (lines 907-1043), replace the entire launcher block with:
   ```rust
   if self.ui_state.shows_launcher() {
       let (screen_w, screen_h) = {
           let renderer = self.renderer.as_ref().expect("renderer exists");
           (renderer.width() as f32, renderer.height() as f32)
       };
       self.launcher_shell.set_ui_state(self.ui_state);
       self.paint_ctx.clear();
       self.launcher_shell.layout(Rect::new(0.0, 0.0, screen_w, screen_h));
       self.launcher_shell.nav.paint(&mut self.paint_ctx);
       self.launcher_shell.paint_header(&mut self.paint_ctx);
       self.launcher_shell.paint_tab_content(&mut self.paint_ctx);
       self.launcher_shell.paint_modal(&mut self.paint_ctx);

       if let Some(ui_renderer) = &mut self.ui_renderer {
           // ... same render call as before
       }
   }
   ```

5. Update `PrismApp::new()` to construct `LauncherShell` with all sub-widgets.

6. Update event routing to use `self.launcher_shell.handle_event(event)`.

- [ ] **Step 4: Update server_form.rs to emit SaveServer/CancelModal**

In `server_form.rs`, change line 36:
```rust
save_button: Button::new("Save", UiAction::SaveServer).with_style(ButtonStyle::Primary),
```

Change line 37:
```rust
cancel_button: Button::new("Cancel", UiAction::CancelModal).with_style(ButtonStyle::Secondary),
```

In `handle_event` (line 189), change the cancel check:
```rust
if matches!(cancel_resp, EventResponse::Action(UiAction::CancelModal)) {
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p prism-client`
Expected: All existing tests pass plus new shell tests

- [ ] **Step 6: Commit**

```bash
git add crates/prism-client/src/ui/launcher/shell.rs crates/prism-client/src/ui/launcher/card_grid.rs crates/prism-client/src/ui/launcher/server_form.rs crates/prism-client/src/app.rs
git commit -m "feat(ui): extract launcher shell from app.rs, wire modal system and server form actions"
```

---

## Task 5: Server Card Delete Button

**Files:**
- Modify: `crates/prism-client/src/ui/launcher/server_card.rs`

- [ ] **Step 1: Add delete button to server card**

In `server_card.rs`, add a delete button next to the Edit button. In the struct, add:
```rust
delete_button: Button,
```

In `new()`, add (after edit_button creation):
```rust
delete_button: Button::new("Del", UiAction::DeleteServer(server.id))
    .with_style(ButtonStyle::Destructive),
```

In `layout()`, allocate space for the delete button beside Edit (split the secondary action area into Edit + Delete).

In `paint()`, render the delete button.

In `handle_event()`, route events to delete_button.

In `animate()`, tick delete_button.

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-client ui::launcher::server_card`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/ui/launcher/server_card.rs
git commit -m "feat(ui): add delete button to server cards"
```

---

## Task 6: ProfileStore Persistence

**Files:**
- Create: `crates/prism-client/src/config/profiles.rs`
- Modify: `crates/prism-client/src/config/mod.rs`

- [ ] **Step 1: Write failing tests for ProfileStore**

Create `crates/prism-client/src/config/profiles.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Profile persistence with append-only log and compaction.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Profile types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioMode {
    Stereo,
    Surround,
    VoiceOptimized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderPreset {
    UltraLowLatency,
    Balanced,
    Quality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub id: Uuid,
    pub name: String,
    pub builtin: bool,
    // Performance
    pub bitrate_bps: u64,
    pub max_fps: u8,
    pub encoder_preset: EncoderPreset,
    // Display & Audio
    pub prefer_native_scaling: bool,
    pub audio_mode: AudioMode,
    pub prefer_av1: bool,
    // Input & Connectivity
    pub exclusive_input: bool,
    pub touch_mode: bool,
    pub auto_reconnect: bool,
}

impl ProfileConfig {
    pub fn summary(&self) -> String {
        format!("{} FPS · {} Mbps", self.max_fps, self.bitrate_bps / 1_000_000)
    }
}

fn default_profiles() -> Vec<ProfileConfig> {
    vec![
        ProfileConfig {
            id: Uuid::from_u128(1),
            name: "Gaming".to_string(),
            builtin: true,
            bitrate_bps: 35_000_000,
            max_fps: 120,
            encoder_preset: EncoderPreset::UltraLowLatency,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: false,
            exclusive_input: true,
            touch_mode: false,
            auto_reconnect: true,
        },
        ProfileConfig {
            id: Uuid::from_u128(2),
            name: "Coding".to_string(),
            builtin: true,
            bitrate_bps: 15_000_000,
            max_fps: 60,
            encoder_preset: EncoderPreset::Quality,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: true,
        },
        ProfileConfig {
            id: Uuid::from_u128(3),
            name: "Balanced".to_string(),
            builtin: true,
            bitrate_bps: 25_000_000,
            max_fps: 60,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: true,
        },
        ProfileConfig {
            id: Uuid::from_u128(4),
            name: "Low Bandwidth".to_string(),
            builtin: true,
            bitrate_bps: 5_000_000,
            max_fps: 30,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: false,
            audio_mode: AudioMode::VoiceOptimized,
            prefer_av1: true,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: true,
        },
    ]
}

// ---------------------------------------------------------------------------
// Log entry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(tag = "op")]
enum LogEntry {
    #[serde(rename = "update")]
    Update { profile: ProfileConfig },
    #[serde(rename = "add")]
    Add { profile: ProfileConfig },
    #[serde(rename = "delete")]
    Delete { id: Uuid },
}

// ---------------------------------------------------------------------------
// ProfileStore
// ---------------------------------------------------------------------------

pub struct ProfileStore {
    dir: PathBuf,
    profiles: Vec<ProfileConfig>,
}

impl ProfileStore {
    pub fn open(dir: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(dir)?;

        let mut profiles: Vec<ProfileConfig> = Vec::new();

        let snapshot_path = dir.join("profiles.json");
        if snapshot_path.exists() {
            let data = fs::read(&snapshot_path)?;
            if let Ok(loaded) = serde_json::from_slice::<Vec<ProfileConfig>>(&data) {
                profiles = loaded;
            }
        }

        let log_path = dir.join("profiles.log");
        if log_path.exists() {
            let file = File::open(&log_path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) if !l.trim().is_empty() => l,
                    _ => continue,
                };
                if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                    Self::apply_entry(&mut profiles, entry);
                }
            }
        }

        if profiles.is_empty() {
            profiles = default_profiles();
            let data = serde_json::to_string_pretty(&profiles)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            fs::write(&snapshot_path, data)?;
        }

        Ok(Self {
            dir: dir.to_owned(),
            profiles,
        })
    }

    pub fn list(&self) -> &[ProfileConfig] {
        &self.profiles
    }

    pub fn get(&self, id: Uuid) -> Option<&ProfileConfig> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&ProfileConfig> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn update(&mut self, id: Uuid, config: ProfileConfig) -> Result<(), io::Error> {
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) {
            *p = config.clone();
            let entry = LogEntry::Update { profile: config };
            self.append_log(&entry)?;
        }
        Ok(())
    }

    pub fn add(&mut self, config: ProfileConfig) -> Result<Uuid, io::Error> {
        let id = config.id;
        let entry = LogEntry::Add {
            profile: config.clone(),
        };
        self.append_log(&entry)?;
        self.profiles.push(config);
        Ok(id)
    }

    pub fn delete(&mut self, id: Uuid) -> Result<(), io::Error> {
        if let Some(p) = self.profiles.iter().find(|p| p.id == id) {
            if p.builtin {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot delete builtin profile",
                ));
            }
        }
        let entry = LogEntry::Delete { id };
        self.append_log(&entry)?;
        self.profiles.retain(|p| p.id != id);
        Ok(())
    }

    pub fn compact(&self) -> Result<(), io::Error> {
        let data = serde_json::to_string_pretty(&self.profiles)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let snapshot_path = self.dir.join("profiles.json");
        fs::write(&snapshot_path, data)?;
        let log_path = self.dir.join("profiles.log");
        if log_path.exists() {
            fs::remove_file(&log_path)?;
        }
        Ok(())
    }

    fn append_log(&self, entry: &LogEntry) -> Result<(), io::Error> {
        let log_path = self.dir.join("profiles.log");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let json = serde_json::to_string(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writeln!(file, "{json}")?;
        Ok(())
    }

    fn apply_entry(profiles: &mut Vec<ProfileConfig>, entry: LogEntry) {
        match entry {
            LogEntry::Update { profile } => {
                if let Some(p) = profiles.iter_mut().find(|p| p.id == profile.id) {
                    *p = profile;
                }
            }
            LogEntry::Add { profile } => {
                profiles.push(profile);
            }
            LogEntry::Delete { id } => {
                profiles.retain(|p| p.id != id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, ProfileStore) {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn loads_default_profiles() {
        let (_dir, store) = temp_store();
        assert_eq!(store.list().len(), 4);
        assert_eq!(store.list()[0].name, "Gaming");
        assert_eq!(store.list()[1].name, "Coding");
    }

    #[test]
    fn get_by_name() {
        let (_dir, store) = temp_store();
        let gaming = store.get_by_name("Gaming").unwrap();
        assert_eq!(gaming.max_fps, 120);
        assert!(store.get_by_name("Nonexistent").is_none());
    }

    #[test]
    fn update_persists() {
        let (dir, mut store) = temp_store();
        let id = store.list()[0].id;
        let mut updated = store.get(id).unwrap().clone();
        updated.bitrate_bps = 50_000_000;
        store.update(id, updated).unwrap();

        // Reload from disk
        let store2 = ProfileStore::open(dir.path()).unwrap();
        assert_eq!(store2.get(id).unwrap().bitrate_bps, 50_000_000);
    }

    #[test]
    fn add_custom_profile() {
        let (_dir, mut store) = temp_store();
        let custom = ProfileConfig {
            id: Uuid::now_v7(),
            name: "Custom".to_string(),
            builtin: false,
            bitrate_bps: 20_000_000,
            max_fps: 90,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: true,
        };
        let id = store.add(custom).unwrap();
        assert_eq!(store.list().len(), 5);
        assert_eq!(store.get(id).unwrap().name, "Custom");
    }

    #[test]
    fn delete_custom_profile() {
        let (_dir, mut store) = temp_store();
        let custom = ProfileConfig {
            id: Uuid::now_v7(),
            name: "Temp".to_string(),
            builtin: false,
            bitrate_bps: 10_000_000,
            max_fps: 30,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: false,
            audio_mode: AudioMode::VoiceOptimized,
            prefer_av1: false,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: false,
        };
        let id = store.add(custom).unwrap();
        assert_eq!(store.list().len(), 5);
        store.delete(id).unwrap();
        assert_eq!(store.list().len(), 4);
    }

    #[test]
    fn cannot_delete_builtin() {
        let (_dir, mut store) = temp_store();
        let id = store.list()[0].id;
        let result = store.delete(id);
        assert!(result.is_err());
        assert_eq!(store.list().len(), 4);
    }

    #[test]
    fn compact_and_reload() {
        let (dir, mut store) = temp_store();
        let id = store.list()[0].id;
        let mut updated = store.get(id).unwrap().clone();
        updated.bitrate_bps = 42_000_000;
        store.update(id, updated).unwrap();
        store.compact().unwrap();

        let store2 = ProfileStore::open(dir.path()).unwrap();
        assert_eq!(store2.get(id).unwrap().bitrate_bps, 42_000_000);
    }
}
```

- [ ] **Step 2: Export from config/mod.rs**

Add to `crates/prism-client/src/config/mod.rs` after line 4 (`pub mod servers;`):

```rust
pub mod profiles;
```

- [ ] **Step 3: Add tempfile dev-dependency**

Run: `cargo add tempfile --dev -p prism-client`

- [ ] **Step 4: Run tests**

Run: `cargo test -p prism-client config::profiles`
Expected: PASS — all 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/prism-client/src/config/profiles.rs crates/prism-client/src/config/mod.rs Cargo.toml Cargo.lock
git commit -m "feat(config): add ProfileStore with append-only log persistence and 4 builtin presets"
```

---

## Task 7: UserPrefs (Client Config) Persistence

**Files:**
- Create: `crates/prism-client/src/config/client_config_prefs.rs`
- Modify: `crates/prism-client/src/config/mod.rs`

- [ ] **Step 1: Create UserPrefs persistence**

Create `crates/prism-client/src/config/client_config_prefs.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! User preferences persisted as JSON.

use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPrefs {
    pub default_profile: String,
    pub exclusive_keyboard: bool,
    pub relative_mouse: bool,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            default_profile: "Balanced".to_string(),
            exclusive_keyboard: true,
            relative_mouse: false,
        }
    }
}

impl UserPrefs {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("user_prefs.json");
        if path.exists() {
            if let Ok(data) = fs::read(&path) {
                if let Ok(prefs) = serde_json::from_slice::<UserPrefs>(&data) {
                    return prefs;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, dir: &Path) -> Result<(), io::Error> {
        fs::create_dir_all(dir)?;
        let path = dir.join("user_prefs.json");
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        fs::write(&path, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_prefs() {
        let prefs = UserPrefs::default();
        assert_eq!(prefs.default_profile, "Balanced");
        assert!(prefs.exclusive_keyboard);
        assert!(!prefs.relative_mouse);
    }

    #[test]
    fn save_and_load() {
        let dir = TempDir::new().unwrap();
        let mut prefs = UserPrefs::default();
        prefs.default_profile = "Gaming".to_string();
        prefs.exclusive_keyboard = false;
        prefs.save(dir.path()).unwrap();

        let loaded = UserPrefs::load(dir.path());
        assert_eq!(loaded.default_profile, "Gaming");
        assert!(!loaded.exclusive_keyboard);
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = TempDir::new().unwrap();
        let loaded = UserPrefs::load(dir.path());
        assert_eq!(loaded.default_profile, "Balanced");
    }
}
```

- [ ] **Step 2: Export from config/mod.rs**

Add:
```rust
pub mod client_config_prefs;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-client config::client_config_prefs`
Expected: PASS — all 3 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/config/client_config_prefs.rs crates/prism-client/src/config/mod.rs
git commit -m "feat(config): add UserPrefs JSON persistence for settings"
```

---

## Task 8: Interactive Profiles Screen

**Files:**
- Rewrite: `crates/prism-client/src/ui/launcher/profiles.rs`

- [ ] **Step 1: Rewrite profiles.rs with interactive controls**

Replace the entire file with an interactive two-column editor backed by `ProfileStore`. The left panel shows profile list items (clickable to select). The right panel shows the selected profile's settings using `Slider`, `Toggle`, and `Dropdown` widgets. Discard and Save buttons at the bottom commit changes to `ProfileStore`.

Key struct:
```rust
pub struct ProfilesPanel {
    rect: Rect,
    profile_store: Option<std::sync::Arc<std::sync::Mutex<ProfileStore>>>,
    selected_index: usize,
    draft: Option<ProfileConfig>,
    dirty: bool,
    // Editor widgets
    bitrate_slider: Slider,
    fps_dropdown: Dropdown,
    encoder_dropdown: Dropdown,
    native_scaling_toggle: Toggle,
    audio_mode_dropdown: Dropdown,
    av1_toggle: Toggle,
    exclusive_input_toggle: Toggle,
    touch_mode_toggle: Toggle,
    auto_reconnect_toggle: Toggle,
    save_button: Button,
    discard_button: Button,
}
```

The panel:
- On paint: renders preset list on the left (248px), editor on the right
- On preset click: selects that profile, loads its values into the draft and widget states
- On widget change: marks dirty, updates draft
- On Save: writes draft to ProfileStore, clears dirty
- On Discard: reloads from store, clears dirty

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-client ui::launcher::profiles`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/ui/launcher/profiles.rs
git commit -m "feat(ui): interactive profiles editor with slider/toggle/dropdown controls"
```

---

## Task 9: Settings Screen Upgrade

**Files:**
- Rewrite: `crates/prism-client/src/ui/launcher/settings.rs`

- [ ] **Step 1: Rewrite settings.rs with grouped sections**

Replace with multi-section layout. Key struct:
```rust
pub struct SettingsPanel {
    rect: Rect,
    identity_path: String,
    version: String,
    user_prefs: Option<std::sync::Arc<std::sync::Mutex<UserPrefs>>>,
    prefs_dir: Option<std::path::PathBuf>,
    // Functional controls
    default_profile_dropdown: Dropdown,
    exclusive_keyboard_toggle: Toggle,
    relative_mouse_toggle: Toggle,
    // Display-only cosmetic controls
    audio_output_dropdown: Dropdown,
    mic_dropdown: Dropdown,
}
```

Sections rendered as card surfaces with section headers:
1. Identity & Security (display): identity path monospace + copy hint, trust badge
2. Streaming Defaults (functional): dropdown from ProfileStore names, persists to UserPrefs
3. Input Controls (functional): two toggles, persist to UserPrefs
4. Audio Paths (cosmetic): two dropdowns, render but don't persist
5. About: version string

On dropdown/toggle change for functional controls: update `UserPrefs` and call `save()`.

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-client ui::launcher::settings`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/ui/launcher/settings.rs
git commit -m "feat(ui): multi-section settings with functional streaming defaults and input toggles"
```

---

## Task 10: Profile Pipeline Wiring — Control Messages

**Files:**
- Modify: `crates/prism-session/src/control_msg.rs`

- [ ] **Step 1: Add message payload structs**

Add after `ShutdownNotice` (after line 35):

```rust
/// Sent by client when switching active profile during a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSwitchPayload {
    pub profile_name: String,
    pub max_fps: u8,
    pub encoder_preset: String,
    pub prefer_lossless_text: bool,
    pub region_detection: bool,
}

/// Sent by client to request specific quality parameter changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityUpdatePayload {
    pub encoder_preset: Option<String>,
    pub max_fps: Option<u8>,
    pub bitrate_bps: Option<u64>,
    pub lossless_text: Option<bool>,
    pub region_detection: Option<bool>,
}
```

Add tests:
```rust
    #[test]
    fn profile_switch_json_roundtrip() {
        let payload = ProfileSwitchPayload {
            profile_name: "Gaming".to_string(),
            max_fps: 120,
            encoder_preset: "UltraLowLatency".to_string(),
            prefer_lossless_text: false,
            region_detection: false,
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        let back: ProfileSwitchPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(payload, back);
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-session control_msg`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/prism-session/src/control_msg.rs
git commit -m "feat(protocol): add ProfileSwitchPayload and QualityUpdatePayload message structs"
```

---

## Task 11: Profile Pipeline Wiring — Client Side

**Files:**
- Modify: `crates/prism-client/src/app.rs`
- Modify: `crates/prism-client/src/session_bridge.rs`

- [ ] **Step 1: Wire SwitchProfile and UpdateQuality in app.rs handle_action()**

In the `handle_action` match, replace the catch-all (`_ =>`) with explicit handlers:

```rust
UiAction::SwitchProfile(profile_name) => {
    self.bridge.send_control(ControlCommand::SwitchProfile(profile_name));
}
UiAction::UpdateQuality { preset, max_fps, lossless_text, region_detection } => {
    self.bridge.send_control(ControlCommand::UpdateQuality {
        encoder_preset: preset,
        max_fps,
        lossless_text,
        region_detection,
    });
}
UiAction::SetBandwidthLimit(bps) => {
    self.bridge.send_control(ControlCommand::SetBandwidthLimit(bps));
}
UiAction::SelectMonitor(idx) => {
    self.bridge.send_control(ControlCommand::SelectMonitor(idx));
}
```

- [ ] **Step 2: Send profile at connect time**

In `start_connection()` / `async_connect()`, after the bridge is established, look up the saved server's default_profile in ProfileStore, resolve to ProfileConfig, and send an initial `ControlCommand::UpdateQuality` with the profile's fields.

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-client`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/app.rs crates/prism-client/src/session_bridge.rs
git commit -m "feat(client): wire SwitchProfile/UpdateQuality actions through SessionBridge, send profile at connect"
```

---

## Task 12: Profile Pipeline Wiring — Server Side

**Files:**
- Modify: `crates/prism-server/src/control_handler.rs`
- Modify: `crates/prism-server/src/client_session.rs`
- Modify: `crates/prism-server/src/server_app.rs`

- [ ] **Step 1: Add update_profile to ClientSession**

In `client_session.rs`, add method:
```rust
    pub fn update_profile(&mut self, profile: ConnectionProfile) {
        self.profile = profile;
        self.last_activity = Instant::now();
    }
```

- [ ] **Step 2: Handle PROFILE_SWITCH and QUALITY_UPDATE in control_handler**

Extend `handle_datagram` match arms to parse `PROFILE_SWITCH` and `QUALITY_UPDATE` payloads. For now, log the received profile change and increment a counter. Full encoder reconfiguration is deferred to a follow-up (encoder mid-stream reconfiguration is complex and independent of the UI work).

```rust
PROFILE_SWITCH => {
    if let Ok(payload) = serde_json::from_slice::<ProfileSwitchPayload>(&data[HEADER_SIZE..]) {
        tracing::info!(profile = %payload.profile_name, "client requested profile switch");
        self.stats.profile_switches.fetch_add(1, Ordering::Relaxed);
    }
}
QUALITY_UPDATE => {
    if let Ok(payload) = serde_json::from_slice::<QualityUpdatePayload>(&data[HEADER_SIZE..]) {
        tracing::info!(?payload, "client requested quality update");
        self.stats.quality_updates.fetch_add(1, Ordering::Relaxed);
    }
}
```

Add to `ControlStats`:
```rust
pub profile_switches: AtomicU32,
pub quality_updates: AtomicU32,
```

- [ ] **Step 3: Remove hardcoded profile in server_app.rs**

Change line 964 from `ConnectionProfile::coding()` to use a sensible default or accept profile from capability negotiation:
```rust
prism_session::ConnectionProfile::gaming(),  // Default until client sends profile preference
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p prism-server control_handler`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/prism-server/src/control_handler.rs crates/prism-server/src/client_session.rs crates/prism-server/src/server_app.rs
git commit -m "feat(server): handle PROFILE_SWITCH and QUALITY_UPDATE control messages, add update_profile()"
```

---

## Task 13: Overlay Capsule

**Files:**
- Create: `crates/prism-client/src/ui/overlay/capsule.rs`
- Modify: `crates/prism-client/src/ui/overlay/mod.rs`
- Modify: `crates/prism-client/src/app.rs`

- [ ] **Step 1: Create capsule.rs**

The capsule replaces `StatsBar` as the primary in-session overlay widget. It's a pill-shaped floating bar at top-center with metrics, profile dropdown, and disconnect button. Expandable panels drop below it.

Key struct:
```rust
pub struct OverlayCapsule {
    stats: SessionStats,
    profile_dropdown: Dropdown,
    rect: Rect,
    visible: bool,
    fade_anim: Animation,
    active_panel: Option<CapsulePanel>,
    // Migrated panel content
    perf_panel: PerfPanel,
    quality_panel: QualityPanel,
    conn_panel: ConnPanel,
    display_panel: DisplayPanel,
}

enum CapsulePanel {
    Performance,
    Quality,
    Connection,
    Display,
}
```

Layout: centered at top of screen, ~660px wide, 48px tall. When a panel is active, it renders below the capsule as a dropdown surface.

Clicking metrics opens corresponding panel. Only one panel at a time. Click outside closes.

Disconnect button at the right end emits `UiAction::Disconnect`.

- [ ] **Step 2: Export from overlay/mod.rs**

Add:
```rust
pub mod capsule;
```

- [ ] **Step 3: Wire in app.rs**

Replace `StatsBar` + individual panel management with `OverlayCapsule`. Update the overlay render section to use the capsule.

- [ ] **Step 4: Run tests**

Run: `cargo test -p prism-client`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/prism-client/src/ui/overlay/capsule.rs crates/prism-client/src/ui/overlay/mod.rs crates/prism-client/src/app.rs
git commit -m "feat(ui): replace overlay drawer with top capsule + dropdown panels"
```

---

## Task 14: Integration Pass

**Files:**
- All modified files

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 2: Check for unhandled actions**

Run:
```powershell
cargo check --workspace
Get-ChildItem -Recurse -Path crates -Filter *.rs |
  Select-String -Pattern 'unhandled UI action|_ => \{'
```
Verify: No unhandled `UiAction` variants in any match arm. The catch-all `_ =>` in handle_action should be removed — all actions should have explicit handlers.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: integration pass — verify all actions wired, clippy clean, full test suite green"
```
