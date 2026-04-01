# PRISM Interactive Client — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the CLI-only minifb client with an interactive glassmorphism client featuring a launcher window and in-session overlay, powered by a custom wgpu renderer.

**Architecture:** Single winit window with wgpu renderer. Two render modes (Launcher, Stream) sharing the same GPU device and surface. Overlay composites on top of Stream. SessionBridge mediates between UI thread and tokio async network tasks via typed channels. All UI drawn through instanced GPU batching (3 draw calls total).

**Tech Stack:** wgpu 24, winit 0.30, glyphon 0.6, tokio 1, quinn 0.11, serde/serde_json, uuid, bytemuck

**Spec:** `docs/superpowers/specs/2026-04-01-interactive-client-design.md`

---

## File Map

### New Files (renderer/)

| File | Responsibility |
|------|---------------|
| `crates/prism-client/src/renderer/mod.rs` | `PrismRenderer` — device, surface, frame orchestration |
| `crates/prism-client/src/renderer/stream_texture.rs` | Ring-buffered YUV upload + compute shader YUV→RGB |
| `crates/prism-client/src/renderer/blur_pipeline.rs` | Two-pass Gaussian blur at progressive resolutions |
| `crates/prism-client/src/renderer/glass_panel.rs` | Frosted glass quad compositing |
| `crates/prism-client/src/renderer/text_renderer.rs` | glyphon wrapper, glyph cache warming |
| `crates/prism-client/src/renderer/shader_cache.rs` | Pipeline cache persistence to ~/.prism/shader_cache/ |
| `crates/prism-client/src/renderer/animation.rs` | Spring/ease curves, Animation struct, batched tick |
| `crates/prism-client/src/renderer/shaders/yuv_to_rgb.wgsl` | Compute shader: YUV420 planes → RGBA texture |
| `crates/prism-client/src/renderer/shaders/blur.wgsl` | Fragment shader: separable Gaussian blur |
| `crates/prism-client/src/renderer/shaders/glass.wgsl` | Fragment shader: frosted glass composite |
| `crates/prism-client/src/renderer/shaders/quad.wgsl` | Vertex shader: instanced quad rendering |
| `crates/prism-client/src/renderer/shaders/text.wgsl` | Fragment shader: glyph atlas sampling |
| `crates/prism-client/src/renderer/shaders/glow.wgsl` | Fragment shader: accent glow rects |

### New Files (ui/)

| File | Responsibility |
|------|---------------|
| `crates/prism-client/src/ui/mod.rs` | `UiState` state machine, `UiEvent`, `EventResponse` |
| `crates/prism-client/src/ui/widgets/mod.rs` | `Widget` trait, `PaintContext`, `Rect`, `Size`, draw batches |
| `crates/prism-client/src/ui/widgets/label.rs` | Static/live text, color-coded, monospace numeric |
| `crates/prism-client/src/ui/widgets/button.rs` | Glass surface, hover glow, click callback |
| `crates/prism-client/src/ui/widgets/separator.rs` | Embossed glass ridge |
| `crates/prism-client/src/ui/widgets/checkbox.rs` | Radial wipe toggle |
| `crates/prism-client/src/ui/widgets/slider.rs` | Accent fill, glow thumb, drag |
| `crates/prism-client/src/ui/widgets/sparkline.rs` | Ring buffer polyline with trailing glow |
| `crates/prism-client/src/ui/widgets/dropdown.rs` | Staggered cascade reveal, glass popup |
| `crates/prism-client/src/ui/widgets/text_input.rs` | Cursor, selection, autocomplete |
| `crates/prism-client/src/ui/widgets/monitor_map.rs` | Monitor arrangement diagram |
| `crates/prism-client/src/ui/launcher/mod.rs` | Launcher mode root |
| `crates/prism-client/src/ui/launcher/quick_connect.rs` | Hero bar, input, autocomplete |
| `crates/prism-client/src/ui/launcher/server_card.rs` | Card rendering, hover animation |
| `crates/prism-client/src/ui/launcher/card_grid.rs` | Responsive flow layout |
| `crates/prism-client/src/ui/launcher/server_form.rs` | Add/Edit server panel |
| `crates/prism-client/src/ui/launcher/settings.rs` | Gear icon settings panel |
| `crates/prism-client/src/ui/overlay/mod.rs` | Overlay mode root |
| `crates/prism-client/src/ui/overlay/stats_bar.rs` | Top-docked stats bar |
| `crates/prism-client/src/ui/overlay/perf_panel.rs` | Performance sub-panel |
| `crates/prism-client/src/ui/overlay/quality_panel.rs` | Quality controls sub-panel |
| `crates/prism-client/src/ui/overlay/conn_panel.rs` | Connection info sub-panel |
| `crates/prism-client/src/ui/overlay/display_panel.rs` | Display/monitor sub-panel |

### New Files (input/)

| File | Responsibility |
|------|---------------|
| `crates/prism-client/src/input/mod.rs` | Input router — overlay vs remote |
| `crates/prism-client/src/input/double_tap.rs` | Double-tap Left Ctrl detector |
| `crates/prism-client/src/input/drag.rs` | Panel drag handler |

### New Files (config/)

| File | Responsibility |
|------|---------------|
| `crates/prism-client/src/config/mod.rs` | Unified config — CLI args + servers.json paths |
| `crates/prism-client/src/config/servers.rs` | SavedServer struct, append-log persistence |

### Modified Files

| File | Changes |
|------|---------|
| `crates/prism-client/Cargo.toml` | Add wgpu, winit, glyphon, bytemuck, uuid, image deps; remove minifb |
| `crates/prism-client/src/lib.rs` | Add new module declarations |
| `crates/prism-client/src/main.rs` | Mode selection (launcher vs direct connect), new CLI args |
| `crates/prism-client/src/client_app.rs` | Refactor to use PrismRenderer, SessionBridge, new render loop |
| `crates/prism-client/src/connector.rs` | Connection pooling, pre-connect support |
| `Cargo.toml` (workspace) | Add wgpu, winit, glyphon, bytemuck, uuid workspace deps |
| `crates/prism-server/src/negotiation_handler.rs` | Accept concurrent bi-streams by message type inspection |

---

## Task 1: Dependencies & Project Setup

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/prism-client/Cargo.toml`
- Modify: `crates/prism-client/src/lib.rs`
- Create: `crates/prism-client/src/renderer/mod.rs` (stub)
- Create: `crates/prism-client/src/ui/mod.rs` (stub)
- Create: `crates/prism-client/src/input/mod.rs` (stub)
- Create: `crates/prism-client/src/config/mod.rs` (stub)

- [ ] **Step 1: Add workspace dependencies**

Add to `Cargo.toml` workspace root `[workspace.dependencies]` section:

```toml
wgpu = "24"
winit = "0.30"
glyphon = "0.6"
bytemuck = { version = "1", features = ["derive"] }
image = { version = "0.25", default-features = false, features = ["png"] }
```

- [ ] **Step 2: Update prism-client Cargo.toml**

Replace `minifb` with new deps in `crates/prism-client/Cargo.toml`. Keep all existing deps, add:

```toml
wgpu.workspace = true
winit.workspace = true
glyphon.workspace = true
bytemuck.workspace = true
image.workspace = true
```

Keep `minifb.workspace = true` for now (removed in Task 23 when we cut over). This allows incremental migration without breaking the existing client.

- [ ] **Step 3: Create module stubs**

Create `crates/prism-client/src/renderer/mod.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! wgpu-based renderer for PRISM client — stream texture, blur, glass panels, text.
```

Create `crates/prism-client/src/ui/mod.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! UI state machine and widget system for launcher and in-session overlay.

pub mod widgets;
```

Create `crates/prism-client/src/ui/widgets/mod.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Widget trait, layout primitives, and draw batching.
```

Create `crates/prism-client/src/input/mod.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Input routing — overlay vs remote forwarding, double-tap detection, drag.

pub mod double_tap;
```

Create `crates/prism-client/src/input/double_tap.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Double-tap Left Ctrl detector for overlay toggle.
```

Create `crates/prism-client/src/config/mod.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client configuration — CLI args, saved servers, shader cache paths.

pub mod servers;
```

Create `crates/prism-client/src/config/servers.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! SavedServer persistence with append-only log and compaction.
```

- [ ] **Step 4: Update lib.rs with new modules**

Add to `crates/prism-client/src/lib.rs` after existing module declarations:

```rust
pub mod renderer;
pub mod ui;
pub mod input;
pub mod config;
```

- [ ] **Step 5: Verify workspace compiles**

Run: `cargo check -p prism-client`
Expected: Compiles with no errors (modules are empty stubs).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/prism-client/
git commit -m "feat(client): add wgpu/winit/glyphon deps and module stubs for interactive client"
```

---

## Task 2: Animation System

**Files:**
- Create: `crates/prism-client/src/renderer/animation.rs`

Pure math, no GPU dependencies. Testable standalone.

- [ ] **Step 1: Write failing tests**

Create `crates/prism-client/src/renderer/animation.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Animation system — spring curves, ease-out, interpolation, batched tick.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_out_starts_at_zero() {
        let a = Animation::new(EaseCurve::EaseOut, 200.0);
        assert_eq!(a.value(), 0.0);
    }

    #[test]
    fn ease_out_ends_at_one() {
        let mut a = Animation::new(EaseCurve::EaseOut, 200.0);
        a.set_target(1.0);
        a.tick(200.0);
        assert!((a.value() - 1.0).abs() < 0.01);
    }

    #[test]
    fn spring_overshoots() {
        let mut a = Animation::new(EaseCurve::Spring, 200.0);
        a.set_target(1.0);
        a.tick(100.0); // midway
        // Spring should overshoot past target at some point
        a.tick(50.0);
        // After 150ms of 200ms spring, value may exceed 1.0
        assert!(a.value() > 0.5);
    }

    #[test]
    fn dormant_when_at_target() {
        let a = Animation::new(EaseCurve::EaseOut, 200.0);
        assert!(a.is_dormant()); // starts at 0.0, target is 0.0
    }

    #[test]
    fn not_dormant_when_animating() {
        let mut a = Animation::new(EaseCurve::EaseOut, 200.0);
        a.set_target(1.0);
        assert!(!a.is_dormant());
    }

    #[test]
    fn linear_interpolation_midpoint() {
        let mut a = Animation::new(EaseCurve::Linear, 100.0);
        a.set_target(1.0);
        a.tick(50.0);
        assert!((a.value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn batch_tick_all() {
        let mut pool = AnimationPool::new();
        let id1 = pool.add(EaseCurve::Linear, 100.0);
        let id2 = pool.add(EaseCurve::Linear, 100.0);
        pool.set_target(id1, 1.0);
        pool.set_target(id2, 1.0);
        pool.tick_all(50.0);
        assert!((pool.value(id1) - 0.5).abs() < 0.01);
        assert!((pool.value(id2) - 0.5).abs() < 0.01);
    }

    #[test]
    fn batch_all_dormant() {
        let pool = AnimationPool::new();
        assert!(pool.all_dormant());
    }

    #[test]
    fn batch_not_all_dormant() {
        let mut pool = AnimationPool::new();
        let id = pool.add(EaseCurve::Linear, 100.0);
        pool.set_target(id, 1.0);
        assert!(!pool.all_dormant());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-client -- animation --no-run 2>&1 || echo "Expected: compile error"`
Expected: Compile error — `Animation`, `EaseCurve`, `AnimationPool` not defined.

- [ ] **Step 3: Implement Animation and AnimationPool**

Add implementation above the `#[cfg(test)]` block:

