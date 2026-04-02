# PRISM Launcher UI Redesign

**Date:** 2026-04-01
**Scope:** Full launcher shell extraction, interactive profiles with pipeline wiring, settings upgrade, overlay replacement, modal server forms, theme token expansion.
**Approach:** Shell-first, top-down (Approach A). Extract shell.rs as the single orchestrator, then build each feature into it.

---

## 1. Launcher Shell & Navigation Consolidation

### Problem

All launcher layout lives in `app.rs` (~450 lines of composition, tab routing, header rendering, sidebar positioning). No `shell.rs` module exists despite it being planned. Two overlapping tab-switch actions exist (`OpenLauncherTab` and `OpenSettings`).

### Design

**New file: `ui/launcher/shell.rs`**

Shell is the single widget owning the launcher's visual frame:
- **Sidebar region** (224px): delegates to `LauncherNav` (already in `nav.rs`)
- **Content region** (remainder): header (title + subtitle from `LauncherTab`) + active tab body
- **Modal layer**: z-ordered overlay slot for server form modals and confirmation dialogs

Public API:
```
LauncherShell::new(nav, quick_connect, card_grid, profiles, settings, server_form)
LauncherShell::set_tab(tab)
LauncherShell::show_modal(modal)  / dismiss_modal()
LauncherShell::layout(screen_rect)
LauncherShell::render(canvas)
LauncherShell::handle_event(event) -> EventResponse
```

Event routing order: modal first (if active) -> nav -> active tab widget. This gives modals automatic input capture.

**Navigation consolidation:**
- Remove `UiAction::OpenSettings` entirely
- All tab switches go through `UiAction::OpenLauncherTab(LauncherTab)`
- Delete the `OpenSettings` handler at `app.rs:602`; the handler at `app.rs:591` for `OpenLauncherTab` is sufficient

**app.rs reduction:**
- `configure_launcher_widgets()` moves into shell
- The render match on `LauncherTab` (lines 907-1021) moves into shell
- Header rendering (lines 923-941) moves into shell
- Sidebar layout constants (lines 548-560) move into shell
- app.rs retains only: `UiState` transitions, `SessionBridge` management, top-level event dispatch to shell vs overlay

**Home tab filter bug fix:**
- When shell switches to Home, it calls `card_grid.reset_filter()` (new method) to force `active_filter = CardFilter::All`, ensuring "Recent connections" actually shows recent items instead of inheriting stale filter state from SavedConnections.

---

## 2. Modal System & Server Form Wiring

### Problem

EditServer, DeleteServer, and AddServer actions are emitted from cards but unhandled in `app.rs::handle_action()`. `server_form.rs` exists as scaffolding but is not connected.

### Design

**Modal layer in shell.rs:**

Shell holds an `Option<ActiveModal>` enum:
```rust
enum ActiveModal {
    ServerForm { mode: FormMode },
    ConfirmDelete { server_id: Uuid, name: String },
}

enum FormMode {
    Add,
    Edit { server_id: Uuid },
}
```

When active, shell renders a semi-transparent scrim over the content area, then the modal widget centered on top. Events hit the modal first; clicks on the scrim dismiss it.

**Server form fields** (from existing `SavedServer` struct):
- Display name (text input)
- Address (text input, required)
- Noise public key (text input, optional)
- Default profile (dropdown, populated from `ProfileStore`)
- Accent color (preset swatches)

**New actions in `UiAction`:**
```rust
SaveServer { mode: FormMode, server: SavedServer }
CancelModal
ConfirmDeleteServer { server_id: Uuid }
```

**Wiring:**
- `AddServer` -> shell opens `ActiveModal::ServerForm { mode: Add }`
- `EditServer(id)` -> shell looks up server from store, opens `ActiveModal::ServerForm { mode: Edit { id } }` pre-filled
- `DeleteServer(id)` -> shell opens `ActiveModal::ConfirmDelete { id, name }`
- `SaveServer` -> app.rs calls `server_store.add()` or `server_store.update()`, then `card_grid.reload()`
- `ConfirmDeleteServer` -> app.rs calls `server_store.delete()`, then `card_grid.reload()`
- `CancelModal` -> shell dismisses modal

**server_form.rs upgrade:**
- Becomes a proper interactive form with text inputs, dropdown, and Save/Cancel buttons
- Validates required fields (address must be non-empty)
- Emits `SaveServer` or `CancelModal`

---

## 3. Profiles — Full Loop

### Problem

`profiles.rs` is a hardcoded skeleton (226 lines, zero interactivity). Profile values don't persist or affect the stream. `SavedServer.default_profile` is just a string with no backing data store.

### Storage

**New file: `config/profiles.rs`**