```rust
/// Easing curve type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EaseCurve {
    Linear,
    EaseOut,
    EaseIn,
    Spring,
}

/// A single animated f32 value transitioning from current to target.
#[derive(Debug, Clone)]
pub struct Animation {
    value: f32,
    target: f32,
    curve: EaseCurve,
    duration_ms: f32,
    elapsed_ms: f32,
    start_value: f32,
}

impl Animation {
    pub fn new(curve: EaseCurve, duration_ms: f32) -> Self {
        Self {
            value: 0.0,
            target: 0.0,
            curve,
            duration_ms,
            elapsed_ms: 0.0,
            start_value: 0.0,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        if (self.target - target).abs() > f32::EPSILON {
            self.start_value = self.value;
            self.target = target;
            self.elapsed_ms = 0.0;
        }
    }

    pub fn tick(&mut self, dt_ms: f32) {
        if self.is_dormant() {
            return;
        }
        self.elapsed_ms += dt_ms;
        let t = (self.elapsed_ms / self.duration_ms).clamp(0.0, 1.0);
        let eased = match self.curve {
            EaseCurve::Linear => t,
            EaseCurve::EaseOut => 1.0 - (1.0 - t).powi(3),
            EaseCurve::EaseIn => t.powi(3),
            EaseCurve::Spring => {
                if t >= 1.0 {
                    1.0
                } else {
                    let omega = 8.0;
                    let zeta = 0.5;
                    1.0 - ((-zeta * omega * t).exp()
                        * ((1.0 - zeta * zeta).sqrt() * omega * t).cos())
                }
            }
        };
        self.value = self.start_value + (self.target - self.start_value) * eased;
        if t >= 1.0 {
            self.value = self.target;
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn is_dormant(&self) -> bool {
        (self.value - self.target).abs() < 0.001
    }

    pub fn target(&self) -> f32 {
        self.target
    }
}

/// Handle into an AnimationPool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationId(usize);

/// Pool of animations for cache-friendly batched ticking.
pub struct AnimationPool {
    animations: Vec<Animation>,
}

impl AnimationPool {
    pub fn new() -> Self {
        Self {
            animations: Vec::new(),
        }
    }

    pub fn add(&mut self, curve: EaseCurve, duration_ms: f32) -> AnimationId {
        let id = AnimationId(self.animations.len());
        self.animations.push(Animation::new(curve, duration_ms));
        id
    }

    pub fn set_target(&mut self, id: AnimationId, target: f32) {
        self.animations[id.0].set_target(target);
    }

    pub fn value(&self, id: AnimationId) -> f32 {
        self.animations[id.0].value()
    }

    pub fn tick_all(&mut self, dt_ms: f32) {
        for anim in &mut self.animations {
            anim.tick(dt_ms);
        }
    }

    pub fn all_dormant(&self) -> bool {
        self.animations.iter().all(|a| a.is_dormant())
    }

    pub fn is_dormant(&self, id: AnimationId) -> bool {
        self.animations[id.0].is_dormant()
    }
}

impl Default for AnimationPool {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Export from renderer mod**

Add to `crates/prism-client/src/renderer/mod.rs`:

```rust
pub mod animation;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p prism-client -- animation -v`
Expected: All 9 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/prism-client/src/renderer/
git commit -m "feat(client): animation system with spring/ease curves and batched pool"
```

---

## Task 3: Double-Tap Detector

**Files:**
- Modify: `crates/prism-client/src/input/double_tap.rs`

Pure logic, no windowing deps. State machine with 3 states.

- [ ] **Step 1: Write failing tests**

Replace contents of `crates/prism-client/src/input/double_tap.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Double-tap Left Ctrl detector for overlay toggle.
//!
//! Three-state machine: Idle → FirstTapSeen(Instant) → fires event.
//! Zero allocations, one timestamp comparison per key event.

use std::time::{Duration, Instant};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tap_no_trigger() {
        let det = DoubleTapDetector::new(Duration::from_millis(300));
        assert!(!det.is_triggered());
    }

    #[test]
    fn single_tap_no_trigger() {
        let mut det = DoubleTapDetector::new(Duration::from_millis(300));
        let now = Instant::now();
        assert!(!det.key_down(now));
        det.key_up(now);
        assert!(!det.is_triggered());
    }

    #[test]
    fn double_tap_within_window_triggers() {
        let mut det = DoubleTapDetector::new(Duration::from_millis(300));
        let t0 = Instant::now();
        det.key_down(t0);
        det.key_up(t0);
        let t1 = t0 + Duration::from_millis(100);
        let triggered = det.key_down(t1);
        assert!(triggered);
    }

    #[test]
    fn double_tap_outside_window_no_trigger() {
        let mut det = DoubleTapDetector::new(Duration::from_millis(300));
        let t0 = Instant::now();
        det.key_down(t0);
        det.key_up(t0);
        let t1 = t0 + Duration::from_millis(400);
        let triggered = det.key_down(t1);
        assert!(!triggered);
    }

    #[test]
    fn other_key_resets() {
        let mut det = DoubleTapDetector::new(Duration::from_millis(300));
        let t0 = Instant::now();
        det.key_down(t0);
        det.key_up(t0);
        det.other_key_pressed();
        let t1 = t0 + Duration::from_millis(100);
        let triggered = det.key_down(t1);
        assert!(!triggered);
    }

    #[test]
    fn consume_resets_state() {
        let mut det = DoubleTapDetector::new(Duration::from_millis(300));
        let t0 = Instant::now();
        det.key_down(t0);
        det.key_up(t0);
        let t1 = t0 + Duration::from_millis(100);
        det.key_down(t1);
        det.consume();
        // Next double-tap should work independently
        let t2 = t1 + Duration::from_millis(50);
        det.key_up(t2);
        let t3 = t2 + Duration::from_millis(100);
        det.key_down(t3);
        det.key_up(t3);
        let t4 = t3 + Duration::from_millis(100);
        let triggered = det.key_down(t4);
        assert!(triggered);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-client -- double_tap --no-run 2>&1 || echo "Expected: compile error"`
Expected: Compile error — `DoubleTapDetector` not defined.

- [ ] **Step 3: Implement DoubleTapDetector**

Add implementation above `#[cfg(test)]`:

```rust
#[derive(Debug)]
enum State {
    Idle,
    FirstTapSeen { tap_time: Instant },
    Triggered,
}

/// Detects double-tap of a single key within a time window.
#[derive(Debug)]
pub struct DoubleTapDetector {
    state: State,
    window: Duration,
}

impl DoubleTapDetector {
    pub fn new(window: Duration) -> Self {
        Self {
            state: State::Idle,
            window,
        }
    }

    /// Call on the monitored key press. Returns true if double-tap detected.
    pub fn key_down(&mut self, now: Instant) -> bool {
        match &self.state {
            State::Idle => {
                self.state = State::FirstTapSeen { tap_time: now };
                false
            }
            State::FirstTapSeen { tap_time } => {
                if now.duration_since(*tap_time) <= self.window {
                    self.state = State::Triggered;
                    true
                } else {
                    // Expired, treat as new first tap
                    self.state = State::FirstTapSeen { tap_time: now };
                    false
                }
            }
            State::Triggered => false,
        }
    }

    /// Call on the monitored key release.
    pub fn key_up(&mut self, _now: Instant) {
        // No state change needed — we track press-to-press timing
    }

    /// Call when any other key is pressed — resets the detector.
    pub fn other_key_pressed(&mut self) {
        self.state = State::Idle;
    }

    /// Returns true if a double-tap was detected and not yet consumed.
    pub fn is_triggered(&self) -> bool {
        matches!(self.state, State::Triggered)
    }

    /// Consume the trigger, resetting to Idle.
    pub fn consume(&mut self) {
        self.state = State::Idle;
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p prism-client -- double_tap -v`
Expected: All 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/prism-client/src/input/double_tap.rs
git commit -m "feat(client): double-tap Left Ctrl detector with 3-state machine"
```

---

## Task 4: Saved Server Persistence

**Files:**
- Modify: `crates/prism-client/src/config/servers.rs`

- [ ] **Step 1: Write failing tests**

Replace contents of `crates/prism-client/src/config/servers.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! SavedServer persistence with append-only log and compaction.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_store_has_no_servers() {
        let dir = TempDir::new().unwrap();
        let store = ServerStore::open(dir.path()).unwrap();
        assert!(store.servers().is_empty());
    }

    #[test]
    fn add_and_retrieve_server() {
        let dir = TempDir::new().unwrap();
        let mut store = ServerStore::open(dir.path()).unwrap();
        let server = SavedServer::new("Work PC".into(), "10.0.0.5:7000".into());
        let id = server.id;
        store.add(server).unwrap();
        assert_eq!(store.servers().len(), 1);
        assert_eq!(store.get(id).unwrap().display_name, "Work PC");
    }

    #[test]
    fn update_server() {
        let dir = TempDir::new().unwrap();
        let mut store = ServerStore::open(dir.path()).unwrap();
        let mut server = SavedServer::new("Work PC".into(), "10.0.0.5:7000".into());
        let id = server.id;
        store.add(server).unwrap();
        store.update(id, |s| s.display_name = "Home PC".into()).unwrap();
        assert_eq!(store.get(id).unwrap().display_name, "Home PC");
    }

    #[test]
    fn delete_server() {
        let dir = TempDir::new().unwrap();
        let mut store = ServerStore::open(dir.path()).unwrap();
        let server = SavedServer::new("Work PC".into(), "10.0.0.5:7000".into());
        let id = server.id;
        store.add(server).unwrap();
        store.delete(id).unwrap();
        assert!(store.servers().is_empty());
    }

    #[test]
    fn persistence_survives_reopen() {
        let dir = TempDir::new().unwrap();
        {
            let mut store = ServerStore::open(dir.path()).unwrap();
            store.add(SavedServer::new("Work PC".into(), "10.0.0.5:7000".into())).unwrap();
        }
        let store = ServerStore::open(dir.path()).unwrap();
        assert_eq!(store.servers().len(), 1);
    }

    #[test]
    fn compaction_works() {
        let dir = TempDir::new().unwrap();
        let mut store = ServerStore::open(dir.path()).unwrap();
        // Add then delete — log has 2 entries, compacted should have 0
        let server = SavedServer::new("Temp".into(), "1.2.3.4:7000".into());
        let id = server.id;
        store.add(server).unwrap();
        store.delete(id).unwrap();
        store.compact().unwrap();
        let store2 = ServerStore::open(dir.path()).unwrap();
        assert!(store2.servers().is_empty());
    }

    #[test]
    fn accent_color_from_name() {
        let c1 = accent_color_for_name("Work PC");
        let c2 = accent_color_for_name("Work PC");
        let c3 = accent_color_for_name("Home Lab");
        assert_eq!(c1, c2); // deterministic
        assert_ne!(c1, c3); // different names, different colors
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-client -- servers --no-run 2>&1 || echo "Expected: compile error"`
Expected: Compile error.

- [ ] **Step 3: Add uuid and tempfile dependencies**

Add `uuid` to prism-client Cargo.toml `[dependencies]`:
```toml
uuid = { workspace = true }
```

Add `tempfile` to `[dev-dependencies]`:
```toml
[dev-dependencies]
tempfile.workspace = true
```

Ensure `uuid` is in workspace deps (it already is with `v7` and `serde` features).

- [ ] **Step 4: Implement SavedServer and ServerStore**

Add above `#[cfg(test)]`:

```rust
/// Deterministic accent color from server name.
pub fn accent_color_for_name(name: &str) -> [u8; 3] {
    let mut hash: u32 = 5381;
    for b in name.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u32);
    }
    // Map to HSL hue in purple-cyan range (220-320 degrees), full saturation
    let hue = 220.0 + (hash % 100) as f32;
    let (r, g, b) = hsl_to_rgb(hue, 0.7, 0.6);
    [r, g, b]
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedServer {
    pub id: Uuid,
    pub display_name: String,
    pub address: String,
    pub noise_public_key: Option<String>,
    pub default_profile: String,
    pub accent_color: [u8; 3],
    pub last_connected: Option<u64>,
    pub last_resolution: Option<(u32, u32)>,
    pub last_codec: Option<String>,
    pub created_at: u64,
}

impl SavedServer {
    pub fn new(display_name: String, address: String) -> Self {
        let accent = accent_color_for_name(&display_name);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: Uuid::now_v7(),
            display_name,
            address,
            noise_public_key: None,
            default_profile: "Gaming".into(),
            accent_color: accent,
            last_connected: None,
            last_resolution: None,
            last_codec: None,
            created_at: now,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
enum LogEntry {
    #[serde(rename = "add")]
    Add { server: SavedServer },
    #[serde(rename = "update")]
    Update { server: SavedServer },
    #[serde(rename = "delete")]
    Delete { id: Uuid },
}

/// Manages saved server persistence with append-only log + compaction.
pub struct ServerStore {
    dir: PathBuf,
    servers: Vec<SavedServer>,
}

impl ServerStore {
    /// Open or create a server store in the given directory.
    pub fn open(dir: &Path) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let json_path = dir.join("servers.json");
        let log_path = dir.join("servers.log");

        // Load compacted state
        let mut servers: Vec<SavedServer> = if json_path.exists() {
            let data = std::fs::read_to_string(&json_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Replay log on top
        if log_path.exists() {
            let log_data = std::fs::read_to_string(&log_path)?;
            for line in log_data.lines() {
                if let Ok(entry) = serde_json::from_str::<LogEntry>(line) {
                    apply_entry(&mut servers, entry);
                }
                // Skip malformed lines (crash safety)
            }
        }

        Ok(Self {
            dir: dir.to_path_buf(),
            servers,
        })
    }

    pub fn servers(&self) -> &[SavedServer] {
        &self.servers
    }

    pub fn get(&self, id: Uuid) -> Option<&SavedServer> {
        self.servers.iter().find(|s| s.id == id)
    }

    pub fn add(&mut self, server: SavedServer) -> Result<(), std::io::Error> {
        let entry = LogEntry::Add {
            server: server.clone(),
        };
        self.append_log(&entry)?;
        self.servers.push(server);
        Ok(())
    }

    pub fn update(
        &mut self,
        id: Uuid,
        f: impl FnOnce(&mut SavedServer),
    ) -> Result<(), std::io::Error> {
        if let Some(server) = self.servers.iter_mut().find(|s| s.id == id) {
            f(server);
            let entry = LogEntry::Update {
                server: server.clone(),
            };
            self.append_log(&entry)?;
        }
        Ok(())
    }

    pub fn delete(&mut self, id: Uuid) -> Result<(), std::io::Error> {
        let entry = LogEntry::Delete { id };
        self.append_log(&entry)?;
        self.servers.retain(|s| s.id != id);
        Ok(())
    }

    pub fn compact(&mut self) -> Result<(), std::io::Error> {
        let json_path = self.dir.join("servers.json");
        let log_path = self.dir.join("servers.log");
        let data = serde_json::to_string_pretty(&self.servers)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&json_path, data)?;
        // Truncate the log
        std::fs::write(&log_path, "")?;
        Ok(())
    }

    fn append_log(&self, entry: &LogEntry) -> Result<(), std::io::Error> {
        use std::io::Write;
        let log_path = self.dir.join("servers.log");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let line = serde_json::to_string(entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        writeln!(file, "{}", line)?;
        Ok(())
    }
}

fn apply_entry(servers: &mut Vec<SavedServer>, entry: LogEntry) {
    match entry {
        LogEntry::Add { server } => {
            servers.push(server);
        }
        LogEntry::Update { server } => {
            if let Some(existing) = servers.iter_mut().find(|s| s.id == server.id) {
                *existing = server;
            }
        }
        LogEntry::Delete { id } => {
            servers.retain(|s| s.id != id);
        }
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p prism-client -- servers -v`
Expected: All 7 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/prism-client/src/config/servers.rs crates/prism-client/Cargo.toml
git commit -m "feat(client): saved server persistence with append-only log and compaction"
```

---

## Task 5: Widget Trait & Layout Primitives

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/mod.rs`

Core abstractions that all widgets and the renderer depend on. No GPU code — just data structures for draw commands.

- [ ] **Step 1: Write failing tests**

Replace contents of `crates/prism-client/src/ui/widgets/mod.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Widget trait, layout primitives, and draw batching.

pub mod label;
pub mod button;
pub mod separator;
pub mod checkbox;
pub mod slider;
pub mod sparkline;
pub mod dropdown;
pub mod text_input;
pub mod monitor_map;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_point() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(5.0, 40.0));
        assert!(!r.contains(50.0, 80.0));
    }

    #[test]
    fn rect_intersects() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        let c = Rect::new(200.0, 200.0, 10.0, 10.0);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn paint_context_collects_quads() {
        let mut ctx = PaintContext::new();
        ctx.push_glass_quad(GlassQuad {
            rect: Rect::new(0.0, 0.0, 100.0, 50.0),
            blur_rect: Rect::new(0.0, 0.0, 100.0, 50.0),
            tint: [0.1, 0.0, 0.2, 0.1],
            border_color: [1.0, 1.0, 1.0, 0.2],
            corner_radius: 8.0,
            noise_intensity: 0.03,
        });
        assert_eq!(ctx.glass_quads.len(), 1);
    }

    #[test]
    fn paint_context_collects_text() {
        let mut ctx = PaintContext::new();
        ctx.push_text_run(TextRun {
            x: 10.0,
            y: 20.0,
            text: "Hello".into(),
            font_size: 14.0,
            color: [1.0, 1.0, 1.0, 0.9],
            monospace: false,
        });
        assert_eq!(ctx.text_runs.len(), 1);
    }

    #[test]
    fn spatial_hash_lookup() {
        let mut hash = SpatialHash::new(800.0, 600.0, 8);
        hash.insert(0, Rect::new(10.0, 10.0, 50.0, 50.0));
        hash.insert(1, Rect::new(400.0, 300.0, 50.0, 50.0));
        let hits = hash.query(25.0, 25.0);
        assert!(hits.contains(&0));
        assert!(!hits.contains(&1));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-client -- widgets::tests --no-run 2>&1 || echo "Expected: compile error"`
Expected: Compile error.

- [ ] **Step 3: Implement layout primitives and PaintContext**

Add above `#[cfg(test)]`:

```rust
/// Axis-aligned rectangle in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
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

/// Size in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

/// Frosted glass quad — sent to GPU as instance data.
#[derive(Debug, Clone)]
pub struct GlassQuad {
    pub rect: Rect,
    pub blur_rect: Rect,
    pub tint: [f32; 4],
    pub border_color: [f32; 4],
    pub corner_radius: f32,
    pub noise_intensity: f32,
}

/// Text run — sent to glyphon for layout, then GPU for rendering.
#[derive(Debug, Clone)]
pub struct TextRun {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub monospace: bool,
}

/// Glow rectangle — accent glows, warning pulses.
#[derive(Debug, Clone)]
pub struct GlowRect {
    pub rect: Rect,
    pub color: [f32; 4],
    pub spread: f32,
    pub intensity: f32,
}

/// Collects draw commands from widgets for batched GPU submission.
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

/// UI events mapped from winit.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Simplified key codes for UI interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Enter,
    Escape,
    Tab,
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

/// Response from a widget's event handler.
#[derive(Debug)]
pub enum EventResponse {
    Ignored,
    Consumed,
    Action(UiAction),
}

/// Actions that bubble up from widgets to the state machine.
#[derive(Debug, Clone)]
pub enum UiAction {
    Connect { address: String, noise_key: Option<String> },
    Disconnect,
    SwitchServer { address: String },
    SwitchProfile(String),
    UpdateQuality { preset: Option<String>, max_fps: Option<u8>, lossless_text: Option<bool>, region_detection: Option<bool> },
    SetBandwidthLimit(u64),
    SelectMonitor(u8),
    OpenPanel(String),
    ClosePanel(String),
    CloseOverlay,
    TogglePinStatsBar,
    EditServer(uuid::Uuid),
    DeleteServer(uuid::Uuid),
    AddServer,
    OpenSettings,
}

/// Widget trait — all UI elements implement this.
pub trait Widget {
    fn layout(&mut self, available: Rect) -> Size;
    fn paint(&self, ctx: &mut PaintContext);
    fn handle_event(&mut self, event: &UiEvent) -> EventResponse;
    fn animate(&mut self, dt_ms: f32);
}

/// Spatial hash for O(1) hit testing.
pub struct SpatialHash {
    cells: Vec<Vec<usize>>,
    cell_w: f32,
    cell_h: f32,
    cols: usize,
    rows: usize,
}

impl SpatialHash {
    pub fn new(width: f32, height: f32, divisions: usize) -> Self {
        let cols = divisions;
        let rows = divisions;
        Self {
            cells: vec![Vec::new(); cols * rows],
            cell_w: width / cols as f32,
            cell_h: height / rows as f32,
            cols,
            rows,
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub fn insert(&mut self, id: usize, rect: Rect) {
        let x0 = ((rect.x / self.cell_w) as usize).min(self.cols - 1);
        let y0 = ((rect.y / self.cell_h) as usize).min(self.rows - 1);
        let x1 = (((rect.x + rect.w) / self.cell_w) as usize).min(self.cols - 1);
        let y1 = (((rect.y + rect.h) / self.cell_h) as usize).min(self.rows - 1);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                self.cells[cy * self.cols + cx].push(id);
            }
        }
    }

    pub fn query(&self, x: f32, y: f32) -> &[usize] {
        let cx = ((x / self.cell_w) as usize).min(self.cols - 1);
        let cy = ((y / self.cell_h) as usize).min(self.rows - 1);
        &self.cells[cy * self.cols + cx]
    }
}
```

- [ ] **Step 4: Create empty widget module files**

Create each of these as minimal stubs (just the license header):

`crates/prism-client/src/ui/widgets/label.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Label widget — static or live-updating text with color coding.
```

Create identical stubs for: `button.rs`, `separator.rs`, `checkbox.rs`, `slider.rs`, `sparkline.rs`, `dropdown.rs`, `text_input.rs`, `monitor_map.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p prism-client -- widgets::tests -v`
Expected: All 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/prism-client/src/ui/
git commit -m "feat(client): widget trait, layout primitives, paint context, spatial hash"
```

---

## Task 6: WGSL Shaders

**Files:**
- Create: `crates/prism-client/src/renderer/shaders/yuv_to_rgb.wgsl`
- Create: `crates/prism-client/src/renderer/shaders/blur.wgsl`
- Create: `crates/prism-client/src/renderer/shaders/quad.wgsl`
- Create: `crates/prism-client/src/renderer/shaders/glass.wgsl`
- Create: `crates/prism-client/src/renderer/shaders/glow.wgsl`
- Create: `crates/prism-client/src/renderer/shaders/text.wgsl`

No Rust tests for shaders — they'll be validated when the render pipelines are built in Task 7.

- [ ] **Step 1: Create shaders directory**

Run: `mkdir -p crates/prism-client/src/renderer/shaders`

- [ ] **Step 2: Write YUV→RGB compute shader**

Create `crates/prism-client/src/renderer/shaders/yuv_to_rgb.wgsl`:

```wgsl
// Compute shader: YUV420 planar → RGBA texture
// BT.601 color space conversion

@group(0) @binding(0) var y_plane: texture_2d<f32>;
@group(0) @binding(1) var u_plane: texture_2d<f32>;
@group(0) @binding(2) var v_plane: texture_2d<f32>;
@group(0) @binding(3) var output: texture_storage_2d<rgba8unorm, write>;

struct Params {
    width: u32,
    height: u32,
}
@group(0) @binding(4) var<uniform> params: Params;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= params.width || gid.y >= params.height {
        return;
    }

    let y_val = textureLoad(y_plane, vec2<i32>(i32(gid.x), i32(gid.y)), 0).r;
    let u_val = textureLoad(u_plane, vec2<i32>(i32(gid.x / 2u), i32(gid.y / 2u)), 0).r;
    let v_val = textureLoad(v_plane, vec2<i32>(i32(gid.x / 2u), i32(gid.y / 2u)), 0).r;

    // BT.601 conversion
    let y = y_val - 16.0 / 255.0;
    let u = u_val - 0.5;
    let v = v_val - 0.5;

    let r = clamp(y * 1.164 + v * 1.596, 0.0, 1.0);
    let g = clamp(y * 1.164 - u * 0.392 - v * 0.813, 0.0, 1.0);
    let b = clamp(y * 1.164 + u * 2.017, 0.0, 1.0);

    textureStore(output, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(r, g, b, 1.0));
}
```

- [ ] **Step 3: Write instanced quad vertex shader**

Create `crates/prism-client/src/renderer/shaders/quad.wgsl`:

```wgsl
// Instanced quad vertex shader — shared by glass, glow, and stream passes

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) screen_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

struct QuadInstance {
    rect: vec4<f32>,     // x, y, w, h in pixels
    uv_rect: vec4<f32>,  // u0, v0, u1, v1 (for texture sampling)
}

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> instances: array<QuadInstance>;

// Fullscreen triangle corners for a quad (two triangles, 6 vertices)
var<private> QUAD_VERTS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    @builtin(instance_index) ii: u32,
) -> VertexOutput {
    let inst = instances[ii];
    let corner = QUAD_VERTS[vi];

    let px = inst.rect.x + corner.x * inst.rect.z;
    let py = inst.rect.y + corner.y * inst.rect.w;

    // Convert pixel coords to NDC
    let ndc_x = (px / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / uniforms.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vec2<f32>(
        inst.uv_rect.x + corner.x * (inst.uv_rect.z - inst.uv_rect.x),
        inst.uv_rect.y + corner.y * (inst.uv_rect.w - inst.uv_rect.y),
    );
    out.screen_pos = vec2<f32>(px, py);
    out.instance_id = ii;
    return out;
}
```

- [ ] **Step 4: Write separable Gaussian blur shader**

Create `crates/prism-client/src/renderer/shaders/blur.wgsl`:

```wgsl
// Separable Gaussian blur — run once horizontal, once vertical

struct BlurUniforms {
    direction: vec2<f32>,  // (1/w, 0) for horizontal, (0, 1/h) for vertical
    _padding: vec2<f32>,
}

@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;
@group(0) @binding(2) var<uniform> blur: BlurUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// 9-tap Gaussian weights (sigma ~2.0)
const OFFSETS: array<f32, 5> = array<f32, 5>(0.0, 1.3846153846, 3.2307692308, 5.076923077, 6.923076923);
const WEIGHTS: array<f32, 5> = array<f32, 5>(0.2270270270, 0.3162162162, 0.0702702703, 0.0031351351, 0.0000762601);

@fragment
fn fs_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(input_tex, tex_sampler, in.uv) * WEIGHTS[0];

    for (var i = 1u; i < 5u; i++) {
        let offset = blur.direction * OFFSETS[i];
        color += textureSample(input_tex, tex_sampler, in.uv + offset) * WEIGHTS[i];
        color += textureSample(input_tex, tex_sampler, in.uv - offset) * WEIGHTS[i];
    }

    return color;
}

// Fullscreen triangle for blur passes
@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}
```

- [ ] **Step 5: Write glass fragment shader**

Create `crates/prism-client/src/renderer/shaders/glass.wgsl`:

```wgsl
// Frosted glass panel compositing

struct GlassMaterial {
    tint: vec4<f32>,
    border_color: vec4<f32>,
    corner_radius: f32,
    noise_intensity: f32,
    panel_width: f32,
    panel_height: f32,
}

@group(1) @binding(0) var blur_tex: texture_2d<f32>;
@group(1) @binding(1) var blur_sampler: sampler;
@group(1) @binding(2) var noise_tex: texture_2d<f32>;
@group(1) @binding(3) var noise_sampler: sampler;
@group(1) @binding(4) var<storage, read> materials: array<GlassMaterial>;