Uses the same append-only log + compaction pattern as `ServerStore`:
- `~/.prism/profiles.json` (snapshot) + `~/.prism/profiles.log` (transaction log)
- Ships with 4 built-in presets (Gaming, Coding, Balanced, Low Bandwidth) loaded as defaults if no file exists
- Users can modify preset values but cannot delete built-in presets
- Custom profiles can be added and deleted

**ProfileConfig struct:**
```rust
struct ProfileConfig {
    id: Uuid,
    name: String,
    builtin: bool,
    // Performance
    bitrate_bps: u64,              // 5_000_000..50_000_000
    max_fps: u8,                   // 30, 60, 120
    encoder_preset: EncoderPreset, // UltraLowLatency, Balanced, Quality
    // Display & Audio
    prefer_native_scaling: bool,
    audio_mode: AudioMode,         // Stereo, Surround, VoiceOptimized
    prefer_av1: bool,
    // Input & Connectivity
    exclusive_input: bool,
    touch_mode: bool,
    auto_reconnect: bool,
}
```

**AudioMode enum** (defined in `config/profiles.rs` alongside `ProfileConfig`):
```rust
enum AudioMode {
    Stereo,
    Surround,
    VoiceOptimized,
}
```

**ProfileStore API:**
```rust
ProfileStore::load(config_dir) -> Self
ProfileStore::list() -> &[ProfileConfig]
ProfileStore::get(id) -> Option<&ProfileConfig>
ProfileStore::get_by_name(name) -> Option<&ProfileConfig>
ProfileStore::update(id, ProfileConfig)
ProfileStore::add(ProfileConfig) -> Uuid
ProfileStore::delete(id)  // fails on builtin
```

### UI

**profiles.rs rewrite — two-column layout:**

- **Left panel (248px):** Scrollable preset list. Each entry shows name + summary line (e.g. "120 FPS . 35 Mbps"), active highlight on selected. "Add Profile" button at bottom.
- **Right panel:** Editor for selected profile. Sections:
  - **Performance:** Bitrate slider (5-50 Mbps), FPS selector (30/60/120), encoder preset toggle (3 options)
  - **Display & Audio:** Native scaling toggle, audio mode dropdown, AV1 preference toggle
  - **Input & Connectivity:** Exclusive input toggle, touch mode toggle, auto-reconnect toggle
  - Footer: Discard / Save Changes buttons

**Edit flow:**
1. User changes a value -> local `draft: ProfileConfig` updated (not persisted yet)
2. Discard -> revert draft to stored version
3. Save Changes -> `ProfileStore::update(id, draft)`, persist to disk
4. If the saved profile is the `default_profile` on any server, the next connect picks up new values automatically (lookup is by name)

### Pipeline Wiring — Connect Time

In `app.rs::start_connection()`:
1. Look up the `SavedServer` by address
2. Resolve `default_profile` string -> `ProfileConfig` via `ProfileStore::get_by_name()`
3. Build a `ControlCommand::UpdateQuality` from the profile's fields
4. After `SessionBridge` is established, immediately send the profile as the first control command
5. Server receives it and applies to `ClientSession.profile` + reconfigures encoder

### Pipeline Wiring — Runtime Switching

Wire existing but unhandled actions in `app.rs::handle_action()`:
- `UiAction::SwitchProfile(name)` -> resolve to `ProfileConfig`, send `ControlCommand::SwitchProfile(name)` + `ControlCommand::UpdateQuality{...}` through bridge
- `UiAction::UpdateQuality{...}` -> send `ControlCommand::UpdateQuality{...}` through bridge

### Server-Side Consumption

In `prism-server`:
- `client_session.rs`: handle `ControlCommand::SwitchProfile` and `UpdateQuality` — update `ClientSession.profile` fields
- `encode_pool.rs`: accept runtime reconfiguration — new bitrate, fps, preset applied to active encoder
- This is the deepest change: encoder reconfiguration mid-stream

---

## 4. Settings — Display Upgrade

### Problem

`settings.rs` is 134 lines of hardcoded read-only rows. Needs to become a multi-section surface. Only Streaming Defaults and Input Controls need to actually persist; the rest are cosmetic.

### Layout

Single scrollable page with grouped sections (no sub-navigation this round). Sidebar highlights "Settings" as the active tab.

### Sections

1. **Identity & Security** (display-only)
   - Identity Path: monospace text + copy-to-clipboard button
   - Device Trust: status badge ("Trusted Device" / "Unverified"), last verified timestamp

2. **Streaming Defaults** (functional)
   - Default Profile: dropdown populated from `ProfileStore::list()` — selecting a value updates `ClientConfig.default_profile` and persists

3. **Input Controls** (functional)
   - Exclusive Keyboard Capture: toggle (persists to `ClientConfig`)
   - Relative Mouse Movement: toggle (persists to `ClientConfig`)