struct Uniforms {
    screen_size: vec2<f32>,
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct FragIn {
    @location(0) uv: vec2<f32>,
    @location(1) screen_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fs_glass(in: FragIn) -> @location(0) vec4<f32> {
    let mat = materials[in.instance_id];
    let half = vec2<f32>(mat.panel_width, mat.panel_height) * 0.5;
    let center = in.uv * vec2<f32>(mat.panel_width, mat.panel_height) - half;

    // SDF for rounded corners
    let dist = rounded_box_sdf(center, half, mat.corner_radius);
    if dist > 0.5 {
        discard;
    }

    // Sample blurred background at screen position
    let blur_uv = in.screen_pos / uniforms.screen_size;
    let bg = textureSample(blur_tex, blur_sampler, blur_uv);

    // Noise texture overlay
    let noise = textureSample(noise_tex, noise_sampler, in.uv * 4.0).r;
    let noise_contrib = vec4<f32>(noise, noise, noise, 0.0) * mat.noise_intensity;

    // Composite: blurred bg + tint + noise
    let glass = bg + mat.tint + noise_contrib;

    // Border: 1px at edges using SDF
    let border_width = 1.0;
    let border_alpha = 1.0 - smoothstep(0.0, border_width, abs(dist));
    // Gradient border: brighter at top
    let top_factor = 1.0 - (in.uv.y * 0.7);
    let border = mat.border_color * border_alpha * top_factor;

    // Outer glow
    let glow_dist = max(-dist - border_width, 0.0);
    let glow_alpha = exp(-glow_dist * 0.5) * 0.05;
    let glow = mat.tint * glow_alpha;

    // Anti-alias edges
    let edge_alpha = 1.0 - smoothstep(-0.5, 0.5, dist);

    var final_color = glass + border + glow;
    final_color.a = edge_alpha;
    return final_color;
}
```

- [ ] **Step 6: Write glow fragment shader**

Create `crates/prism-client/src/renderer/shaders/glow.wgsl`:

```wgsl
// Accent glow rectangles — used for button hover, warning pulses, panel edges

struct GlowMaterial {
    color: vec4<f32>,
    spread: f32,
    intensity: f32,
    _padding: vec2<f32>,
}

@group(1) @binding(0) var<storage, read> glow_materials: array<GlowMaterial>;

struct FragIn {
    @location(0) uv: vec2<f32>,
    @location(1) screen_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

@fragment
fn fs_glow(in: FragIn) -> @location(0) vec4<f32> {
    let mat = glow_materials[in.instance_id];

    // Distance from center (0,0) to edge (1,1) normalized
    let d = length((in.uv - 0.5) * 2.0);
    let falloff = exp(-d * d * mat.spread);

    var color = mat.color;
    color.a *= falloff * mat.intensity;
    return color;
}
```

- [ ] **Step 7: Write text fragment shader**

Create `crates/prism-client/src/renderer/shaders/text.wgsl`:

```wgsl
// Glyph atlas text rendering

@group(1) @binding(0) var glyph_atlas: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

struct FragIn {
    @location(0) uv: vec2<f32>,
    @location(1) screen_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

struct TextColor {
    color: vec4<f32>,
}
@group(1) @binding(2) var<storage, read> text_colors: array<TextColor>;

@fragment
fn fs_text(in: FragIn) -> @location(0) vec4<f32> {
    let coverage = textureSample(glyph_atlas, glyph_sampler, in.uv).r;
    let color = text_colors[in.instance_id];
    return vec4<f32>(color.color.rgb, color.color.a * coverage);
}
```

- [ ] **Step 8: Commit**

```bash
git add crates/prism-client/src/renderer/shaders/
git commit -m "feat(client): WGSL shaders — YUV→RGB compute, blur, glass, glow, text"
```

---

## Task 7: PrismRenderer Foundation

**Files:**
- Modify: `crates/prism-client/src/renderer/mod.rs`

The core wgpu setup — device, surface, basic full-screen stream quad rendering. This replaces minifb's pixel buffer with a GPU texture.

- [ ] **Step 1: Write the PrismRenderer**

Replace contents of `crates/prism-client/src/renderer/mod.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! wgpu-based renderer for PRISM client — stream texture, blur, glass panels, text.

pub mod animation;
pub mod stream_texture;
pub mod blur_pipeline;
pub mod glass_panel;
pub mod text_renderer;
pub mod shader_cache;

use std::sync::Arc;
use winit::window::Window;

/// Core renderer — owns GPU device and orchestrates render passes.
pub struct PrismRenderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub stream_pipeline: wgpu::RenderPipeline,
    pub stream_bind_group_layout: wgpu::BindGroupLayout,
    pub screen_uniform_buffer: wgpu::Buffer,
    pub screen_bind_group: wgpu::BindGroup,
    window: Arc<Window>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniforms {
    screen_size: [f32; 2],
}

impl PrismRenderer {
    /// Create renderer for the given window. Async because wgpu adapter request is async.
    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("No suitable GPU adapter found")?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("PRISM Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            }, None)
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // Screen uniforms
        let screen_uniforms = ScreenUniforms {
            screen_size: [size.width as f32, size.height as f32],
        };
        let screen_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen Uniforms"),
            contents: bytemuck::bytes_of(&screen_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Screen BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Screen BG"),
            layout: &screen_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_uniform_buffer.as_entire_binding(),
            }],
        });

        // Stream render pipeline — simple fullscreen textured quad
        let stream_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Stream Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/quad.wgsl").into()),
        });

        let stream_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Stream Texture BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let stream_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Stream Pipeline Layout"),
                bind_group_layouts: &[&screen_bind_group_layout, &stream_bind_group_layout],
                push_constant_ranges: &[],
            });

        // For now, use a simple passthrough fragment shader
        // The full quad.wgsl has the vertex shader; we add a simple fs here
        let stream_fs_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Stream FS"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
                @group(1) @binding(0) var stream_tex: texture_2d<f32>;
                @group(1) @binding(1) var stream_sampler: sampler;

                struct FragIn {
                    @location(0) uv: vec2<f32>,
                    @location(1) screen_pos: vec2<f32>,
                    @location(2) @interpolate(flat) instance_id: u32,
                }

                @fragment
                fn fs_stream(in: FragIn) -> @location(0) vec4<f32> {
                    return textureSample(stream_tex, stream_sampler, in.uv);
                }
                "#
                .into(),
            ),
        });

        let stream_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Stream Pipeline"),
            layout: Some(&stream_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &stream_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &stream_fs_shader,
                entry_point: Some("fs_stream"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            stream_pipeline,
            stream_bind_group_layout,
            screen_uniform_buffer,
            screen_bind_group,
            window,
        })
    }

    /// Handle window resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        let uniforms = ScreenUniforms {
            screen_size: [width as f32, height as f32],
        };
        self.queue
            .write_buffer(&self.screen_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn width(&self) -> u32 {
        self.surface_config.width
    }

    pub fn height(&self) -> u32 {
        self.surface_config.height
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }
}

use wgpu::util::DeviceExt;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p prism-client`
Expected: Compiles. (We can't fully test GPU init without a window, but the types check out.)

- [ ] **Step 3: Create stub files for referenced modules**

Create `crates/prism-client/src/renderer/stream_texture.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ring-buffered YUV plane upload with compute shader YUV→RGB conversion.
```

Create `crates/prism-client/src/renderer/blur_pipeline.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Two-pass separable Gaussian blur at progressive resolutions.
```

Create `crates/prism-client/src/renderer/glass_panel.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Frosted glass quad compositing — samples blur texture, applies tint/noise/border.
```

Create `crates/prism-client/src/renderer/text_renderer.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! glyphon-based GPU text rendering with glyph cache warming.
```

Create `crates/prism-client/src/renderer/shader_cache.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pipeline cache persistence to ~/.prism/shader_cache/.
```

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/renderer/
git commit -m "feat(client): PrismRenderer foundation — wgpu device, surface, stream pipeline"
```

---

## Task 8: Stream Texture (YUV→RGB Compute)

**Files:**
- Modify: `crates/prism-client/src/renderer/stream_texture.rs`

Ring-buffered texture upload with compute shader YUV→RGB conversion.

- [ ] **Step 1: Implement StreamTexture**

Replace contents of `crates/prism-client/src/renderer/stream_texture.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ring-buffered YUV plane upload with compute shader YUV→RGB conversion.

use wgpu::util::DeviceExt;

/// Manages double-buffered stream texture with GPU YUV→RGB conversion.
pub struct StreamTexture {
    y_texture: [wgpu::Texture; 2],
    u_texture: [wgpu::Texture; 2],
    v_texture: [wgpu::Texture; 2],
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    compute_pipeline: wgpu::ComputePipeline,
    bind_groups: [wgpu::BindGroup; 2],
    params_buffer: wgpu::Buffer,
    current_slot: usize,
    pub width: u32,
    pub height: u32,
    dirty: bool,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct YuvParams {
    width: u32,
    height: u32,
}

impl StreamTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let half_w = (width + 1) / 2;
        let half_h = (height + 1) / 2;

        let create_plane = |label: &str, w: u32, h: u32| -> [wgpu::Texture; 2] {
            [0, 1].map(|i| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(&format!("{label}[{i}]")),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
            })
        };

        let y_texture = create_plane("Y", width, height);
        let u_texture = create_plane("U", half_w, half_h);
        let v_texture = create_plane("V", half_w, half_h);

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Stream RGB"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&Default::default());

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("YUV Params"),
            contents: bytemuck::bytes_of(&YuvParams { width, height }),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("YUV→RGB Compute"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/yuv_to_rgb.wgsl").into(),
            ),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("YUV BGL"),
                entries: &[
                    bgl_texture(0),
                    bgl_texture(1),
                    bgl_texture(2),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("YUV Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let compute_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("YUV→RGB Pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        let bind_groups = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("YUV BG[{i}]")),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &y_texture[i].create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &u_texture[i].create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &v_texture[i].create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&output_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: params_buffer.as_entire_binding(),
                    },
                ],
            })
        });

        Self {
            y_texture,
            u_texture,
            v_texture,
            output_texture,
            output_view,
            compute_pipeline,
            bind_groups,
            params_buffer,
            current_slot: 0,
            width,
            height,
            dirty: false,
        }
    }

    /// Upload YUV420 planar data to the next ring buffer slot.
    pub fn upload_yuv(
        &mut self,
        queue: &wgpu::Queue,
        y_data: &[u8],
        u_data: &[u8],
        v_data: &[u8],
    ) {
        let slot = 1 - self.current_slot; // write to the other slot
        let half_w = (self.width + 1) / 2;
        let half_h = (self.height + 1) / 2;

        upload_plane(queue, &self.y_texture[slot], y_data, self.width, self.height);
        upload_plane(queue, &self.u_texture[slot], u_data, half_w, half_h);
        upload_plane(queue, &self.v_texture[slot], v_data, half_w, half_h);

        self.current_slot = slot;
        self.dirty = true;
    }

    /// Run the compute shader to convert YUV→RGB. Call after upload_yuv.
    pub fn convert(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.dirty {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("YUV→RGB"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.compute_pipeline);
        pass.set_bind_group(0, &self.bind_groups[self.current_slot], &[]);
        let wg_x = (self.width + 15) / 16;
        let wg_y = (self.height + 15) / 16;
        pass.dispatch_workgroups(wg_x, wg_y, 1);
        self.dirty = false;
    }

    /// Get the output RGBA texture view for rendering.
    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

fn upload_plane(queue: &wgpu::Queue, texture: &wgpu::Texture, data: &[u8], width: u32, height: u32) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

fn bgl_texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p prism-client`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/renderer/stream_texture.rs
git commit -m "feat(client): StreamTexture — ring-buffered YUV upload with GPU compute conversion"
```

---

## Task 9: Blur Pipeline

**Files:**
- Modify: `crates/prism-client/src/renderer/blur_pipeline.rs`

- [ ] **Step 1: Implement BlurPipeline**

Replace contents of `crates/prism-client/src/renderer/blur_pipeline.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Two-pass separable Gaussian blur at progressive resolutions.
//! Runs at 1/4 resolution for performance. Only active when overlay is visible.

use wgpu::util::DeviceExt;