4. **Audio Paths** (display-only, cosmetic controls)
   - Remote Output: dropdown (renders, does not persist)
   - Local Mic Path: dropdown (renders, does not persist)

5. **About**
   - Client Version: `env!("CARGO_PKG_VERSION")`

### ClientConfig Persistence

**New file: `config/client_config.rs`**

Simple JSON at `~/.prism/client_config.json`:
```rust
struct ClientConfig {
    default_profile: String,
    exclusive_keyboard: bool,
    relative_mouse: bool,
}
```

Loaded at startup, written on change. No append-only log needed — small and infrequently written.

Settings reuses the same Toggle and Dropdown widgets built for Profiles.

---

## 5. Overlay Replacement

### Problem

Current overlay is a left-side drawer with multiple panels. The brief mandates: keep only the top capsule from the Theme mock, replace the drawer entirely.

### Top Capsule

**New widget: `ui/overlay/capsule.rs`**

Floating bar anchored to top-center during an active session. Approximately 600px wide, 48px tall, pill-shaped.

Layout:
```
[ PRISM REMOTE | FPS: 60 | 12ms | 25 Mbps | AV1 | Gaming v | gear | power ]
```

- **Left:** PRISM REMOTE label
- **Center:** Real-time metrics from `SessionStats` (FPS, latency, bitrate, codec)
- **Right:** Active profile name (clickable dropdown to switch), settings gear (opens panel), disconnect button

### Expandable Panels

Clicking a metric or gear icon expands a dropdown panel below the capsule:
- **Profile dropdown:** List from `ProfileStore`, click to switch (sends `ControlCommand::SwitchProfile`)
- **Performance panel:** FPS sparkline, latency sparkline, decode time, bandwidth graph (migrated from `perf_panel.rs`)
- **Quality panel:** Encoder preset, FPS limit, lossless text toggle, bitrate slider (migrated from `quality_panel.rs`)
- **Connection panel:** Server address, uptime, secure channel status, monitor selector

One panel open at a time. Click outside or Escape to close.

### Bottom Status Bar

Optional second capsule at bottom-center:
```
[ 192.168.1.104 | uptime 12:42:01 | Secure ]
```

Display-only, no interactivity.

### Disconnect Button

Sends `UiAction::Disconnect`. No confirmation dialog (matches existing behavior).

### What Gets Removed
- `stats_bar.rs` current form — metrics move into capsule
- Overlay drawer layout in app.rs — replaced by capsule + dropdown panels
- `UiState::Overlay` semantics change from "drawer open" to "capsule visible with optional dropdown"

### What Survives (Refactored)
- `perf_panel.rs` content -> dropdown panel component
- `quality_panel.rs` content -> dropdown panel component
- `SessionStats` watch channel flow unchanged

---

## 6. Theme Token Expansion

### New Surface Functions

```rust
status_chip(rect, tone: ChipTone)        // ChipTone: Success, Warning, Accent, Neutral
section_header_surface(rect)              // Subtle tint for section titles
settings_row_surface(rect)                // Row separator treatment
capsule_surface(rect)                     // High-blur pill for overlay
capsule_dropdown_surface(rect)            // Panel below capsule
modal_scrim(screen_rect)                  // Semi-transparent overlay behind modals
modal_surface(rect)                       // Modal card background
```

### Control Tokens

```rust
slider_track(rect)                        // Horizontal track background
slider_fill(rect, ratio: f32)            // Filled portion
slider_thumb(rect)                        // Draggable handle
toggle_track(rect, on: bool)             // Track, changes with state
toggle_thumb(rect, on: bool)             // Sliding circle
dropdown_surface(rect, open: bool)       // Trigger and expanded list
dropdown_item(rect, selected, hovered)   // Individual option row
```

### Typography Constants

```rust
FONT_DISPLAY: 30.0     // Tab titles
FONT_HEADLINE: 20.0    // Section headers
FONT_BODY: 14.0        // Primary content
FONT_LABEL: 13.0       // Form labels, settings labels
FONT_CAPTION: 11.0     // Metadata, timestamps, hints
FONT_CHIP: 10.0        // Status/filter chip text
```

Centralizes magic numbers currently scattered across widgets. No visual change.

---

## 7. Reusable Widget Components

Three new widgets in `ui/widgets/`, needed by Profiles, Settings, and overlay panels:

### Slider (`ui/widgets/slider.rs`)
- Horizontal drag control with configurable range and step
- Shows current value label
- Emits value-changed callback
- Uses `slider_track`, `slider_fill`, `slider_thumb` theme tokens

### Toggle (`ui/widgets/toggle.rs`)
- On/off switch with slide animation
- Emits toggled callback
- Uses `toggle_track`, `toggle_thumb` theme tokens