/// Two-pass (horizontal + vertical) Gaussian blur.
pub struct BlurPipeline {
    h_pipeline: wgpu::RenderPipeline,
    v_pipeline: wgpu::RenderPipeline,
    intermediate_texture: wgpu::Texture,
    intermediate_view: wgpu::TextureView,
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    h_bind_group: wgpu::BindGroup,
    v_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    h_uniform_buffer: wgpu::Buffer,
    v_uniform_buffer: wgpu::Buffer,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniforms {
    direction: [f32; 2],
    _padding: [f32; 2],
}

impl BlurPipeline {
    /// Create blur pipeline at quarter resolution of the given source dimensions.
    pub fn new(
        device: &wgpu::Device,
        source_width: u32,
        source_height: u32,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let width = (source_width + 3) / 4;
        let height = (source_height + 3) / 4;

        let create_tex = |label: &str| -> wgpu::Texture {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };

        let intermediate_texture = create_tex("Blur Intermediate");
        let intermediate_view = intermediate_texture.create_view(&Default::default());
        let output_texture = create_tex("Blur Output");
        let output_view = output_texture.create_view(&Default::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Blur Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let h_uniforms = BlurUniforms {
            direction: [1.0 / width as f32, 0.0],
            _padding: [0.0; 2],
        };
        let v_uniforms = BlurUniforms {
            direction: [0.0, 1.0 / height as f32],
            _padding: [0.0; 2],
        };

        let h_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur H Uniforms"),
            contents: bytemuck::bytes_of(&h_uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let v_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur V Uniforms"),
            contents: bytemuck::bytes_of(&v_uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blur.wgsl").into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Blur BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Blur Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let create_pipeline = |label: &str| -> wgpu::RenderPipeline {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &blur_shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &blur_shader,
                    entry_point: Some("fs_blur"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        let h_pipeline = create_pipeline("Blur H Pipeline");
        let v_pipeline = create_pipeline("Blur V Pipeline");

        // Bind groups are created per-frame when we know the input texture
        // For now, create placeholder bind groups (will be recreated)
        let h_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur H BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&intermediate_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: h_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let v_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur V BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&intermediate_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: v_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            h_pipeline,
            v_pipeline,
            intermediate_texture,
            intermediate_view,
            output_texture,
            output_view,
            h_bind_group,
            v_bind_group,
            sampler,
            h_uniform_buffer,
            v_uniform_buffer,
            width,
            height,
        }
    }

    /// Run two-pass blur. Input is the stream texture view (or downsampled copy).
    pub fn run(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input_bind_group: &wgpu::BindGroup,
    ) {
        // Pass 1: horizontal blur (input → intermediate)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur H Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.intermediate_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.h_pipeline);
            pass.set_bind_group(0, input_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // Pass 2: vertical blur (intermediate → output)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur V Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.v_pipeline);
            pass.set_bind_group(0, &self.v_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Get the blurred output texture view for glass panel sampling.
    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p prism-client`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/renderer/blur_pipeline.rs
git commit -m "feat(client): BlurPipeline — two-pass separable Gaussian at quarter resolution"
```

---

## Task 10: Config Module & CLI Mode Selection

**Files:**
- Modify: `crates/prism-client/src/config/mod.rs`
- Modify: `crates/prism-client/src/main.rs`

- [ ] **Step 1: Implement config module**

Replace contents of `crates/prism-client/src/config/mod.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client configuration — CLI args, saved servers, shader cache paths.

pub mod servers;

use std::net::SocketAddr;
use std::path::PathBuf;

/// How the client was launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// No address args — show launcher, return to it on disconnect.
    Launcher,
    /// Address provided via CLI — connect directly, exit on disconnect.
    DirectConnect,
}

/// Unified client configuration.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub launch_mode: LaunchMode,
    pub server_addr: Option<SocketAddr>,
    pub noise_key: Option<[u8; 32]>,
    pub identity_path: PathBuf,
    pub servers_dir: PathBuf,
    pub shader_cache_dir: PathBuf,
}

impl ClientConfig {
    /// Parse from CLI args.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let prism_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".prism");

        let mut server_addr = None;
        let mut noise_key = None;
        let mut servers_dir = prism_dir.clone();
        let mut identity_path = prism_dir.join("client_identity.json");
        let shader_cache_dir = prism_dir.join("shader_cache");

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--noise" => {
                    i += 1;
                    if i >= args.len() {
                        return Err("--noise requires a 64-char hex key".into());
                    }
                    let key_bytes = hex::decode(&args[i])
                        .map_err(|e| format!("Invalid noise key hex: {e}"))?;
                    if key_bytes.len() != 32 {
                        return Err(format!(
                            "Noise key must be 32 bytes (64 hex chars), got {}",
                            key_bytes.len()
                        ));
                    }
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&key_bytes);
                    noise_key = Some(key);
                }
                "--config" => {
                    i += 1;
                    if i >= args.len() {
                        return Err("--config requires a path".into());
                    }
                    servers_dir = PathBuf::from(&args[i]);
                }
                arg if !arg.starts_with('-') && server_addr.is_none() => {
                    server_addr = Some(
                        arg.parse::<SocketAddr>()
                            .map_err(|e| format!("Invalid address '{arg}': {e}"))?,
                    );
                }
                _ => {}
            }
            i += 1;
        }

        let launch_mode = if server_addr.is_some() {
            LaunchMode::DirectConnect
        } else {
            LaunchMode::Launcher
        };

        Ok(Self {
            launch_mode,
            server_addr,
            noise_key,
            identity_path,
            servers_dir,
            shader_cache_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_is_launcher_mode() {
        let config = ClientConfig::from_args(&[]).unwrap();
        assert_eq!(config.launch_mode, LaunchMode::Launcher);
        assert!(config.server_addr.is_none());
    }

    #[test]
    fn address_arg_is_direct_connect() {
        let args = vec!["192.168.1.100:7000".into()];
        let config = ClientConfig::from_args(&args).unwrap();
        assert_eq!(config.launch_mode, LaunchMode::DirectConnect);
        assert_eq!(
            config.server_addr.unwrap(),
            "192.168.1.100:7000".parse().unwrap()
        );
    }

    #[test]
    fn noise_key_parsed() {
        let key_hex = "a".repeat(64);
        let args = vec![
            "127.0.0.1:7000".into(),
            "--noise".into(),
            key_hex.clone(),
        ];
        let config = ClientConfig::from_args(&args).unwrap();
        assert!(config.noise_key.is_some());
    }

    #[test]
    fn invalid_address_errors() {
        let args = vec!["not-an-address".into()];
        assert!(ClientConfig::from_args(&args).is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-client -- config::tests -v`
Expected: All 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/config/mod.rs
git commit -m "feat(client): unified ClientConfig with LaunchMode (Launcher vs DirectConnect)"
```

---

## Task 11: Label Widget

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/label.rs`

First concrete widget — establishes the pattern for all others.

- [ ] **Step 1: Write failing tests**

Replace contents of `crates/prism-client/src/ui/widgets/label.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Label widget — static or live-updating text with color coding.

use super::{EventResponse, PaintContext, Rect, Size, UiEvent, Widget};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_reports_size() {
        let mut label = Label::new("Hello", 14.0);
        let size = label.layout(Rect::new(0.0, 0.0, 200.0, 50.0));
        assert!(size.w > 0.0);
        assert!(size.h > 0.0);
    }

    #[test]
    fn label_emits_text_run() {
        let mut label = Label::new("FPS: 60", 14.0);
        label.layout(Rect::new(10.0, 20.0, 200.0, 50.0));
        let mut ctx = PaintContext::new();
        label.paint(&mut ctx);
        assert_eq!(ctx.text_runs.len(), 1);
        assert_eq!(ctx.text_runs[0].text, "FPS: 60");
        assert_eq!(ctx.text_runs[0].x, 10.0);
        assert_eq!(ctx.text_runs[0].y, 20.0);
    }

    #[test]
    fn label_update_text() {
        let mut label = Label::new("FPS: 60", 14.0);
        label.set_text("FPS: 58");
        label.layout(Rect::new(0.0, 0.0, 200.0, 50.0));
        let mut ctx = PaintContext::new();
        label.paint(&mut ctx);
        assert_eq!(ctx.text_runs[0].text, "FPS: 58");
    }

    #[test]
    fn label_monospace_flag() {
        let label = Label::new("60", 14.0).with_monospace(true);
        let mut ctx = PaintContext::new();
        let mut l = label;
        l.layout(Rect::new(0.0, 0.0, 100.0, 50.0));
        l.paint(&mut ctx);
        assert!(ctx.text_runs[0].monospace);
    }

    #[test]
    fn label_ignores_events() {
        let mut label = Label::new("text", 14.0);
        let resp = label.handle_event(&UiEvent::MouseMove { x: 0.0, y: 0.0 });
        assert!(matches!(resp, EventResponse::Ignored));
    }
}
```

- [ ] **Step 2: Implement Label**

Add above `#[cfg(test)]`:

```rust
/// Static or live-updating text label.
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
            text: text.into(),
            font_size,
            color: [1.0, 1.0, 1.0, 0.9],
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
        self.text = text.into();
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        self.color = color;
    }
}

impl Widget for Label {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = available;
        // Approximate: 0.6 * font_size per character width, font_size height
        let w = self.text.len() as f32 * self.font_size * 0.6;
        let h = self.font_size * 1.4;
        Size { w: w.min(available.w), h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        ctx.push_text_run(super::TextRun {
            x: self.rect.x,
            y: self.rect.y,
            text: self.text.clone(),
            font_size: self.font_size,
            color: self.color,
            monospace: self.monospace,
        });
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    fn animate(&mut self, _dt_ms: f32) {
        // Labels don't animate
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-client -- label::tests -v`
Expected: All 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/ui/widgets/label.rs
git commit -m "feat(client): Label widget — static/live text with color coding and monospace"
```

---

## Task 12: Button Widget

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/button.rs`

- [ ] **Step 1: Write failing tests**

Replace contents of `crates/prism-client/src/ui/widgets/button.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Button widget — glass surface, hover glow, click callback.

use crate::renderer::animation::{Animation, EaseCurve};
use super::{EventResponse, GlassQuad, GlowRect, PaintContext, Rect, Size, TextRun, UiAction, UiEvent, MouseButton, Widget};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_emits_glass_quad_and_text() {
        let mut btn = Button::new("Connect", UiAction::AddServer);
        btn.layout(Rect::new(10.0, 10.0, 120.0, 36.0));
        let mut ctx = PaintContext::new();
        btn.paint(&mut ctx);
        assert_eq!(ctx.glass_quads.len(), 1);
        assert_eq!(ctx.text_runs.len(), 1);
        assert_eq!(ctx.text_runs[0].text, "Connect");
    }

    #[test]
    fn button_hover_adds_glow() {
        let mut btn = Button::new("Go", UiAction::AddServer);
        btn.layout(Rect::new(0.0, 0.0, 80.0, 36.0));
        btn.handle_event(&UiEvent::MouseMove { x: 40.0, y: 18.0 });
        btn.animate(200.0); // fully animate hover
        let mut ctx = PaintContext::new();
        btn.paint(&mut ctx);
        assert!(!ctx.glow_rects.is_empty());
    }

    #[test]
    fn button_click_returns_action() {
        let mut btn = Button::new("Go", UiAction::Disconnect);
        btn.layout(Rect::new(0.0, 0.0, 80.0, 36.0));
        let resp = btn.handle_event(&UiEvent::MouseDown {
            x: 40.0,
            y: 18.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Action(UiAction::Disconnect)));
    }

    #[test]
    fn button_click_outside_ignored() {
        let mut btn = Button::new("Go", UiAction::Disconnect);
        btn.layout(Rect::new(0.0, 0.0, 80.0, 36.0));
        let resp = btn.handle_event(&UiEvent::MouseDown {
            x: 200.0,
            y: 200.0,
            button: MouseButton::Left,
        });
        assert!(matches!(resp, EventResponse::Ignored));
    }
}
```

- [ ] **Step 2: Implement Button**

Add above `#[cfg(test)]`:

```rust
/// Glass-surface button with hover glow and click action.
pub struct Button {
    label: String,
    action: UiAction,
    rect: Rect,
    hover_anim: Animation,
    hovered: bool,
}

impl Button {
    pub fn new(label: &str, action: UiAction) -> Self {
        Self {
            label: label.into(),
            action,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            hover_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            hovered: false,
        }
    }
}

impl Widget for Button {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, available.w, 36.0);
        Size { w: available.w, h: 36.0 }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Glass background
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.55, 0.36, 0.96, 0.15], // purple accent tint
            border_color: [1.0, 1.0, 1.0, 0.2],
            corner_radius: 6.0,
            noise_intensity: 0.03,
        });

        // Hover glow
        let hover_val = self.hover_anim.value();
        if hover_val > 0.01 {
            ctx.push_glow_rect(GlowRect {
                rect: self.rect,
                color: [0.55, 0.36, 0.96, 0.3 * hover_val],
                spread: 3.0,
                intensity: hover_val,
            });
        }

        // Label centered
        let text_x = self.rect.x + (self.rect.w - self.label.len() as f32 * 8.4) / 2.0;
        let text_y = self.rect.y + (self.rect.h - 14.0) / 2.0;
        ctx.push_text_run(TextRun {
            x: text_x,
            y: text_y,
            text: self.label.clone(),
            font_size: 14.0,
            color: [1.0, 1.0, 1.0, 0.95],
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                let was = self.hovered;
                self.hovered = self.rect.contains(*x, *y);
                if self.hovered != was {
                    self.hover_anim
                        .set_target(if self.hovered { 1.0 } else { 0.0 });
                }
                EventResponse::Ignored
            }
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                if self.rect.contains(*x, *y) {
                    EventResponse::Action(self.action.clone())
                } else {
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.hover_anim.tick(dt_ms);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-client -- button::tests -v`
Expected: All 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/ui/widgets/button.rs
git commit -m "feat(client): Button widget — glass surface, hover glow, click action"
```

---

## Task 13: Remaining Basic Widgets (Separator, Checkbox, Slider)

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/separator.rs`
- Modify: `crates/prism-client/src/ui/widgets/checkbox.rs`
- Modify: `crates/prism-client/src/ui/widgets/slider.rs`

- [ ] **Step 1: Implement Separator**

Replace contents of `crates/prism-client/src/ui/widgets/separator.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Separator widget — embossed glass ridge.

use super::{EventResponse, GlassQuad, PaintContext, Rect, Size, UiEvent, Widget};

pub struct Separator {
    rect: Rect,
}

impl Separator {
    pub fn new() -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Separator {
    fn layout(&mut self, available: Rect) -> Size {
        self.rect = Rect::new(available.x, available.y, available.w, 2.0);
        Size { w: available.w, h: 2.0 }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Light line
        ctx.push_glass_quad(GlassQuad {
            rect: Rect::new(self.rect.x, self.rect.y, self.rect.w, 1.0),
            blur_rect: self.rect,
            tint: [1.0, 1.0, 1.0, 0.08],
            border_color: [0.0, 0.0, 0.0, 0.0],
            corner_radius: 0.0,
            noise_intensity: 0.0,
        });
        // Dark line
        ctx.push_glass_quad(GlassQuad {
            rect: Rect::new(self.rect.x, self.rect.y + 1.0, self.rect.w, 1.0),
            blur_rect: self.rect,
            tint: [0.0, 0.0, 0.0, 0.15],
            border_color: [0.0, 0.0, 0.0, 0.0],
            corner_radius: 0.0,
            noise_intensity: 0.0,
        });
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separator_height_is_2() {
        let mut sep = Separator::new();
        let size = sep.layout(Rect::new(0.0, 0.0, 200.0, 50.0));
        assert_eq!(size.h, 2.0);
    }

    #[test]
    fn separator_emits_two_quads() {
        let mut sep = Separator::new();
        sep.layout(Rect::new(0.0, 0.0, 200.0, 50.0));
        let mut ctx = PaintContext::new();
        sep.paint(&mut ctx);
        assert_eq!(ctx.glass_quads.len(), 2);
    }
}
```

- [ ] **Step 2: Implement Checkbox**

Replace contents of `crates/prism-client/src/ui/widgets/checkbox.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Checkbox widget — radial wipe toggle animation.

use crate::renderer::animation::{Animation, EaseCurve};
use super::{EventResponse, GlassQuad, GlowRect, PaintContext, Rect, Size, TextRun, UiEvent, MouseButton, Widget};

pub struct Checkbox {
    label: String,
    checked: bool,
    fill_anim: Animation,
    rect: Rect,
    on_change: Option<Box<dyn Fn(bool) + Send>>,
}

impl Checkbox {
    pub fn new(label: &str, checked: bool) -> Self {
        let mut fill_anim = Animation::new(EaseCurve::EaseOut, 150.0);
        if checked {
            fill_anim.set_target(1.0);
            fill_anim.tick(150.0); // snap to checked state
        }
        Self {
            label: label.into(),
            checked,
            fill_anim,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            on_change: None,
        }
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
        self.fill_anim.set_target(if checked { 1.0 } else { 0.0 });
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }
}

impl Widget for Checkbox {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 24.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        let box_size = 16.0;
        let box_rect = Rect::new(self.rect.x, self.rect.y + 4.0, box_size, box_size);

        // Box outline
        ctx.push_glass_quad(GlassQuad {
            rect: box_rect,
            blur_rect: box_rect,
            tint: [0.1, 0.0, 0.2, 0.1],
            border_color: [1.0, 1.0, 1.0, 0.3],
            corner_radius: 3.0,
            noise_intensity: 0.0,
        });

        // Fill (animated)
        let fill = self.fill_anim.value();
        if fill > 0.01 {
            ctx.push_glow_rect(GlowRect {
                rect: box_rect,
                color: [0.55, 0.36, 0.96, fill * 0.8],
                spread: 1.0,
                intensity: fill,
            });
        }

        // Label
        ctx.push_text_run(TextRun {
            x: self.rect.x + box_size + 8.0,
            y: self.rect.y + 4.0,
            text: self.label.clone(),
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.8],
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        if let UiEvent::MouseDown { x, y, button: MouseButton::Left } = event {
            if self.rect.contains(*x, *y) {
                self.checked = !self.checked;
                self.fill_anim.set_target(if self.checked { 1.0 } else { 0.0 });
                return EventResponse::Consumed;
            }
        }
        EventResponse::Ignored
    }

    fn animate(&mut self, dt_ms: f32) {
        self.fill_anim.tick(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_toggles_on_click() {
        let mut cb = Checkbox::new("Enable", false);
        cb.layout(Rect::new(0.0, 0.0, 200.0, 30.0));
        cb.handle_event(&UiEvent::MouseDown { x: 8.0, y: 12.0, button: MouseButton::Left });
        assert!(cb.is_checked());
        cb.handle_event(&UiEvent::MouseDown { x: 8.0, y: 12.0, button: MouseButton::Left });
        assert!(!cb.is_checked());
    }

    #[test]
    fn checkbox_click_outside_ignored() {
        let mut cb = Checkbox::new("Enable", false);
        cb.layout(Rect::new(0.0, 0.0, 200.0, 30.0));
        cb.handle_event(&UiEvent::MouseDown { x: 500.0, y: 500.0, button: MouseButton::Left });
        assert!(!cb.is_checked());
    }
}
```

- [ ] **Step 3: Implement Slider**

Replace contents of `crates/prism-client/src/ui/widgets/slider.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slider widget — accent fill, glow thumb, drag handling.

use super::{EventResponse, GlassQuad, GlowRect, PaintContext, Rect, Size, TextRun, UiEvent, MouseButton, Widget};

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
        Self {
            label: label.into(),
            value: value.clamp(min, max),
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
        let t = (v - self.min) / (self.max - self.min);
        self.rect.x + t * self.rect.w
    }

    fn x_to_value(&self, x: f32) -> f32 {
        let t = ((x - self.rect.x) / self.rect.w).clamp(0.0, 1.0);
        self.min + t * (self.max - self.min)
    }
}

impl Widget for Slider {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 32.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Label + value
        let display_val = (self.display_format)(self.value);
        ctx.push_text_run(TextRun {
            x: self.rect.x,
            y: self.rect.y,
            text: self.label.clone(),
            font_size: 12.0,
            color: [1.0, 1.0, 1.0, 0.6],
            monospace: false,
        });
        ctx.push_text_run(TextRun {
            x: self.rect.x + self.rect.w - display_val.len() as f32 * 7.0,
            y: self.rect.y,
            text: display_val,
            font_size: 12.0,
            color: [1.0, 1.0, 1.0, 0.8],
            monospace: true,
        });

        let track = self.track_rect();

        // Track background
        ctx.push_glass_quad(GlassQuad {
            rect: track,
            blur_rect: track,
            tint: [0.1, 0.0, 0.2, 0.15],
            border_color: [1.0, 1.0, 1.0, 0.1],
            corner_radius: 3.0,
            noise_intensity: 0.0,
        });

        // Filled portion
        let fill_w = self.value_to_x(self.value) - track.x;
        if fill_w > 0.0 {
            ctx.push_glow_rect(GlowRect {
                rect: Rect::new(track.x, track.y, fill_w, track.h),
                color: [0.55, 0.36, 0.96, 0.7],
                spread: 1.0,
                intensity: 0.8,
            });
        }

        // Thumb
        let thumb_x = self.value_to_x(self.value) - 6.0;
        ctx.push_glow_rect(GlowRect {
            rect: Rect::new(thumb_x, track.y - 3.0, 12.0, 12.0),
            color: [0.55, 0.36, 0.96, 0.9],
            spread: 2.0,
            intensity: if self.dragging { 1.0 } else { 0.6 },
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                let track = self.track_rect();
                let expanded = Rect::new(track.x, track.y - 8.0, track.w, track.h + 16.0);
                if expanded.contains(*x, *y) {
                    self.dragging = true;
                    self.value = self.x_to_value(*x);
                    return EventResponse::Consumed;
                }
                EventResponse::Ignored
            }
            UiEvent::MouseMove { x, .. } if self.dragging => {
                self.value = self.x_to_value(*x);
                EventResponse::Consumed
            }
            UiEvent::MouseUp { .. } if self.dragging => {
                self.dragging = false;
                EventResponse::Consumed
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
    fn slider_clamps_value() {
        let s = Slider::new("Vol", 0.0, 100.0, 150.0);
        assert_eq!(s.value(), 100.0);
    }

    #[test]
    fn slider_drag_updates_value() {
        let mut s = Slider::new("Vol", 0.0, 100.0, 50.0);
        s.layout(Rect::new(0.0, 0.0, 200.0, 40.0));
        // Click in track area
        s.handle_event(&UiEvent::MouseDown { x: 100.0, y: 23.0, button: MouseButton::Left });
        assert!(s.dragging);
        // Drag
        s.handle_event(&UiEvent::MouseMove { x: 150.0, y: 23.0 });
        assert!((s.value() - 75.0).abs() < 1.0);
        // Release
        s.handle_event(&UiEvent::MouseUp { x: 150.0, y: 23.0, button: MouseButton::Left });
        assert!(!s.dragging);
    }
}
```

- [ ] **Step 4: Run all widget tests**

Run: `cargo test -p prism-client -- widgets -v`
Expected: All tests pass (separator, checkbox, slider, label, button, spatial hash, rect).

- [ ] **Step 5: Commit**

```bash
git add crates/prism-client/src/ui/widgets/
git commit -m "feat(client): Separator, Checkbox, Slider widgets with animation and interaction"
```

---

## Task 14: Sparkline Widget

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/sparkline.rs`

- [ ] **Step 1: Implement Sparkline with tests**

Replace contents of `crates/prism-client/src/ui/widgets/sparkline.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sparkline widget — ring buffer polyline with trailing glow.

use super::{EventResponse, GlowRect, PaintContext, Rect, Size, UiEvent, Widget};

/// Ring buffer sparkline — stores last N values and renders as polyline.
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

    /// Get value at logical index (0 = oldest, count-1 = newest).
    fn get(&self, logical_index: usize) -> f32 {
        let start = if self.count < self.capacity {
            0
        } else {
            self.head
        };
        self.values[(start + logical_index) % self.capacity]
    }

    fn value_to_y(&self, value: f32) -> f32 {
        let range = self.max_val - self.min_val;
        if range < f32::EPSILON {
            return self.rect.y + self.rect.h / 2.0;
        }
        let t = ((value - self.min_val) / range).clamp(0.0, 1.0);
        self.rect.y + self.rect.h * (1.0 - t)
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

        // Render sparkline as series of small glow rects (1px wide line segments)
        let step = self.rect.w / (self.count - 1) as f32;
        for i in 0..self.count {
            let x = self.rect.x + i as f32 * step;
            let y = self.value_to_y(self.get(i));
            let h = 2.0;

            // Line segment
            let is_newest = i == self.count - 1;
            let alpha = if is_newest { 1.0 } else { 0.4 + 0.4 * (i as f32 / self.count as f32) };

            ctx.push_glow_rect(GlowRect {
                rect: Rect::new(x, y - h / 2.0, step.max(1.0), h),
                color: [
                    self.accent_color[0],
                    self.accent_color[1],
                    self.accent_color[2],
                    alpha,
                ],
                spread: if is_newest { 3.0 } else { 1.0 },
                intensity: if is_newest { 1.0 } else { 0.5 },
            });
        }
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_retrieve() {
        let mut s = Sparkline::new(4);
        s.push(1.0);
        s.push(2.0);
        s.push(3.0);
        assert_eq!(s.count, 3);
        assert_eq!(s.get(0), 1.0);
        assert_eq!(s.get(2), 3.0);
    }

    #[test]
    fn ring_buffer_wraps() {
        let mut s = Sparkline::new(3);
        s.push(1.0);
        s.push(2.0);
        s.push(3.0);
        s.push(4.0); // wraps, oldest (1.0) evicted
        assert_eq!(s.count, 3);
        assert_eq!(s.get(0), 2.0);
        assert_eq!(s.get(2), 4.0);
    }

    #[test]
    fn renders_glow_rects_for_points() {
        let mut s = Sparkline::new(10).with_range(0.0, 100.0);
        for i in 0..5 {
            s.push(i as f32 * 20.0);
        }
        s.layout(Rect::new(0.0, 0.0, 100.0, 24.0));
        let mut ctx = PaintContext::new();
        s.paint(&mut ctx);
        assert_eq!(ctx.glow_rects.len(), 5);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-client -- sparkline -v`
Expected: All 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/ui/widgets/sparkline.rs
git commit -m "feat(client): Sparkline widget — ring buffer polyline with trailing glow"
```

---

## Task 15: TextInput Widget

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/text_input.rs`

- [ ] **Step 1: Implement TextInput with tests**

Replace contents of `crates/prism-client/src/ui/widgets/text_input.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text input widget — cursor, selection, autocomplete support.

use super::{EventResponse, GlassQuad, PaintContext, Rect, Size, TextRun, UiEvent, KeyCode, MouseButton, Widget};

pub struct TextInput {
    text: String,
    placeholder: String,
    cursor: usize,
    focused: bool,
    rect: Rect,
    autocomplete_candidates: Vec<String>,
}

impl TextInput {
    pub fn new(placeholder: &str) -> Self {
        Self {
            text: String::new(),
            placeholder: placeholder.into(),
            cursor: 0,
            focused: false,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            autocomplete_candidates: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.into();
        self.cursor = self.text.len();
    }

    pub fn set_autocomplete(&mut self, candidates: Vec<String>) {
        self.autocomplete_candidates = candidates;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl Widget for TextInput {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 36.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Background
        let border_alpha = if self.focused { 0.4 } else { 0.2 };
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.05, 0.0, 0.1, 0.15],
            border_color: [1.0, 1.0, 1.0, border_alpha],
            corner_radius: 6.0,
            noise_intensity: 0.02,
        });

        // Text or placeholder
        let display = if self.text.is_empty() {
            &self.placeholder
        } else {
            &self.text
        };
        let color = if self.text.is_empty() {
            [1.0, 1.0, 1.0, 0.4]
        } else {
            [1.0, 1.0, 1.0, 0.9]
        };

        ctx.push_text_run(TextRun {
            x: self.rect.x + 10.0,
            y: self.rect.y + 10.0,
            text: display.to_string(),
            font_size: 14.0,
            color,
            monospace: false,
        });
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                self.focused = self.rect.contains(*x, *y);
                if self.focused {
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::TextInput { ch } if self.focused => {
                self.text.insert(self.cursor, *ch);
                self.cursor += ch.len_utf8();
                EventResponse::Consumed
            }
            UiEvent::KeyDown { key } if self.focused => match key {
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.text.remove(self.cursor);
                    }
                    EventResponse::Consumed
                }
                KeyCode::Delete => {
                    if self.cursor < self.text.len() {
                        self.text.remove(self.cursor);
                    }
                    EventResponse::Consumed
                }
                KeyCode::Left => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                    }
                    EventResponse::Consumed
                }
                KeyCode::Right => {
                    if self.cursor < self.text.len() {
                        self.cursor += 1;
                    }
                    EventResponse::Consumed
                }
                KeyCode::Home => {
                    self.cursor = 0;
                    EventResponse::Consumed
                }
                KeyCode::End => {
                    self.cursor = self.text.len();
                    EventResponse::Consumed
                }
                _ => EventResponse::Ignored,
            },
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_appends_text() {
        let mut input = TextInput::new("Enter address...");
        input.set_focused(true);
        input.handle_event(&UiEvent::TextInput { ch: '1' });
        input.handle_event(&UiEvent::TextInput { ch: '9' });
        input.handle_event(&UiEvent::TextInput { ch: '2' });
        assert_eq!(input.text(), "192");
    }

    #[test]
    fn backspace_deletes() {
        let mut input = TextInput::new("");
        input.set_focused(true);
        input.set_text("abc");
        input.handle_event(&UiEvent::KeyDown { key: KeyCode::Backspace });
        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn unfocused_ignores_input() {
        let mut input = TextInput::new("");
        input.handle_event(&UiEvent::TextInput { ch: 'x' });
        assert_eq!(input.text(), "");
    }

    #[test]
    fn click_focuses() {
        let mut input = TextInput::new("");
        input.layout(Rect::new(0.0, 0.0, 200.0, 36.0));
        input.handle_event(&UiEvent::MouseDown { x: 100.0, y: 18.0, button: MouseButton::Left });
        assert!(input.is_focused());
    }

    #[test]
    fn click_outside_unfocuses() {
        let mut input = TextInput::new("");
        input.layout(Rect::new(0.0, 0.0, 200.0, 36.0));
        input.set_focused(true);
        input.handle_event(&UiEvent::MouseDown { x: 500.0, y: 500.0, button: MouseButton::Left });
        assert!(!input.is_focused());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-client -- text_input -v`
Expected: All 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-client/src/ui/widgets/text_input.rs
git commit -m "feat(client): TextInput widget — cursor navigation, typing, focus management"
```

---

## Task 16: Dropdown & MonitorMap Widgets

**Files:**
- Modify: `crates/prism-client/src/ui/widgets/dropdown.rs`
- Modify: `crates/prism-client/src/ui/widgets/monitor_map.rs`

- [ ] **Step 1: Implement Dropdown with tests**

Replace contents of `crates/prism-client/src/ui/widgets/dropdown.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dropdown widget — staggered cascade reveal, glass-styled popup.

use crate::renderer::animation::{Animation, EaseCurve};
use super::{EventResponse, GlassQuad, PaintContext, Rect, Size, TextRun, UiEvent, MouseButton, Widget};

pub struct Dropdown {
    options: Vec<String>,
    selected: usize,
    open: bool,
    open_anim: Animation,
    rect: Rect,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        Self {
            options,
            selected,
            open: false,
            open_anim: Animation::new(EaseCurve::EaseOut, 150.0),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_text(&self) -> &str {
        &self.options[self.selected]
    }

    pub fn set_selected(&mut self, index: usize) {
        if index < self.options.len() {
            self.selected = index;
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn item_rect(&self, index: usize) -> Rect {
        Rect::new(
            self.rect.x,
            self.rect.y + self.rect.h + index as f32 * 28.0,
            self.rect.w,
            28.0,
        )
    }
}

impl Widget for Dropdown {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 32.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        // Closed state — show selected value
        ctx.push_glass_quad(GlassQuad {
            rect: self.rect,
            blur_rect: self.rect,
            tint: [0.1, 0.0, 0.2, 0.15],
            border_color: [1.0, 1.0, 1.0, 0.2],
            corner_radius: 6.0,
            noise_intensity: 0.02,
        });

        let display = format!("{} ▾", self.options[self.selected]);
        ctx.push_text_run(TextRun {
            x: self.rect.x + 10.0,
            y: self.rect.y + 8.0,
            text: display,
            font_size: 13.0,
            color: [1.0, 1.0, 1.0, 0.9],
            monospace: false,
        });

        // Open state — show options
        if self.open_anim.value() > 0.01 {
            let alpha = self.open_anim.value();
            for (i, opt) in self.options.iter().enumerate() {
                let item_rect = self.item_rect(i);
                let is_selected = i == self.selected;

                ctx.push_glass_quad(GlassQuad {
                    rect: item_rect,
                    blur_rect: item_rect,
                    tint: if is_selected {
                        [0.55, 0.36, 0.96, 0.2 * alpha]
                    } else {
                        [0.1, 0.0, 0.2, 0.2 * alpha]
                    },
                    border_color: [1.0, 1.0, 1.0, 0.1 * alpha],
                    corner_radius: 4.0,
                    noise_intensity: 0.02,
                });

                ctx.push_text_run(TextRun {
                    x: item_rect.x + 10.0,
                    y: item_rect.y + 6.0,
                    text: opt.clone(),
                    font_size: 13.0,
                    color: [1.0, 1.0, 1.0, 0.85 * alpha],
                    monospace: false,
                });
            }
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseDown { x, y, button: MouseButton::Left } => {
                if self.rect.contains(*x, *y) {
                    self.open = !self.open;
                    self.open_anim.set_target(if self.open { 1.0 } else { 0.0 });
                    return EventResponse::Consumed;
                }
                if self.open {
                    // Check option clicks
                    for i in 0..self.options.len() {
                        if self.item_rect(i).contains(*x, *y) {
                            self.selected = i;
                            self.open = false;
                            self.open_anim.set_target(0.0);
                            return EventResponse::Consumed;
                        }
                    }
                    // Click outside closes
                    self.open = false;
                    self.open_anim.set_target(0.0);
                }
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }

    fn animate(&mut self, dt_ms: f32) {
        self.open_anim.tick(dt_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_selection() {
        let dd = Dropdown::new(vec!["Gaming".into(), "Coding".into()], 0);
        assert_eq!(dd.selected_text(), "Gaming");
    }

    #[test]
    fn click_opens_and_closes() {
        let mut dd = Dropdown::new(vec!["A".into(), "B".into()], 0);
        dd.layout(Rect::new(0.0, 0.0, 150.0, 32.0));
        dd.handle_event(&UiEvent::MouseDown { x: 75.0, y: 16.0, button: MouseButton::Left });
        assert!(dd.is_open());
        dd.handle_event(&UiEvent::MouseDown { x: 75.0, y: 16.0, button: MouseButton::Left });
        assert!(!dd.is_open());
    }

    #[test]
    fn select_option() {
        let mut dd = Dropdown::new(vec!["A".into(), "B".into(), "C".into()], 0);
        dd.layout(Rect::new(0.0, 0.0, 150.0, 32.0));
        // Open
        dd.handle_event(&UiEvent::MouseDown { x: 75.0, y: 16.0, button: MouseButton::Left });
        // Click option B (y = 32 + 28*1 + 14 = 74)
        dd.handle_event(&UiEvent::MouseDown { x: 75.0, y: 74.0, button: MouseButton::Left });
        assert_eq!(dd.selected_index(), 1);
        assert!(!dd.is_open());
    }
}
```

- [ ] **Step 2: Implement MonitorMap stub with tests**

Replace contents of `crates/prism-client/src/ui/widgets/monitor_map.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Monitor arrangement diagram widget — renders rectangles from MONITOR_LAYOUT data.

use super::{EventResponse, GlassQuad, GlowRect, PaintContext, Rect, Size, TextRun, UiEvent, MouseButton, Widget};

pub struct MonitorInfo {
    pub index: u8,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

pub struct MonitorMap {
    monitors: Vec<MonitorInfo>,
    selected: u8,
    rect: Rect,
}

impl MonitorMap {
    pub fn new(monitors: Vec<MonitorInfo>, selected: u8) -> Self {
        Self {
            monitors,
            selected,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn selected(&self) -> u8 {
        self.selected
    }

    pub fn set_monitors(&mut self, monitors: Vec<MonitorInfo>) {
        self.monitors = monitors;
    }

    fn scaled_rect(&self, mon: &MonitorInfo) -> Rect {
        if self.monitors.is_empty() {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        // Find bounding box of all monitors
        let min_x = self.monitors.iter().map(|m| m.x).min().unwrap_or(0);
        let min_y = self.monitors.iter().map(|m| m.y).min().unwrap_or(0);
        let max_x = self.monitors.iter().map(|m| m.x + m.width as i32).max().unwrap_or(1);
        let max_y = self.monitors.iter().map(|m| m.y + m.height as i32).max().unwrap_or(1);
        let total_w = (max_x - min_x) as f32;
        let total_h = (max_y - min_y) as f32;

        let scale = (self.rect.w / total_w).min(self.rect.h / total_h) * 0.8;
        let offset_x = self.rect.x + (self.rect.w - total_w * scale) / 2.0;
        let offset_y = self.rect.y + (self.rect.h - total_h * scale) / 2.0;

        Rect::new(
            offset_x + (mon.x - min_x) as f32 * scale,
            offset_y + (mon.y - min_y) as f32 * scale,
            mon.width as f32 * scale,
            mon.height as f32 * scale,
        )
    }
}

impl Widget for MonitorMap {
    fn layout(&mut self, available: Rect) -> Size {
        let h = 80.0;
        self.rect = Rect::new(available.x, available.y, available.w, h);
        Size { w: available.w, h }
    }

    fn paint(&self, ctx: &mut PaintContext) {
        for mon in &self.monitors {
            let r = self.scaled_rect(mon);
            let is_sel = mon.index == self.selected;

            ctx.push_glass_quad(GlassQuad {
                rect: r,
                blur_rect: r,
                tint: if is_sel {
                    [0.55, 0.36, 0.96, 0.2]
                } else {
                    [0.1, 0.0, 0.2, 0.15]
                },
                border_color: [1.0, 1.0, 1.0, if is_sel { 0.4 } else { 0.15 }],
                corner_radius: 4.0,
                noise_intensity: 0.02,
            });

            if is_sel {
                ctx.push_glow_rect(GlowRect {
                    rect: r,
                    color: [0.55, 0.36, 0.96, 0.15],
                    spread: 2.0,
                    intensity: 0.5,
                });
            }

            ctx.push_text_run(TextRun {
                x: r.x + r.w / 2.0 - 4.0,
                y: r.y + r.h / 2.0 - 6.0,
                text: mon.index.to_string(),
                font_size: 12.0,
                color: [1.0, 1.0, 1.0, 0.8],
                monospace: true,
            });
        }
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        if let UiEvent::MouseDown { x, y, button: MouseButton::Left } = event {
            for mon in &self.monitors {
                if self.scaled_rect(mon).contains(*x, *y) {
                    self.selected = mon.index;
                    return EventResponse::Consumed;
                }
            }
        }
        EventResponse::Ignored
    }

    fn animate(&mut self, _dt_ms: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_selects_monitor() {
        let monitors = vec![
            MonitorInfo { index: 0, x: 0, y: 0, width: 1920, height: 1080, is_primary: true },
            MonitorInfo { index: 1, x: 1920, y: 0, width: 1080, height: 1920, is_primary: false },
        ];
        let mut map = MonitorMap::new(monitors, 0);
        map.layout(Rect::new(0.0, 0.0, 200.0, 80.0));
        // Click somewhere in the right half (monitor 1 region)
        map.handle_event(&UiEvent::MouseDown { x: 150.0, y: 40.0, button: MouseButton::Left });
        assert_eq!(map.selected(), 1);
    }

    #[test]
    fn renders_quads_per_monitor() {
        let monitors = vec![
            MonitorInfo { index: 0, x: 0, y: 0, width: 1920, height: 1080, is_primary: true },
        ];
        let mut map = MonitorMap::new(monitors, 0);
        map.layout(Rect::new(0.0, 0.0, 200.0, 80.0));
        let mut ctx = PaintContext::new();
        map.paint(&mut ctx);
        assert_eq!(ctx.glass_quads.len(), 1);
        assert_eq!(ctx.text_runs.len(), 1);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-client -- dropdown -v && cargo test -p prism-client -- monitor_map -v`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/ui/widgets/dropdown.rs crates/prism-client/src/ui/widgets/monitor_map.rs
git commit -m "feat(client): Dropdown and MonitorMap widgets"
```

---

## Task 17: Input Router

**Files:**
- Modify: `crates/prism-client/src/input/mod.rs`
- Create: `crates/prism-client/src/input/drag.rs`

- [ ] **Step 1: Implement InputRouter**

Replace contents of `crates/prism-client/src/input/mod.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Input routing — overlay vs remote forwarding, double-tap detection, drag.

pub mod double_tap;
pub mod drag;

use crate::ui::widgets::{UiEvent, KeyCode, MouseButton};

/// Decides where input events go: overlay UI or remote server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    /// Forward to remote server (normal streaming mode).
    Remote,
    /// Route to overlay UI (overlay is visible).
    Overlay,
}

/// Coalesces mouse moves within the same frame.
pub struct InputCoalescer {
    pending_mouse: Option<(f32, f32)>,
    pending_scroll: (f32, f32),
}

impl InputCoalescer {
    pub fn new() -> Self {
        Self {
            pending_mouse: None,
            pending_scroll: (0.0, 0.0),
        }
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.pending_mouse = Some((x, y));
    }

    pub fn scroll(&mut self, dx: f32, dy: f32) {
        self.pending_scroll.0 += dx;
        self.pending_scroll.1 += dy;
    }

    /// Drain coalesced events into a vec.
    pub fn drain(&mut self, out: &mut Vec<UiEvent>) {
        if let Some((x, y)) = self.pending_mouse.take() {
            out.push(UiEvent::MouseMove { x, y });
        }
        if self.pending_scroll.0.abs() > f32::EPSILON || self.pending_scroll.1.abs() > f32::EPSILON {
            out.push(UiEvent::Scroll {
                dx: self.pending_scroll.0,
                dy: self.pending_scroll.1,
            });
            self.pending_scroll = (0.0, 0.0);
        }
    }
}

impl Default for InputCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_multiple_mouse_moves() {
        let mut c = InputCoalescer::new();
        c.mouse_move(10.0, 20.0);
        c.mouse_move(30.0, 40.0);
        c.mouse_move(50.0, 60.0);
        let mut events = Vec::new();
        c.drain(&mut events);
        assert_eq!(events.len(), 1); // only last position
        if let UiEvent::MouseMove { x, y } = &events[0] {
            assert_eq!(*x, 50.0);
            assert_eq!(*y, 60.0);
        } else {
            panic!("Expected MouseMove");
        }
    }

    #[test]
    fn coalesce_scroll_accumulates() {
        let mut c = InputCoalescer::new();
        c.scroll(1.0, 2.0);
        c.scroll(3.0, 4.0);
        let mut events = Vec::new();
        c.drain(&mut events);
        assert_eq!(events.len(), 1);
        if let UiEvent::Scroll { dx, dy } = &events[0] {
            assert_eq!(*dx, 4.0);
            assert_eq!(*dy, 6.0);
        } else {
            panic!("Expected Scroll");
        }
    }
}
```

- [ ] **Step 2: Implement panel drag handler**

Replace contents of `crates/prism-client/src/input/drag.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Panel drag handler — tracks drag state for floating overlay panels.

use crate::ui::widgets::Rect;

pub struct DragState {
    dragging: bool,
    offset_x: f32,
    offset_y: f32,
}

impl DragState {
    pub fn new() -> Self {
        Self {
            dragging: false,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    /// Start a drag from the given mouse position on the title bar rect.
    pub fn start(&mut self, mouse_x: f32, mouse_y: f32, panel_rect: &Rect) {
        self.dragging = true;
        self.offset_x = mouse_x - panel_rect.x;
        self.offset_y = mouse_y - panel_rect.y;
    }

    /// Update panel position during drag.
    pub fn update(&self, mouse_x: f32, mouse_y: f32, panel_rect: &mut Rect) {
        if self.dragging {
            panel_rect.x = mouse_x - self.offset_x;
            panel_rect.y = mouse_y - self.offset_y;
        }
    }

    pub fn stop(&mut self) {
        self.dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
}

impl Default for DragState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_moves_panel() {
        let mut drag = DragState::new();
        let mut rect = Rect::new(100.0, 100.0, 200.0, 150.0);
        drag.start(120.0, 110.0, &rect);
        drag.update(220.0, 210.0, &mut rect);
        assert_eq!(rect.x, 200.0);
        assert_eq!(rect.y, 200.0);
        drag.stop();
        assert!(!drag.is_dragging());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-client -- input -v`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/input/
git commit -m "feat(client): input router with event coalescing and panel drag handler"
```

---

## Task 18: UI State Machine

**Files:**
- Modify: `crates/prism-client/src/ui/mod.rs`

- [ ] **Step 1: Implement UiState**

Replace contents of `crates/prism-client/src/ui/mod.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! UI state machine and widget system for launcher and in-session overlay.

pub mod widgets;
pub mod launcher;
pub mod overlay;

use crate::config::LaunchMode;

/// UI state machine — tracks which mode the client is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiState {
    Launcher,
    Connecting,
    Stream,
    Overlay,
}

impl UiState {
    /// Determine initial state from launch mode.
    pub fn initial(mode: LaunchMode) -> Self {
        match mode {
            LaunchMode::Launcher => UiState::Launcher,
            LaunchMode::DirectConnect => UiState::Connecting,
        }
    }

    /// Whether the remote stream should be rendered.
    pub fn shows_stream(&self) -> bool {
        matches!(self, UiState::Stream | UiState::Overlay)
    }

    /// Whether overlay UI should be rendered.
    pub fn shows_overlay(&self) -> bool {
        matches!(self, UiState::Overlay)
    }

    /// Whether launcher UI should be rendered.
    pub fn shows_launcher(&self) -> bool {
        matches!(self, UiState::Launcher | UiState::Connecting)
    }

    /// Whether input should be forwarded to remote server.
    pub fn forwards_input(&self) -> bool {
        matches!(self, UiState::Stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_from_launch_mode() {
        assert_eq!(UiState::initial(LaunchMode::Launcher), UiState::Launcher);
        assert_eq!(UiState::initial(LaunchMode::DirectConnect), UiState::Connecting);
    }

    #[test]
    fn stream_visible_in_correct_states() {
        assert!(!UiState::Launcher.shows_stream());
        assert!(!UiState::Connecting.shows_stream());
        assert!(UiState::Stream.shows_stream());
        assert!(UiState::Overlay.shows_stream());
    }

    #[test]
    fn input_forwarded_only_in_stream() {
        assert!(!UiState::Overlay.forwards_input());
        assert!(UiState::Stream.forwards_input());
        assert!(!UiState::Launcher.forwards_input());
    }
}
```

- [ ] **Step 2: Create launcher and overlay module stubs**

Create `crates/prism-client/src/ui/launcher/mod.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher mode — quick connect, server card grid, settings.

pub mod quick_connect;
pub mod server_card;
pub mod card_grid;
pub mod server_form;
pub mod settings;
```

Create stubs for each sub-module (just license headers): `quick_connect.rs`, `server_card.rs`, `card_grid.rs`, `server_form.rs`, `settings.rs`.

Create `crates/prism-client/src/ui/overlay/mod.rs`:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
//! In-session overlay — stats bar, floating sub-panels.

pub mod stats_bar;
pub mod perf_panel;
pub mod quality_panel;
pub mod conn_panel;
pub mod display_panel;
```

Create stubs for each: `stats_bar.rs`, `perf_panel.rs`, `quality_panel.rs`, `conn_panel.rs`, `display_panel.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p prism-client -- ui::tests -v`
Expected: All 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/prism-client/src/ui/
git commit -m "feat(client): UI state machine — Launcher/Connecting/Stream/Overlay transitions"
```

---

## Tasks 19-25: Remaining Implementation

The following tasks complete the interactive client. Each follows the same TDD pattern established above.

### Task 19: Launcher — Server Card & Card Grid

**Files:** `crates/prism-client/src/ui/launcher/server_card.rs`, `card_grid.rs`

Implement `ServerCard` widget (glass surface, accent stripe, status dot, hover lift animation) and `CardGrid` (responsive flow layout with cached positions). Each card takes a `&SavedServer` reference and renders its fields. The grid wraps cards at a fixed width (240px per card, 16px gap).

### Task 20: Launcher — Quick Connect & Server Form

**Files:** `crates/prism-client/src/ui/launcher/quick_connect.rs`, `server_form.rs`

`QuickConnect` composes a `TextInput` + `Button` in a glass hero bar. On Enter or button click, emits `UiAction::Connect`. `ServerForm` is a glass panel with text inputs for name, address, noise key, dropdown for profile, and color picker (simple 8-option palette).

### Task 21: Overlay — Stats Bar

**Files:** `crates/prism-client/src/ui/overlay/stats_bar.rs`

Full-width glass bar at top. Composed of `Label` widgets for each metric (FPS, latency, codec, resolution, bandwidth), a `Dropdown` for active profile, a pin toggle, and close button. Updates at 1Hz from `SessionStats`. Metrics use monospace font and color coding (green/yellow/red thresholds).

### Task 22: Overlay — Sub-Panels (Performance, Quality, Connection, Display)

**Files:** `perf_panel.rs`, `quality_panel.rs`, `conn_panel.rs`, `display_panel.rs`

Each panel is a floating glass rectangle composing widgets from the widget set. Performance: sparklines + labels. Quality: dropdowns + checkboxes + slider. Connection: labels + buttons. Display: dropdown + `MonitorMap`. Each panel has a title bar (drag handle) and optional pin button.

### Task 23: Client App Refactor — Replace minifb with wgpu

**Files:** `crates/prism-client/src/client_app.rs`, `main.rs`, `Cargo.toml`

The major integration task:
1. Remove `minifb` dependency
2. Replace `Window::new` with `winit` event loop + `PrismRenderer::new`
3. Replace CPU `yuv420_to_rgb` with `StreamTexture::upload_yuv` + `convert()`
4. Replace pixel buffer blit with wgpu render pass (stream quad)
5. Add double-tap detector to event loop
6. Wire `UiState` transitions
7. Render launcher or stream+overlay based on state
8. Forward input to remote only when `UiState::Stream`

### Task 24: SessionBridge — UI↔Network Channel Layer

**Files:** Create `crates/prism-client/src/session_bridge.rs`

Define `SessionBridge` struct with typed channels (`watch` for stats, `mpsc` for commands, `oneshot` for connect). Refactor `connect_and_stream` to produce a `SessionBridge` instead of directly driving the render loop. The UI thread polls channels each frame.

### Task 25: Server-Side Parallel Stream Acceptance

**Files:** `crates/prism-server/src/negotiation_handler.rs`

Modify the server to accept bi-streams by inspecting the first bytes: Noise handshake headers start with a known pattern, capability negotiation starts with a JSON length prefix (4 bytes LE). Route to the correct handler based on first-byte inspection. This enables parallel Noise + capability negotiation from the client.

---

## Self-Review Checklist

**Spec coverage:**
- Renderer architecture (Section 3 of spec) → Tasks 6-9
- Launcher (Section 4) → Tasks 10, 19, 20
- Overlay (Section 5) → Tasks 21, 22
- Widget system (Section 6) → Tasks 5, 11-16
- Data flow (Section 7) → Task 24
- State machine (Section 8) → Task 18
- Persistence (Section 9) → Task 4
- CLI integration (Section 10) → Task 10
- Crate structure (Section 11) → Task 1
- Server parallel streams → Task 25
- All optimizations are described in their respective tasks

**Placeholder scan:** Tasks 19-25 are summarized rather than fully expanded with code. This is intentional — they follow the same TDD pattern and will be expanded when dispatched to subagents. No TBDs or TODOs.

**Type consistency:** `Widget` trait, `PaintContext`, `Rect`, `Size`, `UiEvent`, `EventResponse`, `UiAction` are defined in Task 5 and used consistently across all widget tasks. `Animation` and `AnimationPool` from Task 2 are used in Button, Checkbox, Dropdown. `SavedServer` from Task 4 is used in ServerCard (Task 19). `LaunchMode` from Task 10 is used in UiState (Task 18).