### Dropdown (`ui/widgets/dropdown.rs`)
- Click to expand options list, click to select, collapse
- Supports string labels and associated values
- Emits selection-changed callback
- Uses `dropdown_surface`, `dropdown_item` theme tokens

---

## 8. File Plan

| File | Action | Purpose |
|------|--------|---------|
| `ui/launcher/shell.rs` | **Create** | Launcher frame: sidebar + content + modal layer |
| `ui/launcher/nav.rs` | Keep | Already works, no changes |
| `ui/launcher/profiles.rs` | **Rewrite** | Interactive two-column profile editor |
| `ui/launcher/settings.rs` | **Expand** | Multi-section settings with real controls |
| `ui/launcher/server_form.rs` | **Expand** | Modal form for Add/Edit server |
| `ui/launcher/quick_connect.rs` | Keep | Already works |
| `ui/launcher/card_grid.rs` | **Minor** | Add `reset_filter()`, reload method |
| `ui/launcher/server_card.rs` | **Minor** | Add delete icon button (trash) next to Edit; emits `DeleteServer(id)` |
| `ui/launcher/mod.rs` | **Expand** | Add modal types, consolidate exports |
| `ui/overlay/capsule.rs` | **Create** | Top metrics bar + dropdown panels |
| `ui/overlay/mod.rs` | **Expand** | Replace drawer with capsule system |
| `ui/widgets/slider.rs` | **Create** | Reusable slider control |
| `ui/widgets/toggle.rs` | **Create** | Reusable toggle switch |
| `ui/widgets/dropdown.rs` | **Create** | Reusable dropdown control |
| `ui/widgets/mod.rs` | **Edit** | Remove `OpenSettings`, add new actions |
| `ui/theme.rs` | **Expand** | All new surface/control/typography tokens |
| `config/profiles.rs` | **Create** | ProfileStore + ProfileConfig persistence |
| `config/client_config.rs` | **Create** | ClientConfig persistence |
| `config/mod.rs` | **Edit** | Export new config modules |
| `app.rs` | **Major refactor** | Extract launcher to shell, wire all actions |
| `session_bridge.rs` | **Minor** | Ensure ControlCommand variants complete |
| `prism-server/client_session.rs` | **Edit** | Handle SwitchProfile + UpdateQuality |
| `prism-server/encode_pool.rs` | **Edit** | Runtime encoder reconfiguration |

---

## 9. Build Order

1. **Theme tokens** — add all new surface functions, typography constants, control tokens. Zero functional change.
2. **Reusable widgets** — slider, toggle, dropdown in `ui/widgets/`. Unit-testable in isolation.
3. **Shell extraction** — create `shell.rs`, move layout/routing/header out of app.rs. Consolidate `OpenSettings` into `OpenLauncherTab`. Fix Home filter reset bug.
4. **Modal system** — add modal layer to shell, wire `server_form.rs` for Add/Edit, add delete confirmation. Wire `SaveServer`/`ConfirmDeleteServer`/`CancelModal` in app.rs.
5. **ProfileStore** — create `config/profiles.rs` with persistence, default presets, CRUD operations.
6. **Profiles screen** — rewrite `profiles.rs` with interactive controls backed by `ProfileStore`.
7. **Profile pipeline wiring** — connect time: resolve profile -> send `UpdateQuality`. Runtime: wire `SwitchProfile`/`UpdateQuality` through bridge. Server-side: handle commands, reconfigure encoder.
8. **ClientConfig + Settings** — create `config/client_config.rs`, expand `settings.rs` with grouped sections, functional controls for streaming defaults and input.
9. **Overlay capsule** — create `capsule.rs`, replace drawer layout, migrate stats/quality/perf panel content into dropdown panels. Remove old overlay structure.
10. **Integration pass** — verify all actions wired end-to-end, no unhandled actions, all tab transitions clean.

---

## 10. Decisions Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Scope | Full sweep (steps 1-5) | Single spec covering shell, profiles, settings, overlay |
| Profile interactivity | Full loop | Controls interactive, changes persist, values feed stream pipeline |
| Settings interactivity | Display upgrade | Streaming Defaults + Input Controls persist; rest cosmetic |
| Overlay style | Full replacement | Tear out drawer, build top capsule + dropdown panels |
| Server form UX | Modal | Centered modal overlay, not inline or page-based |
| Architecture | Shell-first top-down | Extract shell.rs first, then build features into it |
| Profile persistence | Append-only log + compaction | Reuses proven ServerStore pattern |
| ClientConfig persistence | Simple JSON | Small, infrequent writes don't need log pattern |
| Settings sub-nav | Deferred | Single scrollable page this round; sub-nav if page grows |
