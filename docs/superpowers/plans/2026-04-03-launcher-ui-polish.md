---
goal: "Launcher UI Layout & Geometry Polish — Bridge remaining gaps between Implementation.png and screen.png"
version: 2.1
date_created: 2026-04-03
last_updated: 2026-04-03
owner: PRISM Client Team
status: 'Planned'
tags: [feature, design, ui, launcher, polish]
---

# Introduction

![Status: Planned](https://img.shields.io/badge/status-Planned-blue)

> **Prior art:** `docs/superpowers/plans/2026-04-02-launcher-light-theme-fix.md`
> established the foundational light-mode color system — `LAUNCHER_BACKDROP`,
> `launcher_sidebar_surface()`, `LT_TEXT_*` palette, `PRIMARY_BLUE`, and
> `ColorMode::Light` widget support.

**Problem:**
The 2026-04-02 plan successfully flipped the launcher from dark to light mode,
but comparing `Implementation.png` to the Stitch target `screen.png` reveals
several critical visual and structural gaps:

1. **Typography** — All text renders at a single weight. Bold/semibold variants
   specified in the design are not being applied.
2. **Button color** — The "Connect" button renders as light cyan instead of
   `#0F6CBD` corporate blue.
3. **Quick Connect layout** — Inputs stretch to fill available width instead of
   being constrained and centered within the hero panel.
4. **Sidebar geometry** — Renders as a floating rounded card with margins
   instead of an edge-to-edge structural panel flush against the window frame.
5. **Recent Connections** — Only the empty state is shown; no data rendering loop.
6. **Icons** — Still deferred; sidebar nav, header bar, and section decorators
   are all text-only.

**Goal:** Systematically close each gap with targeted changes to layout
geometry, typography, data rendering, and a new icon rendering primitive.

---

## Progress Tracker

Check off each phase/pass as it is completed. Refer to **Section 10 — Recommended Execution Order** for task ranges.

### Passes

- [x] **Pass 0 — Data Prerequisites** (Phase 0: TASK-P01 → TASK-P06)
- [x] **Pass 1 — Foundations** (Phases 1–3: TASK-002 → TASK-015)
- [x] **Pass 2 — Icons & Header** (Phases 5–6: TASK-024 → TASK-033)
- [ ] **Pass 3 — Data & Screens** (Phases 4, 7–9: TASK-016 → TASK-023, TASK-034 → TASK-064)
- [ ] **Pass 4 — Settings & Polish** (Phase 10: TASK-066 → TASK-081)
- [ ] **Verification** (Phase 11: TASK-082 → TASK-086a)

### Phases

- [x] Phase 0 — Data-Layer Prerequisites (6 tasks: P01–P06)
- [x] Phase 1 — Bold Text Support (3 tasks: 002–004)
- [x] Phase 2 — Primary Button Color Fix (3 tasks: 005–007)
- [x] Phase 3 — Sidebar Geometry Overhaul (7 tasks: 008–015)
- [ ] Phase 4 — Home Screen: Recent Connections (4 tasks: 016–019)
- [x] Phase 5 — Icon Rendering Primitive (9 tasks: 021–025b)
- [x] Phase 6 — Sidebar Nav Icons & Header Bar (8 tasks: 026–033)
- [ ] Phase 7 — Saved Connections: Filter Bar & Card Grid (11 tasks: 034–044)
- [ ] Phase 8 — Saved Connections: FAB & Card Polish (6 tasks: 045–050)
- [ ] Phase 9 — Profiles Editor Polish (14 tasks: 051–064)
- [ ] Phase 10 — Settings Panel Polish (16 tasks: 066–081)
- [ ] Phase 11 — Cross-Screen Verification (6 tasks: 082–086a)

---

## Screen-by-Screen Gap Analysis

> Each section compares the current implementation (`Theme/*/Implementation.png`)
> against the Stitch design target (`Theme/*/screen.png` + `code.html`).
> The 2026-04-02 plan fixed the systemic dark-to-light color issue; this plan
> addresses the remaining **layout, typography, geometry, data, and icon** gaps.

### Screen 1: Home

**Implementation (current):**
- Light blue gradient background ✓ (fixed by prior plan)
- Cream sidebar with "PRISM" / "Remote client" text branding, no icons
- Sidebar is a **floating rounded card** with 28px padding from all edges and 28px corner radius
- Active nav item is a **floating rounded pill** with side margins inside the sidebar
- "Home" title + "Connect instantly..." subtitle in content area at `FONT_DISPLAY`
- White frosted glass hero panel with "Quick Connect" title ✓
- **Subtitle** "Enter a hostname or IP address" below hero title — not in design
- Input and Connect button **stretch full width** (minus 32px padding) inside hero panel
- Input and button use **fully-rounded pill shape** (maximum border-radius)
- Connect button is `PRIMARY_BLUE` ✓ — but still rendering as a **lighter cyan** in practice (needs audit)
- "Recent Connections" section header at 13px — **no populated rows**, only empty state "No saved desktops match this filter."
- Separator line below section header
- No icons anywhere

**Design target (Stitch `screen.png`):**
- Sidebar is **edge-to-edge flush** against top/left/bottom window edges — no margins, no rounded corners on left side
- Sidebar top shows **hamburger menu icon** (`≡`) only — no "PRISM" / "Remote client" text
- Nav items have **Material Symbols icons** to the left of labels (Home 🏠, Saved Connections 🔖, Profiles 📁)
- Active "Home" item has **edge-to-edge flush rectangle** with `PRIMARY_BLUE` left-border bar — no floating pill
- Settings shown at sidebar bottom with ⚙ icon
- **Header bar** at top right edge: "Home" title (bold) + spacer + PRISM gem icon + "PRISM" text + user avatar circle + window controls
- No subtitle under "Home" in header bar
- Hero panel "Quick Connect" in **bold** weight — centered, no subtitle below it
- Input field is **constrained width** (~480px), centered within the hero panel, with **standard 8px rounded-rectangle** corners (not pills)
- Connect button matches input width, **standard 8px corners**, solid `#0F6CBD` blue
- "Recent Connections" as a **bold** section heading at larger font size
- White frosted list container with **3 populated rows**: numbered index + bold server name + status chip (Online/Sleeping/Unreachable) + "Last connected" timestamp + outlined "Reconnect" button with refresh icon
- Subtle inner separators between rows

**Key gaps:**

| # | Element | Current (Implementation.png) | Target (screen.png) | Fix (Phase:Task) |
|---|---------|-----|--------|-----|
| 1 | Sidebar origin | Floating card at `(28, 28)` with `SIDEBAR_RADIUS=28` | Edge-to-edge at `(0, 0)` with `radius: 0.0` | Phase 3: TASK-009, TASK-010 |
| 2 | Sidebar branding | "PRISM" / "Remote client" text | Hamburger `≡` icon only | Phase 3: TASK-012b (conditional — uses `Icon` if Phase 5 has landed) |
| 3 | Nav item icons | Text-only labels | Material Symbols icons + labels | Phase 6: TASK-028 |
| 4 | Active nav style | Floating rounded pill (`CONTROL_RADIUS`, `SIDE_PADDING`) | Edge-to-edge flush rect + 4px `PRIMARY_BLUE` left bar | Phase 3: TASK-012a |
| 5 | Header bar | Title + subtitle float in content area | Dedicated header bar with title + PRISM logo + avatar | Phase 6: TASK-029, TASK-030 |
| 6 | Hero title weight | Regular weight `FONT_HERO` | **Bold** weight | Phase 1: TASK-003 |
| 7 | Hero subtitle | "Enter a hostname or IP address" visible | **Removed** — not in design | Phase 1: TASK-003a |
| 8 | Input/button width | Stretches full hero width (minus padding) | Constrained ~480px, centered | Phase 2: TASK-007 |
| 9 | Input/button shape | Fully-rounded pill | Standard 8px rounded rectangle | Phase 2: TASK-007a |
| 10 | Connect button color | May still render cyan (needs audit) | Solid `#0F6CBD` `PRIMARY_BLUE` | Phase 1: TASK-004 |
| 11 | Section header weight | Regular weight 13px | **Bold** larger font | Phase 1: TASK-003 |
| 12 | Connections list | Empty state only | Populated rows with status chips, timestamps, buttons | Phase 4: TASK-014–019 |
| 13 | Settings position | Separate nav item at sidebar bottom | Same position, but with ⚙ icon | Phase 6: TASK-028 |
| 14 | Sidebar footer | Empty | Avatar + "Verified Dev" | Phase 10: TASK-074 |

---

### Screen 2: Saved Connections

**Implementation (current):**
- Light blue gradient background ✓
- Same floating rounded sidebar with text-only nav, no icons
- "Saved Connections" title + subtitle at `FONT_DISPLAY` / `FONT_BODY`
- Filter chip bar: "All Hosts (0)" active in solid `PRIMARY_BLUE` pill ✓, "Recent"/"Dormant"/"New" as outlined chips ✓
- No right-aligned Filter/Sort buttons
- Empty state: "No saved desktops match this filter." — no populated cards
- "Add New Connection" card: white glass, solid border, blue "+" circle, "Add New Connection" + "Manual IP or Network Discovery" text — positioned as the only grid item (not last)
- No FAB (floating action button) circle

**Design target (Stitch `screen.png`):**
- Edge-to-edge sidebar with icons, hamburger top, Settings at bottom
- Active "Saved Connections" with blue left-border indicator
- Filter chips: "All Hosts (14)" solid blue, "Work"/"Personal"/"Linux Servers" as outlined pills (tag-based)
- Right-aligned "Filter" and "Last Connected" buttons with icons
- **3-column card grid** with rich data cards:
  - Top half: hero image (desktop screenshot), ONLINE/SLEEPING/UNREACHABLE status badge, WORK/PERSONAL tag badge, heart ♥ icon
  - Body: bold server name + kebab menu (⋮), OS + IP subtitle, timestamp + latency/WOL badge
  - Footer: contextual button (Connect/Wake & Connect/Retry Discovery) + edit icon button
  - Unreachable card is visually dimmed (reduced opacity, muted text)
- **Dashed border** "Add New Connection" card as **last** grid item
- Blue circular **FAB** button ("+") in bottom-right corner

**Key gaps:**

| # | Element | Current | Target | Fix (Phase:Task) |
|---|---------|---------|--------|-----|
| 1 | Sidebar/header | Same floating sidebar, no header bar | Edge-to-edge sidebar + header bar with title | Phase 3 + Phase 6 |
| 2 | Page subtitle | In content area below title | Below header bar, above filter pills | Phase 7: TASK-035 |
| 3 | Filter chips (tags) | Time-based only (Recent/Dormant/New) | Tag-based too (Work/Personal/Linux Servers) | Phase 0: TASK-P04 |
| 4 | Filter/Sort buttons | Missing | Right-aligned "Filter" + "Last Connected" | Phase 7: TASK-037 |
| 5 | Card grid layout | Single item only | 2-3 column responsive grid | Phase 8: TASK-039 |
| 6 | Card hero area | No hero image area | Placeholder gradient top ~55% | Phase 8: TASK-041 |
| 7 | Card badges | Status chip only, no tags | Status + tag + heart overlays on hero | Phase 8: TASK-042 |
| 8 | Card body | Name + address + time | Name + kebab ⋮ + OS/IP + time + latency | Phase 8: TASK-043 |
| 9 | Card action buttons | Connect/Edit/Delete uniform | Contextual per status + edit icon | Phase 8: TASK-044 |
| 10 | Unreachable dimming | Same opacity as others | `opacity-80`, muted title, disabled button | Phase 8: TASK-045 |
| 11 | Add card position | Only grid item (centered) | **Last** item in populated grid | Phase 8: TASK-039 |
| 12 | Add card border | Solid border | **Dashed** white border | Phase 8: TASK-040 |
| 13 | FAB button | Missing | Blue "+" circle, bottom-right | Phase 8: TASK-046 |
| 14 | Scroll support | No scroll | Vertical scroll for many cards | Phase 8: TASK-047a |
| 15 | Server data fields | Only `display_name`, `address`, `last_connected` | + `os_label`, `tags`, `last_latency_ms`, `wol_supported` | Phase 0: TASK-P03 |

---

### Screen 3: Profiles

**Implementation (current):**
- Light blue gradient background ✓
- Same floating sidebar
- "Profiles" title + subtitle
- Two-column layout: left preset list (248px), right editor panel ✓
- Left list: "Presets" header **inside** the white list card, 4 items (Gaming/Coding/Balanced/Low Bandwidth) with name + "Built-in • XX FPS • XX Mbps" subtitle
- Selected item: light gray highlight with blue left accent bar ✓ — no icon, no "Active" dot
- Right editor: "Gaming" name at `FONT_HERO`, "SYSTEM" chip (accent blue), Discard/Save buttons (both full-width styled)
- Description text below name
- Controls in two columns: Bitrate slider (label + "XX Mbps" value), Max FPS dropdown, Latency vs Quality segmented, Audio Mode dropdown, Native Scaling/Exclusive Input/Prefer AV1/Touch Mode/Auto Reconnect toggles
- Slider shows "Bitrate" label and "45 Mbps" value at 12px, no range labels
- Segmented control: each option is a separated colored pill (**not** unified container)
- Section labels are plain text at `FONT_CAPTION` — no grouping headers, no icons
- No profile-type icons in list items

**Design target (Stitch `screen.png`):**
- Edge-to-edge sidebar with icons, active "Profiles" with blue left-border
- Header bar with "Profiles" title + **search input** ("Search profiles...") with magnifying glass icon
- "Presets" label + blue "+" button **above** the white list card (on gradient)
- List items have **profile-type icons** (gamepad, code brackets, balance, speed) at ~20px in `PRIMARY_BLUE`
- Active item: pure **white** bg + blue left-border + **green "Active" dot** + "Active" text
- Editor: large profile icon (~32px) + "Gaming" bold + "ACTIVE" **green** chip (not blue "SYSTEM")
- "Save Changes" is a **sharp-rectangle** `PRIMARY_BLUE` button; "Discard" is **text-only** (no background)
- **Section grouping headers**: "PERFORMANCE SETTINGS" / "DISPLAY & AUDIO" / "INPUT & CONNECTIVITY" with blue icons, uppercase, letter-spacing
- Bitrate: large **"35 MBPS"** display (number in `PRIMARY_BLUE` at display size + "MBPS" suffix) + range labels "5 MBPS" / "50 MBPS" below slider
- Latency vs Quality: **unified container** segmented control with solid blue active block
- Dropdowns in **2-column grid** with chevron icons
- Toggle cards in **2-column grid** wrapped in `launcher_toggle_card_surface(0.60)` cards with title + description + toggle

**Key gaps:**

| # | Element | Current | Target | Fix (Phase:Task) |
|---|---------|---------|--------|-----|
| 1 | Sidebar/header | Floating sidebar, no search | Edge-to-edge + header search bar | Phase 3 + Phase 9: TASK-058 |
| 2 | "Presets" label position | Inside list card | **Above** list card, on gradient bg | Phase 9: TASK-048 |
| 3 | "+" add profile button | Missing | Blue circle next to "Presets" | Phase 9: TASK-048 |
| 4 | List item icons | None | Profile-type icons (gamepad etc.) | Phase 9: TASK-049 |
| 5 | Active item styling | Gray bg, no dot | White bg + green "Active" dot | Phase 9: TASK-050 |
| 6 | Editor header icon | None | Large profile icon ~32px | Phase 9: TASK-051 |
| 7 | Chip label | "SYSTEM" (blue) | "ACTIVE" (green) | Phase 9: TASK-051 |
| 8 | Save button shape | Rounded, full | Sharp rectangle (~4px radius) | Phase 9: TASK-052 |
| 9 | Discard button | Styled secondary button | Text-only (no bg/border) | Phase 9: TASK-052 |
| 10 | Section grouping | Flat labels, no grouping | Uppercase headers with icons + separators | Phase 9: TASK-053 |
| 11 | Segmented control | Individual colored pills | Unified container + solid blue active block | Phase 9: TASK-054 |
| 12 | Dropdown layout | Single column | **2-column grid** with chevrons | Phase 9: TASK-055 |
| 13 | Toggle cards | Bare toggles, no cards | Cards with title + description + toggle | Phase 9: TASK-056 |
| 14 | Bitrate display | Small "45 Mbps" text | Large "35" in blue + "MBPS" suffix + range labels | Phase 9: TASK-059 |
| 15 | Editor scroll | No scroll support | Vertical scroll when content overflows | Phase 9: TASK-060 |

---

### Screen 4: Settings

**Implementation (current):**
- Light blue gradient background ✓
- Same floating sidebar, "Settings" tab active as bottom nav item
- "Settings" title + "Review identity, defaults..." subtitle
- "Identity & Security" as section header inside white glass card (correct) ✓
- Content in single scrollable pane: Identity Path (monospace blue text in badge), Device Trust ("Trusted Device" green chip), Streaming Defaults (full-width dropdown, **gray/dark-style rendering**), Input toggles (two `launcher_toggle_card_surface(0.30)` rows ✓), Audio (two dropdowns with "REMOTE OUTPUT" / "LOCAL MIC PATH" labels)
- **"System Default"** stray dropdown visible below Relative Mouse Movement toggle
- Version watermark at bottom ✓
- No sidebar sub-navigation
- No breadcrumb header
- No "Last verified" metadata next to trust chip
- No sidebar footer avatar

**Design target (Stitch `screen.png`):**
- Edge-to-edge sidebar with icons
- Sidebar header: `≡` + PRISM gem icon + "PRISM" text (Settings-specific branding)
- **Sidebar sub-navigation**: "SETTINGS" header label + section items (General, Identity & Security, Streaming, Input, Audio) with icons, active item has blue left-border
- **Breadcrumb header**: "Settings / **Identity & Security**" — "Settings /" in muted, section name in bold
- Content title "Identity & Security" bold + subtitle "Manage your digital footprint..."
- White glass card wrapping all content
- Identity Path in lavender-tinted field
- Device Trust: green dot + "Trusted Device" chip + *"Last verified 2 hours ago"* italic metadata
- Streaming Defaults: **constrained-width** dropdown with `ColorMode::Light` and chevron icon
- Input: toggle cards with title + **description** + toggle
- Audio: **"REMOTE OUTPUT"** / **"LOCAL MIC PATH"** uppercase labels above dropdowns, headphone 🎧 and mic 🎤 icons inside dropdowns
- Sidebar footer: avatar circle + "Verified Dev" name
- No stray dropdown below mouse toggle

**Key gaps:**

| # | Element | Current | Target | Fix (Phase:Task) |
|---|---------|---------|--------|-----|
| 1 | Sidebar sub-nav | Missing | "SETTINGS" header + 5 section items with icons | Phase 10: TASK-066 |
| 2 | Sub-nav active state | N/A | Blue left-border on active section | Phase 10: TASK-067 |
| 3 | Breadcrumb header | Missing | "Settings / **Identity & Security**" | Phase 10: TASK-068 |
| 4 | Content title | Exists but may lack subtitle update | Updated subtitle text | Phase 10: TASK-069 |
| 5 | Sidebar Settings branding | "PRISM" / "Remote client" | `≡` + gem + "PRISM" (Settings only) | Phase 6: TASK-032 |
| 6 | Streaming dropdown | Full-width, may be dark-styled | Constrained width, `ColorMode::Light`, chevron | Phase 10: TASK-070 |
| 7 | Trust metadata | Missing | *"Last verified 2 hours ago"* italic | Phase 10: TASK-071 |
| 8 | Toggle card descriptions | Title only | Title + description text | Phase 10: TASK-072 |
| 9 | Stray dropdown | "System Default" below mouse toggle | **Removed** | Phase 10: TASK-072 |
| 10 | Audio icons | No icons in dropdowns | 🎧 / 🎤 icons inside right edge | Phase 10: TASK-073 |
| 11 | Sidebar footer avatar | Missing | Avatar circle + "Verified Dev" on all tabs | Phase 10: TASK-074 |

---

### Cross-Screen Summary

**These issues affect ALL screens identically:**

| # | Systemic Issue | Current State | Target State | Fix |
|---|---------------|---------------|-------------|-----|
| A | Sidebar geometry | Floating card at `(28,28)` with `SIDEBAR_RADIUS=28` | Edge-to-edge at `(0,0)` with `radius: 0.0` | Phase 3 |
| B | Sidebar branding | "PRISM" / "Remote client" text | `≡` hamburger icon (+ branding on Settings) | Phase 3 + Phase 6 |
| C | Nav icons | Text-only labels | Material Symbols icons at 20px | Phase 5 + Phase 6 |
| D | Active nav style | Floating rounded pill | Edge-to-edge flush rect + blue left bar | Phase 3 |
| E | Header bar | Title floats in content area | Dedicated header bar above content | Phase 6 |
| F | Typography | Single weight (regular) | Bold/semibold for titles, headers, buttons | Phase 1 |
| G | Icon primitive | No icon widget at all | `.ttf` icon font through text pipeline | Phase 5 |
| H | Sidebar footer | Empty | Avatar + "Verified Dev" identity label | Phase 10 |

---

## 1. Requirements & Constraints

- **REQ-001**: Font rendering must support Bold and Semibold weight variants for the hero title, section headers, and button labels.
- **REQ-002**: `ButtonStyle::Primary` + `ColorMode::Light` must resolve to `PRIMARY_BLUE` (`#0F6CBD`) background with white text.
- **REQ-003**: Quick Connect inputs must be horizontally centered within the hero panel with a maximum width proportional to `screen.png` (~480px).
- **REQ-004**: Sidebar must render edge-to-edge: flush against top, left, and bottom window edges with zero corner radius on left corners.
- **REQ-005**: Recent Connections must render populated rows using `launcher_list_surface()`, `launcher_row_surface()`, and `launcher_status_chip()`.
- **REQ-006**: An icon rendering primitive must exist in `ui/widgets/` to unblock sidebar icons, header bar, and section decorators.
- **REQ-007**: The page title (e.g. "Home") must sit in the top header bar, horizontally aligned with the PRISM logo/avatar on the right — not floating inside the content area. The subtitle ("Connect instantly...") must be removed entirely.
- **REQ-008**: The sidebar top must show a hamburger menu icon (`ICON_MENU` / `≡`) instead of the "PRISM / Remote client" text branding.
- **REQ-009**: The active nav item must render as an edge-to-edge flush rectangle with a thick `PRIMARY_BLUE` left-border highlight — not a floating rounded pill with side margins.
- **REQ-010**: The Quick Connect `TextInput` and `Button` must use a standard rounded-rectangle radius (~8px), not a fully-rounded pill shape. The subtitle under "Quick Connect" must be removed.
- **REQ-011**: When the Profiles view is active, the header bar (from TASK-029) must conditionally render a search `TextInput` with an `ICON_SEARCH` prefix and placeholder text "Search profiles...", positioned to the right of the page title.
- **REQ-012**: The Settings view must have sidebar sub-navigation with a "SETTINGS" header and section items (General, Identity & Security, Streaming, Input, Audio), each with an appropriate Material Symbols icon.
- **REQ-013**: The header bar must support breadcrumb rendering ("Settings / {SubPage}") when the Settings view is active.
- **REQ-014**: Settings dropdowns must use `ColorMode::Light` (`launcher_control_surface()`), be width-constrained, and include chevron icons.
- **REQ-015**: Toggle rows in Settings (Exclusive Keyboard Capture, Relative Mouse Movement) must be wrapped in `launcher_toggle_card_surface()` cards with `ColorMode::Light` toggles.
- **REQ-016**: The sidebar must render a user avatar and identity label ("Verified Dev") at the bottom.
- **CON-001**: Overlay UI (`crates/prism-client/src/ui/overlay/*`) must remain unchanged — dark glass capsule stays.
- **CON-002**: No layout/widget/routing/persistence contract changes beyond what is specified.
- **CON-003**: Icon approach chosen: `.ttf` icon font (Material Symbols) rendered through the existing text pipeline — no texture atlas.
- **GUD-001**: Follow existing `with_*()` builder pattern for new widget options (see `Button::with_style()`).
- **PAT-001**: Surface helpers live in `theme.rs`; layout constants live in the consuming module's `const` block.

---

**Important Technical Context (glyphon):**
The text renderer uses the `glyphon` crate. `FontSystem::new()` already loads all system fonts, so no pipeline rewrite or second atlas is needed.
- **Phase 1 (bold):** Thread a `bold` flag from `TextRun` into glyphon's `Attrs` builder: `.weight(if run.bold { Weight::BOLD } else { Weight::NORMAL })`.
- **Phase 5 (icons):** Load the `.ttf` via `font_system.db_mut().load_font_file()` and reference it via `Family::Name("Material Symbols Outlined")`.

**Speed Optimizations:**
- **Icon font loading:** Call `font_system.db_mut().load_font_file()` **once** during `TextPipeline::new()`, not per-frame. The font database is persistent.
- **`TextRun` allocation:** Use `SmallVec<[TextRun; 32]>` or pre-allocate the `Vec<TextRun>` in `PaintContext` to avoid per-frame heap allocation for typical screen layouts (~20–40 text runs).
- **Buffer pooling:** The existing `Vec<Buffer>` pool in `TextPipeline` avoids re-allocating glyphon buffers each frame. Ensure the pool `clear()`s without deallocating (`Vec::clear()` preserves capacity) — already the case.
- **Grid layout caching:** The `two_column_grid()` helper computes pure layout rects from widths and gaps. These can be cached when the container `Rect` doesn't change (which is most frames). Consider a simple `if area_w != cached_w { recompute }` guard.
- **Batch quad submission:** Consolidate `GlassQuad` submissions per surface type. Group opaque quads together and translucent quads together to minimize GPU state changes. The existing `PaintContext.quads` Vec already batches — just ensure callers don't interleave opaque and translucent pushes unnecessarily.
- **Bold weight resolution:** `fontdb` caches font face metadata after first query. Subsequent `Attrs::weight(Weight::BOLD)` lookups are O(1) hash lookups, not file scans. No optimization needed beyond the first frame.

**Git Workflow:**
```
# Before starting Pass 1:
git checkout -b feat/launcher-ui-polish

# After each task completes:
git add -A
git commit -m "feat(launcher): TASK-NNN — <short description>"

# After Phase 11 verification passes:
git checkout main
git merge feat/launcher-ui-polish
```
- **Branch:** `feat/launcher-ui-polish` — created from `main` before any code changes.
- **Commits:** One commit per completed task. Use the format `feat(launcher): TASK-NNN — <description>`. This gives a clean, bisectable history.
- **Merge:** After Phase 11 verification passes, merge into `main`. Do NOT squash — preserve per-task commits for traceability.
- **Release:** Tagging and releasing is a **separate decision** — not bundled into this plan. This branch is a UI polish increment, not necessarily a release milestone. After merging, evaluate whether a release is warranted based on the full state of `main`.

**Recommended Execution Order:**
This plan has 90 tasks with inter-phase dependencies. Execute in five passes to ensure foundational primitives exist before complex views that depend on them:

| Pass | Phases | Tasks | What it builds |
|------|--------|-------|----------------|
| **Pass 0 — Data Prerequisites** | Phase 0 | TASK-P01 → TASK-P06 | `SavedServer` schema extensions (status, OS, tags, latency, WOL), `ServerStatus` enum, heuristic status derivation, `TextRun` `bold`/`icon` fields + manual `Default` impl, `GlassQuad` `Default` impl (TASK-P05a). These are data-layer prerequisites consumed by every rendering phase. |
| **Pass 1 — Foundations** | Phase 1, Phase 2, Phase 3 | TASK-002 → TASK-015 | Bold text rendering, `PRIMARY_BLUE` button audit, Quick Connect layout, sidebar geometry. (`TextRun.bold` field added in Phase 0; Phase 1 threads it through the renderer.) These primitives are consumed by every later phase. |
| **Pass 2 — Icons & Header** | Phase 5, Phase 6 | TASK-024 → TASK-033 | Icon widget, Material Symbols `.ttf` loading, sidebar nav icons, header bar with page title + hamburger menu. Phase 6 depends on Phase 5's `Icon` type. |
| **Pass 3 — Data & Screens** | Phase 4, Phase 7, Phase 8, Phase 9 | TASK-016 → TASK-023, TASK-034 → TASK-064 | Home Recent Connections list (Phase 4), Saved Connections filter bar + card grid + FAB (Phases 7–8), Profiles editor polish + header search bar (Phase 9). Execute one phase at a time sequentially. |
| **Pass 4 — Settings & Polish** | Phase 10 | TASK-066 → TASK-081 | Settings sub-nav, breadcrumb header, dropdown constraints, toggle card surfaces, audio section layout, sidebar footer avatar. Depends on Pass 1 (bold text, sidebar geometry) and Pass 2 (icons, header bar). |

> **Note:** Phase 4 is deferred to Pass 3 because it depends on Phase 1 (bold text) and Phase 5 (icons for status chips and Reconnect button), but does not block Phase 5/6. Phase 11 (Verification) runs last after all passes.

---

### Stitch Design Language — Token Reference

The Stitch `code.html` files define the design system in CSS custom properties.
Below is the mapping from those CSS tokens to the normalised `[f32; 4]` RGBA
values used in `theme.rs`. All values below are for **light mode** only — dark
mode is handled exclusively by the overlay and is NOT modified by this plan.

| CSS Token | Hex | RGBA `[f32; 4]` | Usage |
|-----------|-----|------------------|-------|
| `--colorBrandBackground` | `#0F6CBD` | `[0.059, 0.424, 0.741, 1.0]` | Primary buttons, active filter chip, FAB, slider fill, active nav left-bar, active segmented block |
| `--colorNeutralBackground1` | `#FFFFFF` | `[1.0, 1.0, 1.0, 1.0]` | Card backgrounds (via `launcher_card_surface` / glass tint) |
| `--colorNeutralBackground3` | `#F5F5F5` | `[0.961, 0.961, 0.961, 1.0]` | Sidebar background, input backgrounds |
| `--colorNeutralForeground1` | `#242424` | `[0.141, 0.141, 0.141, 1.0]` | Primary text (≈ `LT_TEXT_PRIMARY`) |
| `--colorNeutralForeground2` | `#616161` | `[0.380, 0.380, 0.380, 1.0]` | Secondary text, subtitles (≈ `LT_TEXT_SECONDARY`) |
| `--colorNeutralForeground3` | `#9E9E9E` | `[0.620, 0.620, 0.620, 1.0]` | Muted text, placeholders (≈ `LT_TEXT_MUTED`) |
| `--colorNeutralStroke1` | `#D1D1D1` | `[0.820, 0.820, 0.820, 1.0]` | Card borders, input borders, separator lines |
| `--colorNeutralStroke2` | `#E0E0E0` | `[0.878, 0.878, 0.878, 1.0]` | Dashed Add Card border, inner row separators |
| `--colorPaletteGreenBackground2` | `#6CCB5F` | `[0.424, 0.796, 0.373, 1.0]` | Online status dot, "Active" chip background |
| `--colorPaletteYellowBackground2` | `#F2C661` | `[0.949, 0.776, 0.380, 1.0]` | Sleeping/Dormant status chip background |
| `--colorPaletteRedBackground2` | `#DC626D` | `[0.863, 0.384, 0.427, 1.0]` | Unreachable/Danger status chip background |
| `--colorBrandForeground1` | `#0F6CBD` | `[0.059, 0.424, 0.741, 1.0]` | Icon tint, link text, "Active" text |
| `--shadow4` | `0 0 2px rgba(0,0,0,0.12), 0 1px 2px rgba(0,0,0,0.14)` | — | Card elevation (modeled via `GlassQuad` tint alpha) |
| `--borderRadiusMedium` | `4px` | `4.0` | Buttons, dropdowns, toggle cards |
| `--borderRadiusLarge` | `8px` | `8.0` | Input fields, cards, hero panel |
| `--borderRadiusXLarge` | `12px` | `12.0` | Filter chips |
| `--borderRadiusCircular` | `50%` | `radius = min(w,h)/2` | FAB, avatar, status dot |

> **Note:** The existing `theme.rs` constants (`PRIMARY_BLUE`, `LT_TEXT_PRIMARY`, `LT_TEXT_SECONDARY`, `LT_TEXT_MUTED`, `CARD_RADIUS`, `CONTROL_RADIUS`, `CHIP_RADIUS`) already approximate many of these tokens. Where the existing value matches, no change is needed. Where a specific gap exists (e.g., `CONTROL_RADIUS=14` vs `--borderRadiusMedium=4`), the fix is noted in the relevant task.

---

## 2. Implementation Steps

### Phase 0: Data-Layer Prerequisites

- GOAL-P00: Extend `SavedServer` with fields required by the design, add a `ServerStatus` enum with heuristic derivation, prepare `TextRun` and `GlassQuad` for non-breaking extensibility (add `bold`/`icon` fields + `Default` impls), and audit the icon font subset.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-P01 | **`ServerStatus` enum:** In `crates/prism-client/src/config/servers.rs`, add `pub enum ServerStatus { Online, Sleeping, Unreachable }`. The design uses status to drive chip color (Success/Warning/Danger), contextual button labels, and unreachable-card dimming. | ✅ | 2026-04-03 |
| TASK-P02 | **Heuristic status derivation:** Add `pub fn derived_status(&self) -> ServerStatus` to `SavedServer`. Logic: if `last_connected` is `None` → `Unreachable`; if age < 6 hours → `Online`; if age < 7 days → `Sleeping`; else → `Unreachable`. This is a temporary heuristic until real-time discovery/ping is implemented. Document the intent to replace with a runtime probe in a follow-up plan. | ✅ | 2026-04-03 |
| TASK-P03 | **Schema extensions:** Add optional fields to `SavedServer`: `pub os_label: Option<String>` (e.g. "WINDOWS 11 PRO"), `pub tags: Vec<String>` (e.g. ["WORK"]), `pub wol_supported: bool` (default `false`), `pub last_latency_ms: Option<u32>`. These fields are `Option`/defaulted so existing serialized data remains compatible. Update `serde` derives and `ServerStore` snapshot logic. | ✅ | 2026-04-03 |
| TASK-P04 | **Filter alignment:** In `ui/launcher/server_card.rs`, update `CardFilter` to support tag-based filters in addition to time-based ones. Add variants: `Tag(String)`. Update `label()` to return the tag name. The filter bar (Phase 7) can render both built-in and tag-derived pills. The current `All`/`Recent`/`Dormant`/`New` filters remain; tag pills are appended dynamically from the union of all servers' `tags`. | ✅ | 2026-04-03 |
| TASK-P05 | **`TextRun` extensibility (merged from Phase 1 TASK-001):** In `ui/widgets/mod.rs`, add `bold: bool` and `icon: bool` fields to `TextRun` (both default `false`). Implement `Default` manually (not `#[derive(Default)]` — `font_size` must default to `14.0`, not `0.0`). Migrate ALL existing `TextRun { ... }` construction sites to use `..Default::default()` syntax. This makes `TextRun` forward-compatible — Phase 1 (bold rendering) and Phase 5 (icon rendering) consume these fields without re-editing the struct or its `Default` impl. | ✅ | 2026-04-03 |
| TASK-P05a | **`GlassQuad` `Default` impl:** In `ui/widgets/mod.rs`, implement `Default` for `GlassQuad` with zeroed fields: `rect: Rect::ZERO`, `blur_rect: Rect::ZERO`, `tint: [0.0; 4]`, `border_color: [0.0; 4]`, `corner_radius: 0.0`, `noise_intensity: 0.0`. This enables `GlassQuad { rect, tint, corner_radius, ..Default::default() }` shorthand throughout all phases. | ✅ | 2026-04-03 |
| TASK-P06 | `cargo check -p prism-client` — verify compilation. All existing `TextRun` and `GlassQuad` construction sites compile with the new `Default` impls. | ✅ | 2026-04-03 |

#### Phase 0 — Implementation Detail

**TASK-P01 — `ServerStatus` enum** (`crates/prism-client/src/config/servers.rs`):

Insert above `pub struct SavedServer`:

```rust
/// Derived connection status for UI rendering.
/// TODO: Replace heuristic derivation with runtime discovery/ping probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ServerStatus {
    Online,
    Sleeping,
    Unreachable,
}
```

**TASK-P02 — `derived_status()`** (`servers.rs`):

Insert inside `impl SavedServer`:

```rust
/// Heuristic status until real-time probing is implemented.
pub fn derived_status(&self) -> ServerStatus {
    use std::time::{SystemTime, UNIX_EPOCH};
    const SECS_6H: u64 = 6 * 60 * 60;
    const SECS_7D: u64 = 7 * 24 * 60 * 60;
    match self.last_connected {
        None => ServerStatus::Unreachable,
        Some(epoch_secs) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let age = now.saturating_sub(epoch_secs);
            if age < SECS_6H {
                ServerStatus::Online
            } else if age < SECS_7D {
                ServerStatus::Sleeping
            } else {
                ServerStatus::Unreachable
            }
        }
    }
}
```

**TASK-P03 — Schema extensions** (`servers.rs`):

Add to `SavedServer` struct fields:

```rust
#[serde(default)]
pub os_label: Option<String>,
#[serde(default)]
pub tags: Vec<String>,
#[serde(default)]
pub wol_supported: bool,
#[serde(default)]
pub last_latency_ms: Option<u32>,
```

**TASK-P04 — `CardFilter` tag variant** (`ui/launcher/server_card.rs`):

Replace the existing `CardFilter` enum:

```rust
pub enum CardFilter {
    All,
    Recent,
    Dormant,
    New,
    Tag(String), // e.g. "WORK", "PERSONAL"
}

impl CardFilter {
    pub fn label(&self) -> String {
        match self {
            Self::All => "All Hosts".into(),
            Self::Recent => "Recent".into(),
            Self::Dormant => "Dormant".into(),
            Self::New => "New".into(),
            Self::Tag(t) => t.clone(),
        }
    }
}
```

**TASK-P05 — `TextRun` & `GlassQuad` extensibility** (`ui/widgets/mod.rs`):

Add `bold: bool` and `icon: bool` fields to `TextRun`. Implement `Default` manually
(not `#[derive(Default)]` — `font_size` must default to `14.0`, not `0.0`):

```rust
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
```

Then migrate ALL existing `TextRun { ... }` construction sites to use `..Default::default()`:

```rust
// BEFORE:
ctx.text_runs.push(TextRun {
    x: rect.x + 18.0,
    y: rect.y + 22.0,
    text: "PRISM".into(),
    font_size: 20.0,
    color: theme::LT_TEXT_PRIMARY,
    monospace: false,
});

// AFTER:
ctx.text_runs.push(TextRun {
    x: rect.x + 18.0,
    y: rect.y + 22.0,
    text: "PRISM".into(),
    font_size: 20.0,
    color: theme::LT_TEXT_PRIMARY,
    ..Default::default()
});
```

**TASK-P05a — `GlassQuad` `Default` impl** (`ui/widgets/mod.rs`):

```rust
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
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 0 — data-layer prerequisites (ServerStatus, schema, TextRun bold+icon, GlassQuad Default)"`

---

### Phase 1: Fix Typography & Primary Colors

- GOAL-001: Thread the `bold` flag (added in Phase 0 TASK-P05) through the text renderer, verify `PRIMARY_BLUE` on the primary button, and remove stale subtitle text.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-002 | In `crates/prism-client/src/renderer/text_renderer.rs`, update the `prepare()` method where `Attrs` is built. Thread the `bold` flag (added to `TextRun` in Phase 0 TASK-P05): `Attrs::new().family(family).weight(if run.bold { Weight::BOLD } else { Weight::NORMAL })`. No new `.ttf` file needed — `FontSystem::new()` already loaded system bold variants. | ✅ | 2026-04-03 |
| TASK-003 | In `ui/launcher/quick_connect.rs`, set `bold: true` on the hero title `TextRun` ("Quick Connect") and the section header `TextRun` ("Recent Connections"). Verify they render in the heavier weight at `FONT_HERO` / `FONT_HEADLINE` sizes respectively. | ✅ | 2026-04-03 |
| TASK-003a | **Micro-polish (REQ-010):** In `ui/launcher/quick_connect.rs`, locate and remove the subtitle `TextRun` under "Quick Connect" (e.g. "Enter a hostname or IP address..."). Only the bold "Quick Connect" title should remain above the input. | ✅ | 2026-04-03 |
| TASK-004 | In `ui/widgets/button.rs`, audit the `paint()` method's `ColorMode::Light` + `ButtonStyle::Primary` branch. Confirm the background quad uses `theme::PRIMARY_BLUE` (`[0.059, 0.424, 0.741, 1.0]`) and text color is `[1.0, 1.0, 1.0, 1.0]`. If the branch is using `theme::ACCENT` or a cyan-tinted `glass_quad()` instead, replace it with `PRIMARY_BLUE`. | ✅ | 2026-04-03 |
| TASK-005 | `cargo check -p prism-client` — verify compilation. | ✅ | 2026-04-03 |

#### Phase 1 — Implementation Detail

**TASK-002 — Thread `bold` into text renderer** (`renderer/text_renderer.rs`):

In `prepare()`, locate the `Attrs` construction (around line 80–100). Currently:

```rust
let attrs = Attrs::new().family(family);
```

Replace with:

```rust
let weight = if run.bold { Weight::BOLD } else { Weight::NORMAL };
let attrs = Attrs::new().family(family).weight(weight);
```

Add at top of file if not present:

```rust
use cosmic_text::Weight;
```

**TASK-003 — Bold hero title** (`ui/launcher/quick_connect.rs`):

Search for the "Quick Connect" `TextRun` push (around line 70–80). Add `bold: true`:

```rust
ctx.text_runs.push(TextRun {
    x: center_x,
    y: panel_rect.y + 40.0,
    text: "Quick Connect".into(),
    font_size: theme::FONT_HERO,
    color: theme::LT_TEXT_PRIMARY,
    bold: true,
    ..Default::default()
});
```

Similarly for the "Recent Connections" header:

```rust
ctx.text_runs.push(TextRun {
    text: "Recent Connections".into(),
    font_size: theme::FONT_HEADLINE,
    bold: true,
    ..Default::default()
});
```

**TASK-003a — Remove subtitle** (`ui/launcher/quick_connect.rs`):

Search for `"Enter a hostname"` or `"Enter a host"`. Delete the entire `TextRun` push
block for the subtitle line. Shift the input `y` coordinate up by the removed
subtitle's height (~20px offset saved).

**TASK-004 — Audit `PRIMARY_BLUE` on primary button** (`ui/widgets/button.rs`):

In `paint()`, find the `ColorMode::Light` + `ButtonStyle::Primary` branch (around line 140–170).
Verify the background quad uses:

```rust
tint: theme::PRIMARY_BLUE,  // [0.059, 0.424, 0.741, 1.0]
```

If it uses `theme::ACCENT` or a glass-tinted value, replace it:

```rust
// Search for this pattern in the Light+Primary branch:
let bg_color = theme::PRIMARY_BLUE;
// Text must be white:
let text_color = [1.0, 1.0, 1.0, 1.0];
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 1 — bold typography + PRIMARY_BLUE button audit"`

---

### Phase 2: Constrain Quick Connect Layout

- GOAL-002: Center the address input and Connect button within the hero panel at a constrained max width, using standard rounded-rectangle shapes (not pills), matching the proportions in `screen.png`.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-006 | In `ui/launcher/quick_connect.rs`, locate the layout method (likely `compute_layout` or the section inside `layout()` that assigns `Rect` to `address_input` and `connect_button`). Currently these rects span the full width of the parent `launcher_hero_surface`. | ✅ | 2026-04-03 |
| TASK-007 | Add a `const MAX_INPUT_W: f32 = 480.0;` constant at the top of `quick_connect.rs`. In the layout calculation, clamp the input/button container width: `let input_w = hero_rect.w.min(MAX_INPUT_W);`. Compute `let input_x = hero_rect.x + (hero_rect.w - input_w) / 2.0;` to center the container. Apply `input_x` and `input_w` to both the `address_input` and `connect_button` rects. | ✅ | 2026-04-03 |
| TASK-007a | **Micro-polish (REQ-010):** The `TextInput` and `Button` currently render with a fully-rounded pill shape (maximum border-radius). Change the border-radius to a standard rounded rectangle (~8px). In `ui/widgets/text_input.rs`, locate the `paint()` method's `GlassQuad` construction and replace the radius value (likely `CONTROL_RADIUS` = 14.0 or a hardcoded large value) with `8.0`. Do the same in `ui/widgets/button.rs` for the button background quad. This applies globally, so verify overlay buttons still look acceptable or guard behind `ColorMode::Light`. | ✅ | 2026-04-03 |
| TASK-008 | Visually verify that the input and button are centered in the hero panel, do not stretch edge-to-edge, and have standard rounded-rectangle corners (not pills). Adjust `MAX_INPUT_W` if the proportions don't match `screen.png`. | ✅ | 2026-04-03 |

#### Phase 2 — Implementation Detail

**TASK-007 — Constrain input/button width** (`ui/launcher/quick_connect.rs`):

Add constant at module top:

```rust
const MAX_INPUT_W: f32 = 480.0;
```

In the layout section (around line 100–120), replace the current width calculation:

```rust
// BEFORE:
let input_w = panel_rect.w - pad * 2.0;
let input_x = panel_rect.x + pad;

// AFTER:
let input_w = (panel_rect.w - pad * 2.0).min(MAX_INPUT_W);
let input_x = panel_rect.x + (panel_rect.w - input_w) / 2.0;
```

Apply `input_x` and `input_w` to both `self.address_input.rect` and `self.connect_button.rect`.

**TASK-007a — Standard corner radius** (`ui/widgets/text_input.rs` + `ui/widgets/button.rs`):

In `text_input.rs`, search for the `GlassQuad` in `paint()`:

```rust
// BEFORE (likely uses CONTROL_RADIUS = 14.0 or rect.h / 2.0 for pill):
corner_radius: theme::CONTROL_RADIUS,
// or:
corner_radius: rect.h / 2.0,

// AFTER:
corner_radius: 8.0,
```

In `button.rs`, same change in the `GlassQuad` construction:

```rust
// BEFORE:
corner_radius: theme::CONTROL_RADIUS,

// AFTER — but guard for overlay mode:
corner_radius: match self.color_mode {
    ColorMode::Light => 8.0,
    ColorMode::Dark => theme::CONTROL_RADIUS, // preserve overlay pill shape
},
```

> **Note:** If a `.with_radius()` builder is preferred (for TASK-052 reuse),
> add `radius_override: Option<f32>` to `Button` and apply it in `paint()`:
> `let radius = self.radius_override.unwrap_or(theme::CONTROL_RADIUS);`

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 2 — constrain Quick Connect layout + standard corner radius"`

---

### Phase 3: Sidebar Edge-to-Edge Fix & Nav Styling

- GOAL-003: Make the sidebar a structural, flush panel against the window's top, left, and bottom edges — not a floating rounded card. Fix the active nav item to use an edge-to-edge highlight with a blue left-border indicator. Replace the sidebar branding text with a hamburger icon placeholder.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-009 | In `ui/launcher/shell.rs`, locate `compute_layout()` (around line 167). The sidebar rect is currently constructed as: `Rect::new(SIDEBAR_PAD, SIDEBAR_PAD, SIDEBAR_W, (screen_h - SIDEBAR_PAD * 2.0).max(280.0))` where `SIDEBAR_PAD = 28.0`. Change this to: `Rect::new(0.0, 0.0, SIDEBAR_W, screen_h)` so the sidebar starts at the window origin and spans the full height. | ✅ | 2026-04-03 |
| TASK-010 | In `ui/theme.rs`, locate `launcher_sidebar_surface()` (around line 288). It currently passes `SIDEBAR_RADIUS` (28.0) to `glass_quad()`. Change the radius argument to `0.0` so the sidebar renders as a sharp rectangle flush against the window frame. | ✅ | 2026-04-03 |
| TASK-011 | Verify `content_x` calculation in `compute_layout()` still works after the sidebar origin change. It should now be: `let content_x = SIDEBAR_W + CONTENT_PAD;` (since `sidebar_rect.x` is now `0.0`). Confirm no visual overlap or gap. | ✅ | 2026-04-03 |
| TASK-012 | In `ui/launcher/nav.rs`, verify that nav item rects are computed relative to the sidebar rect and still render correctly after the origin/size change. Adjust `SIDE_PADDING` if the nav items are now too close to the window edge. | ✅ | 2026-04-03 |
| TASK-012a | **Micro-polish (REQ-009):** In `ui/launcher/nav.rs`, change the active nav item rendering. Currently it uses `launcher_nav_item_surface()` which produces a floating rounded pill with `CONTROL_RADIUS` and side margins. Replace with: (a) an edge-to-edge flush rectangle (`radius: 0.0`, `x: 0.0`, `w: sidebar_rect.w`) with a slightly darker background (`[0.898, 0.898, 0.898, 0.60]` or similar), and (b) a 4px-wide `PRIMARY_BLUE` bar drawn at `x: 0.0` on the far left edge of the item rect. Remove `SIDE_PADDING` from the active item's x-origin and width. **Extract this as a reusable helper** `paint_active_list_indicator(quads, rect, accent_color)` in `theme.rs` — it will be reused by TASK-050 (Profiles active list item) and TASK-067 (Settings sub-nav active state). | ✅ | 2026-04-03 |
| TASK-012b | **Micro-polish (REQ-008):** In `ui/launcher/nav.rs`, locate where the sidebar header renders "PRISM" / "Remote client" text at the top. Replace with a hamburger menu icon. **If Phase 5 has already landed:** render `Icon::new(ICON_MENU).with_size(24.0).with_color(theme::LT_TEXT_SECONDARY)` directly. **If Phase 5 has NOT landed yet:** render the Unicode character `'\u{2261}'` (≡) as a `TextRun` at ~24px in `LT_TEXT_SECONDARY`, and add a `// TODO(Phase 5): replace with Icon::new(ICON_MENU)` comment. Either way, the Phase 6 TASK-028a replacement step is eliminated — the hamburger is finalized in this task. | ✅ | 2026-04-03 |
| TASK-013 | `cargo check -p prism-client` — verify compilation. Visual check: sidebar should be flush against top/left/bottom with no rounded corners on the left edge. Active nav item should be flush with a blue left-border, not a floating pill. Sidebar top should show `≡` not "PRISM / Remote client". | ✅ | 2026-04-03 |

#### Phase 3 — Implementation Detail

**TASK-009 — Sidebar rect flush** (`ui/launcher/shell.rs`):

In `compute_layout()` (around line 167), search for:

```rust
Rect::new(SIDEBAR_PAD, SIDEBAR_PAD, SIDEBAR_W, (screen_h - SIDEBAR_PAD * 2.0).max(280.0))
```

Replace with:

```rust
Rect::new(0.0, 0.0, SIDEBAR_W, screen_h)
```

**TASK-010 — Sidebar surface radius** (`ui/theme.rs`):

Search for `launcher_sidebar_surface` (around line 288). Find the line that passes
`SIDEBAR_RADIUS` to the glass quad:

```rust
// BEFORE:
corner_radius: SIDEBAR_RADIUS,

// AFTER:
corner_radius: 0.0,
```

**TASK-011 — Fix `content_x`** (`ui/launcher/shell.rs`):

After TASK-009, the sidebar starts at `x = 0.0`, so:

```rust
let content_x = SIDEBAR_W + CONTENT_PAD; // was: sidebar_rect.x + SIDEBAR_W + CONTENT_PAD
```

Verify `content_rect` uses this updated `content_x`.

**TASK-012a — Active nav styling** (`ui/launcher/nav.rs` + `ui/theme.rs`):

Step 1: Add reusable helper in `theme.rs`:

```rust
/// Paints an edge-to-edge active indicator with a 4px accent left bar.
/// Reused by nav items (TASK-012a), Profiles list (TASK-050), Settings sub-nav (TASK-067).
pub fn paint_active_list_indicator(
    quads: &mut Vec<GlassQuad>,
    item_rect: Rect,
    accent_color: [f32; 4],
) {
    // Background: edge-to-edge flush, subtle highlight
    quads.push(GlassQuad {
        rect: Rect::new(0.0, item_rect.y, item_rect.w, item_rect.h),
        blur_rect: Rect::ZERO,
        tint: [0.898, 0.898, 0.898, 0.60],
        border_color: [0.0; 4],
        corner_radius: 0.0,
        noise_intensity: 0.0,
    });
    // 4px accent left bar
    quads.push(GlassQuad {
        rect: Rect::new(0.0, item_rect.y, 4.0, item_rect.h),
        blur_rect: Rect::ZERO,
        tint: accent_color,
        border_color: [0.0; 4],
        corner_radius: 0.0,
        noise_intensity: 0.0,
    });
}
```

Step 2: In `nav.rs`, replace the current active item rendering. Search for
`launcher_nav_item_surface` in the active branch and replace with:

```rust
if is_active {
    theme::paint_active_list_indicator(&mut ctx.glass_quads, item_rect, theme::PRIMARY_BLUE);
} else if hovered {
    // Subtle hover highlight
    ctx.glass_quads.push(GlassQuad {
        rect: item_rect,
        tint: [1.0, 1.0, 1.0, 0.40],
        corner_radius: 0.0,
        ..Default::default()
    });
}
```

**TASK-012b — Replace sidebar branding** (`ui/launcher/nav.rs`):

Search for `"PRISM"` and `"Remote client"` text runs (around lines 85–100).
Replace both with a hamburger menu icon.

**If executing Pass 1 and Pass 2 back-to-back** (Phase 5 `Icon` widget available):

```rust
// AFTER (final — no Phase 6 follow-up needed):
Icon::new(ICON_MENU)
    .with_size(24.0)
    .with_color(theme::LT_TEXT_SECONDARY)
    .at(rect.x + 18.0, rect.y + 30.0)
    .paint(ctx);
```

**If Phase 5 has NOT landed yet** (`Icon` widget unavailable):

```rust
// AFTER (temporary — replace with Icon::new(ICON_MENU) when Phase 5 lands):
// TODO(Phase 5): replace with Icon::new(ICON_MENU)
ctx.text_runs.push(TextRun {
    x: rect.x + 18.0,
    y: rect.y + 30.0,
    text: "\u{2261}".into(), // ≡ hamburger
    font_size: 24.0,
    color: theme::LT_TEXT_SECONDARY,
    ..Default::default()
});
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 3 — sidebar edge-to-edge + active nav indicator + hamburger"`

---

### Phase 4: Populate Recent Connections List

- GOAL-004: Implement the data rendering loop for the Recent Connections section on the Home screen, replacing the empty-state placeholder.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-014 | In `shell.rs`, the Home tab currently delegates Recent Connections rendering to `card_grid` in `Rows` mode with `visible_limit = Some(3)`. **Refactor this delegation** — instead of reusing the Connections-page card grid, build a dedicated lightweight row list directly in `quick_connect.rs` (or a new `recent_list.rs`). Remove the `Rows`-mode delegation from the Home tab paint path. This avoids coupling the Home screen's simple 3-row list to the full Connections-page grid/filter/scroll infrastructure. The empty state ("No saved desktops match this filter.") insertion point remains in the same location. | | |
| TASK-015 | Wrap the list area in `theme::launcher_list_surface(list_rect)` — a nearly-opaque white glass container (`rgba(255,255,255,0.85)`). Compute `list_rect` to fill the space below the "Recent Connections" section header within the content area. | | |
| TASK-016 | Implement a rendering loop over the saved-server data (likely `Vec<SavedServer>` from `crate::config::servers`). For each server, compute a row rect and render using `theme::launcher_row_surface(row_rect, hovered)`. | | |
| TASK-017 | Each row must render: (a) a numbered index in `LT_TEXT_MUTED`, (b) server name in `LT_TEXT_PRIMARY` bold, (c) a status chip via `theme::launcher_status_chip(chip_rect, tone)` with text color from `theme::launcher_chip_text_color(tone)` — mapping Online→`Success`, Sleeping→`Warning`, Unreachable→`Danger`, (d) last-connected timestamp in `LT_TEXT_MUTED`, (e) a "Reconnect" button using `ButtonStyle::Secondary` + `ColorMode::Light` with a refresh/sync icon (`ICON_SYNC` or `ICON_REFRESH`) rendered to the left of the label text. The icon should use `LT_TEXT_SECONDARY` color at ~14px. Note: the icon won't render until Phase 5 lands the `Icon` widget — initially render the button text-only, then revisit in Phase 6 to inject the icon. | | |
| TASK-018 | Add `launcher_inner_separator()` between rows inside the list container (permitted per TASK-061 reconciliation rule — dividers are allowed inside card/list containers where the design screenshots show them). | | |
| TASK-019 | Keep the "No saved desktops match this filter." empty state as a fallback when the server list is empty. | | |
| TASK-020 | `cargo check -p prism-client` — verify compilation. | | |

#### Phase 4 — Implementation Detail

**TASK-014 — Remove Home→CardGrid delegation** (`ui/launcher/shell.rs`):

In the Home tab's paint path, locate the `card_grid.paint(ctx, ..., GridMode::Rows, visible_limit: Some(3))`
call. Remove this delegation entirely. The Recent Connections list for the Home screen
will be rendered directly in `quick_connect.rs` (TASK-015–019), not through the
Connections-page `CardGrid` infrastructure. This decouples the two views.

**TASK-015 — List container** (`ui/launcher/quick_connect.rs`):

Below the "Recent Connections" header, compute the list rect:

```rust
let list_y = section_header_y + theme::FONT_HEADLINE + 16.0;
let list_h = (content_rect.y + content_rect.h) - list_y - 16.0;
let list_rect = Rect::new(content_rect.x, list_y, content_rect.w, list_h);
theme::launcher_list_surface(&mut ctx.glass_quads, list_rect);
```

**TASK-017 — Row rendering loop**:

```rust
const ROW_H: f32 = 54.0;
const ROW_PAD: f32 = 18.0;

for (i, server) in servers.iter().take(5).enumerate() {
    let row_y = list_rect.y + ROW_PAD + i as f32 * ROW_H;
    let row_rect = Rect::new(list_rect.x + ROW_PAD, row_y, list_rect.w - ROW_PAD * 2.0, ROW_H);

    // (a) Index number
    ctx.text_runs.push(TextRun {
        x: row_rect.x,
        y: row_y + 18.0,
        text: format!("{}", i + 1),
        font_size: theme::FONT_BODY,
        color: theme::LT_TEXT_MUTED,
        ..Default::default()
    });

    // (b) Server name (bold)
    ctx.text_runs.push(TextRun {
        x: row_rect.x + 32.0,
        y: row_y + 18.0,
        text: server.display_name.clone(),
        font_size: theme::FONT_BODY,
        color: theme::LT_TEXT_PRIMARY,
        bold: true,
        ..Default::default()
    });

    // (c) Status chip — uses ServerStatus from Phase 0
    let status = server.derived_status();
    let chip_rect = Rect::new(row_rect.x + 200.0, row_y + 14.0, 80.0, 24.0);
    theme::launcher_status_chip(&mut ctx.glass_quads, chip_rect, status.into());

    // (d) Timestamp
    // (e) "Reconnect" button (icon added in Phase 6)

    // Inner separator (except last row)
    if i + 1 < servers.len().min(5) {
        theme::launcher_inner_separator(&mut ctx.glass_quads, /* ... */);
    }
}
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 4 — Recent Connections list rendering"`

---

### Phase 5: Icon Rendering Primitive (`.ttf` Icon Font)

- GOAL-005: Introduce an icon rendering primitive using a `.ttf` icon font (Material Symbols) rendered through the existing text pipeline, enabling dynamic recoloring per `ColorMode`.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-021 | ~~Acquire the Material Symbols `.ttf` file.~~ **Done.** `crates/prism-client/assets/fonts/MaterialSymbolsOutlined.ttf` (38 KB, subsetted to 26 codepoints) is already in the codebase with Apache 2.0 license. | ✅ | 2026-04-03 |
| TASK-021a | **Icon font subset audit:** Verify that `MaterialSymbolsOutlined.ttf` (38 KB, subsetted to 26 codepoints) contains ALL 28+ codepoints defined in TASK-025. Run `python -c "from fontTools.ttLib import TTFont; t = TTFont('...'); print(sorted(hex(c) for c in t.getBestCmap().keys()))"` to list available codepoints. If any required codepoints are missing, re-subset from the full Material Symbols variable font (~2.7 MB, Apache 2.0, Google Fonts) using `pyftsubset`. This must pass before TASK-022 loads the font. | ✅ | 2026-04-03 |
| TASK-022 | In the font/text renderer (the system that loads `.ttf` files and creates GPU font atlases — likely in `crates/prism-client/src/renderer/`), load `MaterialSymbolsOutlined.ttf` as a second font face. Assign it a named handle or enum variant (e.g. `FontFamily::Icons`). | ✅ | 2026-04-03 |
| TASK-023 | Create `ui/widgets/icon.rs` with an `Icon` widget. Minimal API: `Icon::new(codepoint: char)` — stores the Unicode codepoint for the desired Material Symbol. Add `.with_size(f32)`, `.with_color([f32; 4])`, `.with_color_mode(ColorMode)` builders. The `paint()` method emits a `TextRun` using the icon font face, the codepoint as text content, and the specified size/color. | ✅ | 2026-04-03 |
| TASK-024 | Register `pub mod icon;` in `ui/widgets/mod.rs`. Re-export `Icon` from the widgets prelude. | ✅ | 2026-04-03 |
| TASK-025 | Define icon codepoint constants in `icon.rs` (or a sibling `icon_codes.rs`) for the icons needed by the launcher. At minimum: `ICON_HOME`, `ICON_DEVICES` (connections), `ICON_TUNE` (profiles), `ICON_SETTINGS`, `ICON_SEARCH`, `ICON_ADD`, `ICON_HEADPHONES`, `ICON_MIC`, `ICON_KEYBOARD`, `ICON_MONITOR`, `ICON_BOLT` (wake), `ICON_SHIELD` (security), **`ICON_MENU`** (hamburger ≡), **`ICON_SYNC`** (refresh/reconnect), **`ICON_HEART`** (favorite), **`ICON_EDIT`** (edit/pencil), **`ICON_FILTER`** (filter funnel), **`ICON_SORT`** (sort/order), **`ICON_GAMEPAD`** (gaming profile), **`ICON_CODE`** (coding profile), **`ICON_BALANCE`** (balanced profile), **`ICON_SPEED`** (low-bandwidth/speed), **`ICON_CHEVRON_DOWN`** (dropdown chevron), **`ICON_DIAL`** (performance/tuning), **`ICON_MORE_VERT`** (kebab menu ⋮), **`ICON_CLOCK`** (last-connected timestamp). Map each to its Material Symbols Unicode value. Verify all codepoints are included in the subsetted `.ttf` (TASK-021a); if not, re-run `pyftsubset` to add them. | ✅ | 2026-04-03 |
| TASK-025a | **`text_width_exact()` helper:** In `renderer/text_renderer.rs`, add `pub fn text_width_exact(font_system: &mut FontSystem, text: &str, font_size: f32, bold: bool) -> f32` that creates a temporary `Buffer`, calls `set_text()` + `shape_until_scroll()`, and returns the actual shaped run width. This replaces the `text.chars().count() * font_size * 0.52` heuristic in `theme::text_width()` for critical centering/alignment tasks (TASK-007, TASK-029, TASK-068 breadcrumbs). Non-critical layout can continue using the heuristic. | ✅ | 2026-04-03 |
| TASK-025b | **`Dropdown.with_trailing_icon()` builder:** In `ui/widgets/dropdown.rs`, add a `trailing_icon: Option<char>` field (default `None`) and a `.with_trailing_icon(codepoint: char)` builder method. In `paint()`, if `trailing_icon` is `Some(cp)`, render `Icon::new(cp).with_size(16.0)` at the right edge of the dropdown rect inside the control surface. Default to `ICON_CHEVRON_DOWN` when `trailing_icon` is `None`. Callers can override with `ICON_SPEAKER`, `ICON_MIC`, etc. Consumed by TASK-055, TASK-070, and TASK-073. | ✅ | 2026-04-03 |
| TASK-026 | `cargo check -p prism-client` — verify the icon widget compiles and the font loads without error. | ✅ | 2026-04-03 |
| TASK-027 | Smoke-test: temporarily render `Icon::new(ICON_HOME).with_size(24.0).with_color(theme::LT_TEXT_PRIMARY)` somewhere visible (e.g. top of the sidebar) to confirm the glyph renders at the correct size and color. Remove the smoke test after verification. | ✅ | 2026-04-03 |

#### Phase 5 — Implementation Detail

**TASK-022 — Load icon font** (`renderer/text_renderer.rs`):

In `TextPipeline::new()`, after `FontSystem::new()`, add:

```rust
// Load Material Symbols icon font — once, persistent in font database.
let icon_font_path = std::path::Path::new("assets/fonts/MaterialSymbolsOutlined.ttf");
font_system.db_mut().load_font_file(icon_font_path)
    .expect("Failed to load MaterialSymbolsOutlined.ttf");
```

In `prepare()`, extend the family selection to support icon text runs:

```rust
let family = if run.icon {
    Family::Name("Material Symbols Outlined")
} else if run.monospace {
    Family::Monospace
} else {
    Family::SansSerif
};
```

> The `icon: bool` field on `TextRun` was already added in Phase 0 TASK-P05.
> The `Icon` widget sets `icon: true` on its emitted `TextRun`.

**TASK-023 — `Icon` widget** (`ui/widgets/icon.rs` — **new file**):

```rust
use super::{TextRun, ColorMode, PaintContext};
use crate::ui::theme;

/// Material Symbols icon rendered through the text pipeline.
pub struct Icon {
    pub codepoint: char,
    pub size: f32,
    pub color: [f32; 4],
    pub x: f32,
    pub y: f32,
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

    pub fn with_size(mut self, size: f32) -> Self { self.size = size; self }
    pub fn with_color(mut self, color: [f32; 4]) -> Self { self.color = color; self }
    pub fn at(mut self, x: f32, y: f32) -> Self { self.x = x; self.y = y; self }

    /// Emit a TextRun for this icon into the paint context.
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
```

**TASK-025 — Icon codepoint constants** (in `icon.rs` or `icon_codes.rs`):

```rust
// Material Symbols Outlined codepoints (verify against subsetted .ttf)
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
pub const ICON_BALANCE: char = '\u{E8F1}'; // tune/balance
pub const ICON_SPEED: char = '\u{E9E4}';
pub const ICON_CHEVRON_DOWN: char = '\u{E5CF}';
pub const ICON_DIAL: char = '\u{E9E1}'; // performance dial
pub const ICON_MORE_VERT: char = '\u{E5D4}'; // kebab ⋮
pub const ICON_CLOCK: char = '\u{E8B5}';
pub const ICON_HEADPHONES: char = '\u{F01D}';
pub const ICON_MIC: char = '\u{E029}';
pub const ICON_KEYBOARD: char = '\u{E312}';
pub const ICON_MONITOR: char = '\u{E30B}';
pub const ICON_BOLT: char = '\u{EA0B}'; // wake/WOL
pub const ICON_SHIELD: char = '\u{E8E8}';
pub const ICON_SPEAKER: char = '\u{E32D}';
pub const ICON_STREAMING: char = '\u{E1B2}'; // cast/streaming
```

> **Important:** Verify each codepoint is in the subsetted `.ttf` (38 KB). If any
> are missing, re-subset with pyftsubset:
> ```bash
> pyftsubset MaterialSymbolsOutlined.ttf \
>   --unicodes="U+E88A,U+E1B1,U+E429,..." \
>   --output-file=MaterialSymbolsOutlined.ttf
> ```

**TASK-025a — `text_width_exact()` helper** (`renderer/text_renderer.rs`):

```rust
/// Measure the exact pixel width of shaped text. Use for critical layout
/// (centering, breadcrumbs, header alignment). For non-critical layout,
/// continue using `theme::text_width()` heuristic.
pub fn text_width_exact(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    bold: bool,
) -> f32 {
    let metrics = Metrics::new(font_size, font_size * 1.2);
    let mut buffer = Buffer::new(font_system, metrics);
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let attrs = Attrs::new().family(Family::SansSerif).weight(weight);
    buffer.set_text(font_system, text, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .next()
        .unwrap_or(0.0)
}
```

**TASK-025b — `Dropdown.with_trailing_icon()`** (`ui/widgets/dropdown.rs`):

Add field and builder:

```rust
pub struct Dropdown {
    // ... existing fields ...
    pub trailing_icon: Option<char>,
}

impl Dropdown {
    pub fn with_trailing_icon(mut self, codepoint: char) -> Self {
        self.trailing_icon = Some(codepoint);
        self
    }
}
```

In `paint()`, after rendering the selected value text, add:

```rust
use crate::ui::widgets::icon::{Icon, ICON_CHEVRON_DOWN};

let icon_cp = self.trailing_icon.unwrap_or(ICON_CHEVRON_DOWN);
Icon::new(icon_cp)
    .with_size(16.0)
    .with_color(theme::LT_TEXT_MUTED)
    .at(rect.x + rect.w - 28.0, rect.y + (rect.h - 16.0) / 2.0)
    .paint(ctx);
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 5 — Icon widget + Material Symbols font loading + codepoint constants + text_width_exact + Dropdown.with_trailing_icon"`

---

### Phase 6: Apply Icons to Sidebar & Header

- GOAL-006: Using the icon primitive from Phase 5, populate the sidebar nav items with icons and build the top-right header bar (PRISM logo, avatar placeholder, window controls area).

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-028 | In `ui/launcher/nav.rs`, update each nav item to render an `Icon` to the left of the label text. Map tabs to icons: Home→`ICON_HOME`, Connections→`ICON_DEVICES`, Profiles→`ICON_TUNE`, Settings→`ICON_SETTINGS`. Size: 20px. Color: `LT_TEXT_PRIMARY` when active, `LT_TEXT_SECONDARY` when inactive. Adjust item rect width and text x-offset to accommodate the icon. Also: if TASK-012b used a temporary `\u{2261}` Unicode char (Phase 5 not yet landed at that time), replace it now with `Icon::new(ICON_MENU)`. | ✅ | 2026-04-03 |
| TASK-029 | **Micro-polish (REQ-007):** In `ui/launcher/shell.rs`, add a `paint_header_bar()` method that renders a horizontal bar spanning from the sidebar's right edge to the window's right edge, at the top of the content area. Contents left-to-right: (a) **the active page title** (e.g. "Home", "Saved Connections") in `LT_TEXT_PRIMARY` bold at `FONT_HEADLINE` — this title moves OUT of the content body and INTO the header bar, (b) spacer, (c) PRISM logo icon or text in `LT_TEXT_MUTED`, (d) user avatar placeholder (32px circle), (e) reserved space for OS window controls. **Remove** the subtitle line ("Connect instantly...") that currently renders below the title in the content area — the design does not show it. | ✅ | 2026-04-03 |
| TASK-030 | Adjust `compute_layout()` in `shell.rs` to account for the header bar height. The content area `y` origin should shift down by the header bar height (e.g. `HEADER_H = 48.0`) so content doesn't overlap the header. | ✅ | 2026-04-03 |
| TASK-031 | `cargo check -p prism-client` — verify compilation. Visual check: sidebar items show icons, header bar is visible. | ✅ | 2026-04-03 |
| TASK-032 | **Sidebar branding (REQ-008 clarification):** The Settings design (`screen.png`) shows `≡` + gem icon + "PRISM" text at the sidebar top, while Home/Connections/Profiles designs show only `≡`. **Canonical rule:** Always render `ICON_MENU` at the far left. When Settings is active, additionally render the PRISM gem icon + "PRISM" text to the right of the hamburger. On all other tabs, the hamburger stands alone. | ✅ | 2026-04-03 |
| TASK-033 | **Server form modal audit:** In `ui/launcher/server_form.rs`, verify the form modal uses `ColorMode::Light` surfaces, `launcher_modal_surface()`, `PRIMARY_BLUE` primary button, 8px rounded-rectangle inputs, and `LT_TEXT_*` colors. If it still uses dark-mode styles, update it to match the launcher's light theme. This is a polish check — no structural changes to the form's fields or routing. | ✅ | 2026-04-03 |
| TASK-034 | `cargo check -p prism-client` — verify all Phase 6 changes compile. | ✅ | 2026-04-03 |

#### Phase 6 — Implementation Detail

**TASK-028 — Nav item icons** (`ui/launcher/nav.rs`):

In the nav item rendering loop, add an `Icon` before each label:

```rust
use crate::ui::widgets::icon::{Icon, ICON_HOME, ICON_DEVICES, ICON_TUNE, ICON_SETTINGS};

let icon_codepoint = match tab {
    Tab::Home => ICON_HOME,
    Tab::Connections => ICON_DEVICES,
    Tab::Profiles => ICON_TUNE,
    Tab::Settings => ICON_SETTINGS,
};
let icon_color = if is_active { theme::LT_TEXT_PRIMARY } else { theme::LT_TEXT_SECONDARY };

Icon::new(icon_codepoint)
    .with_size(20.0)
    .with_color(icon_color)
    .at(item_rect.x + 14.0, item_rect.y + 10.0)
    .paint(ctx);

// Shift label text right to accommodate icon:
let label_x = item_rect.x + 14.0 + 20.0 + 8.0; // icon_pad + icon_size + gap
```

**TASK-029 — Header bar** (`ui/launcher/shell.rs`):

Add constants and a dedicated paint method:

```rust
const HEADER_H: f32 = 48.0;

fn paint_header_bar(&self, ctx: &mut PaintContext, content_rect: Rect, active_tab: Tab) {
    let bar_rect = Rect::new(content_rect.x, 0.0, content_rect.w, HEADER_H);

    // Page title (bold)
    let title = match active_tab {
        Tab::Home => "Home",
        Tab::Connections => "Saved Connections",
        Tab::Profiles => "Profiles",
        Tab::Settings => "Settings",
    };
    ctx.text_runs.push(TextRun {
        x: bar_rect.x + 16.0,
        y: bar_rect.y + 14.0,
        text: title.into(),
        font_size: theme::FONT_HEADLINE,
        color: theme::LT_TEXT_PRIMARY,
        bold: true,
        ..Default::default()
    });

    // Right side: PRISM text + avatar placeholder
    let prism_x = bar_rect.x + bar_rect.w - 120.0;
    ctx.text_runs.push(TextRun {
        x: prism_x,
        y: bar_rect.y + 16.0,
        text: "PRISM".into(),
        font_size: theme::FONT_LABEL,
        color: theme::LT_TEXT_MUTED,
        ..Default::default()
    });

    // Avatar circle (32px)
    let avatar_x = bar_rect.x + bar_rect.w - 56.0;
    ctx.glass_quads.push(GlassQuad {
        rect: Rect::new(avatar_x, 8.0, 32.0, 32.0),
        tint: [0.8, 0.85, 0.9, 1.0],
        corner_radius: 16.0, // circular
        ..Default::default()
    });
}
```

**TASK-030 — Shift content below header** (`ui/launcher/shell.rs`):

In `compute_layout()`, update the content origin:

```rust
// BEFORE:
let content_y = 42.0;

// AFTER:
let content_y = HEADER_H;
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 6 — nav icons + header bar + hamburger icon"`

---

### Phase 7: Saved Connections — Filter Bar & Sort

- GOAL-007: Fix the filter bar styling and add right-aligned action buttons on the Saved Connections screen.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-035 | In `ui/launcher/card_grid.rs`, render the page subtitle ("Browse saved desktops, reconnect quickly, and keep your machines organized.") in `LT_TEXT_SECONDARY` at `FONT_BODY` at the top of the content area, just below the header bar and above the filter pills. This subtitle is page-specific — it renders inside `card_grid.rs`, not in the global header bar. Other pages (Home, Profiles, Settings) may have their own subtitles or none at all. | | |
| TASK-036 | In `ui/launcher/card_grid.rs`, update the active filter pill. It currently uses a cyan-tinted `glass_quad()`. Replace with a solid `PRIMARY_BLUE` background + white text. Inactive pills must use `launcher_control_surface()` with `LT_TEXT_SECONDARY` text and a `border-white/60` border — matching the outlined chip style in `screen.png`. | | |
| TASK-037 | Add right-aligned "Filter" and "Last Connected" buttons to the filter bar row. Use `ButtonStyle::Secondary` + `ColorMode::Light`. Each button should include an icon to the left of the label once Phase 5 icons are available (`ICON_FILTER` and `ICON_SORT` respectively). For now, render text-only. Position them at `x: content_rect.x + content_rect.w - button_w` to right-align. | | |
| TASK-038 | `cargo check -p prism-client` — verify compilation. | | |

#### Phase 7 — Implementation Detail

**TASK-036 — Active filter pill** (`ui/launcher/card_grid.rs`):

Search for the active filter chip rendering (around line 200–250). Replace the
glass-tinted background with a solid `PRIMARY_BLUE`:

```rust
if filter == active_filter {
    // Solid PRIMARY_BLUE pill
    ctx.glass_quads.push(GlassQuad {
        rect: chip_rect,
        tint: theme::PRIMARY_BLUE,
        corner_radius: chip_rect.h / 2.0, // fully rounded pill
        ..Default::default()
    });
    ctx.text_runs.push(TextRun {
        text: filter.label(),
        color: [1.0, 1.0, 1.0, 1.0],
        ..Default::default()
    });
} else {
    // Outlined chip
    theme::launcher_control_surface(&mut ctx.glass_quads, chip_rect, false);
    ctx.text_runs.push(TextRun {
        text: filter.label(),
        color: theme::LT_TEXT_SECONDARY,
        ..Default::default()
    });
}
```

**TASK-037 — Right-aligned buttons**:

```rust
let filter_btn_w = theme::text_width("Filter", theme::FONT_LABEL) + 24.0;
let sort_btn_w = theme::text_width("Last Connected", theme::FONT_LABEL) + 24.0;
let filter_btn_x = content_rect.x + content_rect.w - filter_btn_w - sort_btn_w - 8.0;
// Render Button::new("Filter", UiAction::ToggleFilter)
//     .with_style(ButtonStyle::Secondary).with_color_mode(ColorMode::Light)
// at (filter_btn_x, filter_bar_y, filter_btn_w, FILTER_H)
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 7 — filter bar PRIMARY_BLUE pills + sort buttons"`

---

### Phase 8: Saved Connections — Card Grid, Add Card, & FAB

- GOAL-008: Implement the responsive card grid for saved connections, the dashed "Add Connection" card, complex per-card internal layout, and the bottom-right FAB.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-039 | In `ui/launcher/card_grid.rs`, implement a responsive grid layout algorithm. Cards should fill the content width in columns (2–3 columns depending on `content_rect.w`), with consistent horizontal and vertical gaps (~16px). The "Add New Connection" card must be the **last** item in the grid (matching `screen.png`), not centered alone. Compute card rects row-by-row: `let cols = ((content_rect.w + gap) / (card_w + gap)).floor().max(1.0) as usize;` | | |
| TASK-040 | Update the "Add New Connection" card rendering. Replace the current solid rounded rectangle with: (a) `launcher_card_surface(card_rect)` for the frosted glass background, (b) a **dashed white border** — if `GlassQuad` doesn't support dashed borders natively, approximate by rendering 4 thin dashed lines along each edge or use a solid `[1.0,1.0,1.0,0.60]` border with reduced opacity, (c) a white filled circle (~48px diameter) centered in the card, (d) `ICON_ADD` in `PRIMARY_BLUE` centered within the circle, (e) "Add New Connection" label below in `LT_TEXT_SECONDARY`. | | |
| TASK-041 | In `ui/launcher/server_card.rs`, implement the full card internal layout matching `screen.png`. Top half — **Hero placeholder area**: render a solid muted gradient or flat color (e.g. `[0.85, 0.90, 0.95, 1.0]`) filling the top ~55% of the card rect, with `CARD_RADIUS` on top corners and `0.0` on bottom corners (or uniform `CARD_RADIUS` if independent corner radii aren't supported). This placeholder reserves space for future image support (see RISK-011). | | |
| TASK-042 | Over the hero placeholder area, render: (a) **Status badge** — `launcher_status_chip(chip_rect, tone)` with `launcher_chip_text_color(tone)` positioned top-left with ~8px margin, (b) **Tag badge** — a second chip (e.g. "WORK", "PERSONAL") using `ChipTone::Accent` or `ChipTone::Neutral` positioned to the right of the status badge — sourced from `SavedServer.tags` (added in TASK-P03), (c) **Heart icon** — `ICON_HEART` in `[1.0,1.0,1.0,0.80]` (semi-transparent white) positioned top-right with ~8px margin. | | |
| TASK-043 | Card body (below hero area): (a) Server name in `LT_TEXT_PRIMARY` bold at `FONT_BODY` or `FONT_HEADLINE` with a **kebab menu** (`ICON_MORE_VERT` / `⋮`) right-aligned at 16px in `LT_TEXT_MUTED` (matching `screen.png`), (b) OS + IP subtitle in `LT_TEXT_MUTED` at `FONT_LABEL` — sourced from `SavedServer.os_label` (TASK-P03) + `address`, (c) Last-connected timestamp in `LT_TEXT_MUTED` at `FONT_LABEL`, (d) Latency badge — a small chip with `ChipTone::Neutral` showing e.g. "12ms" in `LT_TEXT_SECONDARY` — sourced from `SavedServer.last_latency_ms` (TASK-P03); render "WOL Supported" chip if `wol_supported` is true (Sleeping cards). | | |
| TASK-044 | Card footer (bottom of body): (a) **Contextual primary button** — "Connect" (`ButtonStyle::Primary` + `ColorMode::Light`) when Online, "Wake & Connect" (`ButtonStyle::Secondary` + `ColorMode::Light` + `ICON_BOLT` prefix) when Sleeping, "Retry Discovery" (disabled style: `[0.898,0.898,0.898,0.50]` bg + `LT_TEXT_MUTED` text, non-interactive) when Unreachable, (b) **Edit button** — a square secondary button (~32x32) with `ICON_EDIT` in `LT_TEXT_SECONDARY`, positioned to the right of the action button. Note: the kebab menu ⋮ renders in the card body row (TASK-043), not in the footer. | | |
| TASK-045 | **Unreachable card dimming**: When server status is Unreachable, reduce the overall card surface alpha to ~0.80, render the server name in `LT_TEXT_MUTED` instead of `LT_TEXT_PRIMARY`, and ensure the action button is visually disabled (non-interactive). | | |
| TASK-046 | In `ui/launcher/shell.rs` (or the Connections view container), implement the **Floating Action Button (FAB)**. Render a circular button (`width: 56.0, height: 56.0`, full border-radius = `28.0`) using `PRIMARY_BLUE` background, with a white `ICON_ADD` (size ~24px) centered within it. Position it fixed at `(content_rect.x + content_rect.w - 56.0 - 24.0, content_rect.y + content_rect.h - 56.0 - 24.0)` — bottom-right with ~24px padding. The FAB should emit `UiAction::ShowAddServerForm` (or equivalent) on click. | | |
| TASK-047 | `cargo check -p prism-client` — verify compilation. Visual check against `Theme/Connections/screen.png`: grid layout, card internal structure, filter bar, FAB. | | |
| TASK-047a | **Scroll support (Connections):** The card grid must support vertical scrolling when cards exceed the visible area. Extend `card_grid.rs` with `scroll_y: f32` and `max_scroll: f32` fields (following the pattern already used in `settings.rs`). On `MouseWheel` events within the content rect, update `scroll_y` and clamp to `[0.0, max_scroll]`. Apply `-scroll_y` offset to all card rects during paint. Render a thin scrollbar indicator on the right edge when `max_scroll > 0.0`. | | |

#### Phase 8 — Implementation Detail

**TASK-039 — Grid layout algorithm** (`ui/launcher/card_grid.rs`):

```rust
const CARD_W: f32 = 282.0;
const CARD_H: f32 = 198.0;
const CARD_GAP: f32 = 20.0;

fn compute_card_rects(content_rect: Rect, count: usize) -> Vec<Rect> {
    let cols = ((content_rect.w + CARD_GAP) / (CARD_W + CARD_GAP)).floor().max(1.0) as usize;
    let mut rects = Vec::with_capacity(count);
    for i in 0..count {
        let col = i % cols;
        let row = i / cols;
        let x = content_rect.x + col as f32 * (CARD_W + CARD_GAP);
        let y = content_rect.y + row as f32 * (CARD_H + CARD_GAP);
        rects.push(Rect::new(x, y, CARD_W, CARD_H));
    }
    rects
}
```

Render server cards first, then the "Add New Connection" card as the **last** item.

**TASK-041 — Card hero placeholder** (`ui/launcher/server_card.rs`):

```rust
let hero_h = (card_rect.h * 0.55).round();
let hero_rect = Rect::new(card_rect.x, card_rect.y, card_rect.w, hero_h);
ctx.glass_quads.push(GlassQuad {
    rect: hero_rect,
    tint: [0.85, 0.90, 0.95, 1.0], // muted blue-gray placeholder
    corner_radius: theme::CARD_RADIUS,
    ..Default::default()
});
```

**TASK-046 — FAB** (`ui/launcher/shell.rs` or `card_grid.rs`):

```rust
const FAB_SIZE: f32 = 56.0;
const FAB_PAD: f32 = 24.0;

let fab_rect = Rect::new(
    content_rect.x + content_rect.w - FAB_SIZE - FAB_PAD,
    content_rect.y + content_rect.h - FAB_SIZE - FAB_PAD,
    FAB_SIZE,
    FAB_SIZE,
);
ctx.glass_quads.push(GlassQuad {
    rect: fab_rect,
    tint: theme::PRIMARY_BLUE,
    corner_radius: FAB_SIZE / 2.0, // circular
    ..Default::default()
});
Icon::new(ICON_ADD)
    .with_size(24.0)
    .with_color([1.0, 1.0, 1.0, 1.0])
    .at(fab_rect.x + 16.0, fab_rect.y + 16.0)
    .paint(ctx);
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 8 — card grid + rich cards + Add card + FAB + scroll"`

---

### Phase 9: Profiles Layout & Editor Polish

- GOAL-009: Fix the Profiles screen layout to match `Theme/Profiles/screen.png` — presets list header placement, list item icons and active styling, editor header with profile icon and chip, section grouping with icons, segmented control unification, 2-column grid for dropdowns and toggle cards.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-048 | **Presets header extraction:** In `ui/launcher/profiles.rs`, move the "Presets" label and the "+" add button **out** of the `launcher_list_surface()` container. Render them directly on the gradient background above the white list card. "Presets" uses `LT_TEXT_PRIMARY` bold at `FONT_HEADLINE`. The "+" button is a small circular `PRIMARY_BLUE` button (or `ICON_ADD` at 20px in `PRIMARY_BLUE`) positioned to the right of the label. The `launcher_list_surface()` card now starts immediately below with zero top padding above the first list item. | | |
| TASK-049 | **List item icons:** Add an `Icon` to the left of each preset name in the list. Map profile types to icons: Gaming→`ICON_GAMEPAD`, Coding→`ICON_CODE`, Balanced→`ICON_BALANCE`, Low Bandwidth→`ICON_SPEED`. Icon size: 20px. Color: `PRIMARY_BLUE` when selected, `LT_TEXT_SECONDARY` when inactive. Adjust the text x-offset to accommodate the icon with ~8px gap. | | |
| TASK-050 | **Active list item styling:** The currently selected item must render with: (a) a pure white background `[1.0, 1.0, 1.0, 1.0]` (not gray-tinted `[0.898, 0.898, 0.898, 0.60]`), (b) reuse `paint_active_list_indicator()` from TASK-012a for the 4px `PRIMARY_BLUE` left-border bar, (c) below the subtitle line, a green dot (`SUCCESS` color, ~6px circle) followed by "Active" text in `LT_TEXT_MUTED` at `FONT_LABEL`. Inactive items: transparent background, `LT_TEXT_SECONDARY` text, hover state `[1.0, 1.0, 1.0, 0.40]`. | | |
| TASK-051 | **Editor header icon & chip:** In the editor panel header, render the profile's icon (e.g. `ICON_GAMEPAD`) at ~32px in `PRIMARY_BLUE` to the left of the profile name. Change the chip from "SYSTEM" / `ChipTone::Accent` to "ACTIVE" / `ChipTone::Success` (green pastel bg + dark green text via `launcher_status_chip()` + `launcher_chip_text_color()`). | | |
| TASK-052 | **Editor header buttons:** The "Save Changes" button must use `ButtonStyle::Primary` + `ColorMode::Light` with a **sharp rectangle** radius (~4px, not the default `CONTROL_RADIUS` or pill shape). Implement this as a new radius override on Button (e.g. `.with_radius(4.0)`) or a dedicated `ButtonStyle::PrimarySharp` variant. The "Discard" button must be text-only — no background, no border, just `LT_TEXT_SECONDARY` text that darkens on hover. Implement as a `ButtonStyle::Text` variant or by rendering a bare `TextRun` with click handling. | | |
| TASK-053 | **Section grouping with icons:** Replace the current flat section headers ("Bitrate Preference", "Max FPS", etc.) with grouped uppercase headers. Render: (a) `launcher_inner_separator()` above each group, (b) a small icon in `PRIMARY_BLUE` at ~16px (e.g. `ICON_DIAL` for PERFORMANCE SETTINGS, `ICON_MONITOR` for DISPLAY & AUDIO, `ICON_KEYBOARD` for INPUT & CONNECTIVITY) followed by (c) the section title in `LT_TEXT_SECONDARY` uppercase bold at `FONT_LABEL` with `tracking-[0.2em]` letter-spacing. Individual control labels (Bitrate, Max FPS, etc.) remain as `LT_TEXT_PRIMARY` at normal size below the group header. | | |
| TASK-054 | **Segmented control unification:** In `ui/widgets/segmented.rs`, refactor the `ColorMode::Light` rendering. Instead of individual pill segments, render a single container quad (`SEGMENTED_CONTAINER_LIGHT` = `[1.0, 1.0, 1.0, 0.50]`, border `[1.0, 1.0, 1.0, 1.0]`, `rounded-xl` ~12px). The active segment renders as a solid `PRIMARY_BLUE` block with white text inside the container. Inactive segments are transparent with `LT_TEXT_MUTED` text. The active block should fill its segment width flush within the container, not float as a separate pill. | | |
| TASK-055 | **2-column grid for dropdowns:** In the DISPLAY & AUDIO section of the editor, arrange the dropdowns in a 2-column grid. **Extract a reusable** `two_column_grid(items, area_w, gap) -> Vec<Rect>` helper in `ui/launcher/layout_helpers.rs` (or inline in `profiles.rs`) — it computes `col_w = (area_w - gap) / 2.0` and assigns rects to alternating columns. This helper is reused by TASK-056 (toggle cards) and TASK-070/073 (Settings dropdowns). Each dropdown must include an `ICON_CHEVRON_DOWN` rendered at the right edge inside the control surface. | | |
| TASK-056 | **2-column grid for toggle cards:** In the INPUT & CONNECTIVITY section, arrange toggle cards ("Exclusive Input", "Touch Mode", "Auto-Reconnect") using the `two_column_grid()` helper from TASK-055. Each card uses `launcher_toggle_card_surface(0.60)` with `CARD_RADIUS` (~20px / `rounded-2xl`) and internal padding (~20px). **Extract a reusable** `paint_toggle_card(quads, texts, rect, title, desc, toggle_state, color_mode)` helper — it is reused by TASK-072 (Settings toggle cards). Card contents: title in `LT_TEXT_PRIMARY` bold at `FONT_BODY`, description in `LT_TEXT_MUTED` at `FONT_LABEL`, toggle (`ColorMode::Light`) right-aligned. | | |
| TASK-057 | `cargo check -p prism-client` — verify compilation. Visual check against `Theme/Profiles/screen.png`. | | |
| TASK-058 | **Header search bar (REQ-011):** In `ui/launcher/shell.rs`, update `paint_header_bar()` to conditionally render a search `TextInput` when the active tab is Profiles. Position it to the right of the "Profiles" title with ~16px gap. The input uses `launcher_control_surface()` (white background), `CONTROL_RADIUS` (~8px), an `ICON_SEARCH` prefix rendered inside the input at ~16px in `LT_TEXT_MUTED`, and placeholder text "Search profiles..." in `LT_TEXT_MUTED`. Wire the input value to a `search_filter: String` field on the Profiles view so that Phase 9 list rendering can filter presets by name. | | |
| TASK-059 | **Slider display value:** In the PERFORMANCE SETTINGS section of `profiles.rs`, render the current bitrate value as a large bold number (e.g. "35") at `FONT_DISPLAY` in `LT_TEXT_PRIMARY` with a smaller "MBPS" suffix in `LT_TEXT_MUTED` at `FONT_LABEL`, right-aligned above the slider. Below the slider track, render range labels: "5 MBPS" at the left edge and "50 MBPS" at the right edge, both in `LT_TEXT_MUTED` at `FONT_CAPTION`. This matches the Profiles `screen.png` design. | | |
| TASK-060 | **Scroll support (Profiles editor):** The editor panel can overflow vertically when many controls are visible. Add `scroll_y: f32` and `max_scroll: f32` fields to the Profiles panel (mirroring `settings.rs`). Handle `MouseWheel` within the editor rect, clamp to `[0.0, max_scroll]`, and offset all editor content by `-scroll_y` during paint. | | |
| TASK-061 | **Design spec reconciliation — dividers:** The `DESIGN.md` files specify "Forbid Dividers — do not use lines to separate list items. Use 12px of vertical white space." However, the Stitch `screen.png` targets clearly show subtle dividers in the Settings and Home views. **Canonical rule:** `launcher_inner_separator()` is permitted inside card/list containers (Recent Connections rows, Settings rows) where the design screenshots show them. Between free-standing cards or list items on the gradient background, use 12px vertical spacing only (no separator). Add a comment in `theme.rs` above `launcher_inner_separator()` documenting this reconciliation. | | |
| TASK-062 | **Design spec reconciliation — primary button style:** The `DESIGN.md` specifies a radial gradient (`#a6c8ff` → `#0067c0`) for primary CTAs. The Stitch `screen.png` targets show a flat `PRIMARY_BLUE` (`#0F6CBD`). **Canonical rule:** Use flat `PRIMARY_BLUE` as implemented. The gradient spec is aspirational; flat blue matches the approved mockups. (ALT-005 already documents this decision in Section 3.) Add a brief code comment in `button.rs` near the `PRIMARY_BLUE` usage: `// ALT-005: flat PRIMARY_BLUE per approved mockups — not radial gradient per DESIGN.md`. | | |
| TASK-063 | `cargo check -p prism-client` — verify compilation for all Phase 9 additions. | | |
| TASK-064 | **Scroll support (Home):** If more than 3–5 Recent Connections rows exist, the Home content area may overflow. Add scroll handling to the Home tab content body in `shell.rs` (or `quick_connect.rs`) using the same `scroll_y` / `max_scroll` pattern. Clamp so the Quick Connect hero panel stays pinned and only the list area scrolls. | | |

#### Phase 9 — Implementation Detail

**TASK-052 — Button radius override + text-only style** (`ui/widgets/button.rs`):

Add field and builder:

```rust
pub struct Button {
    // ... existing fields ...
    pub radius_override: Option<f32>,
}

impl Button {
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius_override = Some(radius);
        self
    }
}
```

In `paint()`, replace hardcoded radius:

```rust
let radius = self.radius_override.unwrap_or(
    match self.color_mode {
        ColorMode::Light => 8.0,
        ColorMode::Dark => theme::CONTROL_RADIUS,
    }
);
```

Add `ButtonStyle::Text` variant:

```rust
pub enum ButtonStyle {
    Primary,
    Secondary,
    Destructive,
    Text, // No background, no border — text only
}
```

In `paint()`, `ButtonStyle::Text` branch:

```rust
ButtonStyle::Text => {
    let text_color = if self.hovered {
        theme::LT_TEXT_PRIMARY
    } else {
        theme::LT_TEXT_SECONDARY
    };
    // No GlassQuad — just the text run
    ctx.text_runs.push(TextRun {
        text: self.label.clone(),
        color: text_color,
        ..Default::default()
    });
    return; // skip background quad
}
```

**TASK-055 — `two_column_grid()` layout helper** (`ui/launcher/profiles.rs`):

```rust
/// Computes rects for items in a 2-column grid layout.
fn two_column_grid(items: usize, area_x: f32, area_y: f32, area_w: f32, row_h: f32, gap: f32) -> Vec<Rect> {
    let col_w = (area_w - gap) / 2.0;
    let mut rects = Vec::with_capacity(items);
    for i in 0..items {
        let col = i % 2;
        let row = i / 2;
        let x = area_x + col as f32 * (col_w + gap);
        let y = area_y + row as f32 * (row_h + gap);
        rects.push(Rect::new(x, y, col_w, row_h));
    }
    rects
}
```

**TASK-056 — `paint_toggle_card()` helper** (`ui/launcher/profiles.rs` or `theme.rs`):

```rust
fn paint_toggle_card(
    ctx: &mut PaintContext,
    rect: Rect,
    title: &str,
    desc: &str,
    toggle_on: bool,
    color_mode: ColorMode,
) {
    // Card surface
    theme::launcher_toggle_card_surface(&mut ctx.glass_quads, rect, 0.60);

    // Title (bold)
    ctx.text_runs.push(TextRun {
        x: rect.x + 20.0,
        y: rect.y + 16.0,
        text: title.into(),
        font_size: theme::FONT_BODY,
        color: theme::LT_TEXT_PRIMARY,
        bold: true,
        ..Default::default()
    });

    // Description
    ctx.text_runs.push(TextRun {
        x: rect.x + 20.0,
        y: rect.y + 36.0,
        text: desc.into(),
        font_size: theme::FONT_LABEL,
        color: theme::LT_TEXT_MUTED,
        ..Default::default()
    });

    // Toggle (right-aligned) — rendered by Toggle widget at (rect.x + rect.w - 64, rect.y + 16)
}
```

**TASK-059 — Slider bitrate display** (`ui/launcher/profiles.rs`):

```rust
// Large value display above slider
let value_str = format!("{}", bitrate_mbps);
ctx.text_runs.push(TextRun {
    x: slider_rect.x + slider_rect.w - 80.0,
    y: slider_rect.y - 40.0,
    text: value_str,
    font_size: theme::FONT_DISPLAY,
    color: theme::LT_TEXT_PRIMARY,
    bold: true,
    ..Default::default()
});
ctx.text_runs.push(TextRun {
    x: slider_rect.x + slider_rect.w - 30.0,
    y: slider_rect.y - 30.0,
    text: "MBPS".into(),
    font_size: theme::FONT_LABEL,
    color: theme::LT_TEXT_MUTED,
    ..Default::default()
});

// Range labels below slider
ctx.text_runs.push(TextRun {
    x: slider_rect.x,
    y: slider_rect.y + slider_rect.h + 4.0,
    text: "5 MBPS".into(),
    font_size: theme::FONT_CAPTION,
    color: theme::LT_TEXT_MUTED,
    ..Default::default()
});
ctx.text_runs.push(TextRun {
    x: slider_rect.x + slider_rect.w - 50.0,
    y: slider_rect.y + slider_rect.h + 4.0,
    text: "50 MBPS".into(),
    font_size: theme::FONT_CAPTION,
    color: theme::LT_TEXT_MUTED,
    ..Default::default()
});
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 9 — Profiles layout polish (icons, grouping, grids, slider display, scroll)"`

---

### Phase 10: Settings View Polish

- GOAL-010: Fix the layout of the Settings single-page view, correcting dropdown constraints, applying toggle card surfaces, and adding missing metadata.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-066 | **Sidebar Settings sub-nav:** In `ui/launcher/nav.rs`, below the main tabs, add a sub-header "SETTINGS" using `LT_TEXT_MUTED` at `FONT_LABEL` with uppercase `tracking-[0.2em]`. Render the sub-nav items below it: General (`ICON_SETTINGS`), Identity & Security (`ICON_SHIELD`), Streaming (`ICON_STREAMING`), Input (`ICON_KEYBOARD`), Audio (`ICON_SPEAKER`). Add `ICON_STREAMING` and `ICON_SPEAKER` to `icon.rs` codepoint constants. | | |
| TASK-067 | **Sub-nav active state:** Reuse `paint_active_list_indicator()` from TASK-012a to apply the edge-to-edge flush styling and `PRIMARY_BLUE` left-border highlight to the active Settings sub-nav item (e.g. "Identity & Security") while keeping the others transparent. | | |
| TASK-068 | **Breadcrumb header:** In `ui/launcher/shell.rs`, update `paint_header_bar()` to support breadcrumbs when the active view is Settings. Instead of a single title, render "Settings / Identity & Security". "Settings / " uses `LT_TEXT_SECONDARY`, and "Identity & Security" uses `LT_TEXT_PRIMARY` bold. | | |
| TASK-069 | **Content title & subtitle:** Inside the main content area of `ui/launcher/settings.rs`, ensure the main title is "Identity & Security" using `LT_TEXT_PRIMARY` bold at `FONT_HEADLINE`. Update the subtitle below it to: "Manage your digital footprint and trusted encryption paths for remote sessions." | | |
| TASK-070 | **Fix dropdown constraints:** Locate the "Streaming Defaults" dropdown. It currently renders as a full-width dark block. Force it to `ColorMode::Light` so it uses `launcher_control_surface()`. Constrain its width (e.g. `max_w: 240.0`) and right-align it within the row layout, ensuring it has an `ICON_CHEVRON_DOWN`. | | |
| TASK-071 | **Device Trust metadata:** To the right of the Trusted Device status chip, render a new `TextRun` containing "Last verified 2 hours ago". It must use `LT_TEXT_MUTED` at `FONT_LABEL` and be italicized (if glyphon supports `.style(Style::Italic)`). | | |
| TASK-072 | **Toggle card surfaces:** Wrap the "Exclusive Keyboard Capture" and "Relative Mouse Movement" rows using `paint_toggle_card()` helper from TASK-056 with `launcher_toggle_card_surface(0.30)`. Ensure the toggles themselves are set to `ColorMode::Light`. Remove the stray "System Default" dropdown that appears below the mouse toggle. | | |
| TASK-073 | **Audio section layout:** Refactor the Audio layout. The labels "REMOTE OUTPUT" and "LOCAL MIC PATH" must be rendered above their respective dropdowns using `LT_TEXT_MUTED`, uppercase, with `tracking-[0.2em]`. Render `ICON_SPEAKER` and `ICON_MIC` inside the right edge of these specific dropdowns instead of a chevron. | | |
| TASK-074 | **Sidebar footer avatar:** At the bottom of the sidebar rect in `nav.rs`, render a circular user avatar (placeholder or loaded texture) alongside the text "Verified Dev" in `LT_TEXT_PRIMARY` bold. **Scope:** Render on **all** tabs (not only Settings). The Settings `screen.png` shows it most prominently, but a persistent user identity indicator is consistent with the header bar's avatar circle (TASK-029). | | |
| TASK-075 | `cargo check -p prism-client` — verify compilation. Visual check against `Theme/Settings/screen.png` for the new sub-nav, breadcrumbs, and constrained dropdowns. | | |
| TASK-076 | **Keyboard / focus navigation stub:** Add a `focused_widget: Option<WidgetId>` field (or equivalent) to `LauncherShell`. On `KeyDown(Tab)`, cycle focus forward through interactive widgets (text inputs, buttons, nav items) in DOM order. On `KeyDown(Shift+Tab)`, cycle backward. On `KeyDown(Escape)`, dismiss the active modal if one is open. Render a 2px `PRIMARY_BLUE` focus ring around the focused widget's rect. This is a **minimal stub** — full keyboard navigation (arrow keys in lists/grids, Enter to activate) is deferred to a follow-up plan. | | |
| TASK-077 | `cargo check -p prism-client` — verify Phase 10 compiles. | | |

#### Phase 10 — Implementation Detail

**TASK-066 — Settings sub-nav** (`ui/launcher/nav.rs`):

Below the main nav items, when Settings is active, render a sub-nav section:

```rust
const SETTINGS_SUB_NAV: &[(&str, char)] = &[
    ("General", ICON_SETTINGS),
    ("Identity & Security", ICON_SHIELD),
    ("Streaming", ICON_STREAMING),
    ("Input", ICON_KEYBOARD),
    ("Audio", ICON_SPEAKER),
];

if active_tab == Tab::Settings {
    let sub_y = main_nav_bottom_y + 16.0;

    // "SETTINGS" header
    ctx.text_runs.push(TextRun {
        x: rect.x + 14.0,
        y: sub_y,
        text: "SETTINGS".into(),
        font_size: theme::FONT_LABEL,
        color: theme::LT_TEXT_MUTED,
        bold: true,
        ..Default::default()
    });

    for (i, (label, icon)) in SETTINGS_SUB_NAV.iter().enumerate() {
        let item_y = sub_y + 24.0 + i as f32 * (ITEM_H + 4.0);
        let item_rect = Rect::new(rect.x, item_y, rect.w, ITEM_H);

        if *label == active_sub_section {
            theme::paint_active_list_indicator(&mut ctx.glass_quads, item_rect, theme::PRIMARY_BLUE);
        }

        Icon::new(*icon).with_size(18.0)
            .with_color(if *label == active_sub_section { theme::LT_TEXT_PRIMARY } else { theme::LT_TEXT_SECONDARY })
            .at(item_rect.x + 14.0, item_rect.y + 11.0)
            .paint(ctx);

        ctx.text_runs.push(TextRun {
            x: item_rect.x + 40.0,
            y: item_rect.y + 12.0,
            text: (*label).into(),
            font_size: theme::FONT_LABEL,
            color: if *label == active_sub_section { theme::LT_TEXT_PRIMARY } else { theme::LT_TEXT_SECONDARY },
            ..Default::default()
        });
    }
}
```

**TASK-068 — Breadcrumb header** (`ui/launcher/shell.rs`):

In `paint_header_bar()`, add a breadcrumb branch for Settings:

```rust
if active_tab == Tab::Settings {
    // "Settings / " in muted
    ctx.text_runs.push(TextRun {
        x: bar_rect.x + 16.0,
        y: bar_rect.y + 14.0,
        text: "Settings / ".into(),
        font_size: theme::FONT_HEADLINE,
        color: theme::LT_TEXT_SECONDARY,
        ..Default::default()
    });
    // Active section in bold
    let crumb_x = bar_rect.x + 16.0 + theme::text_width("Settings / ", theme::FONT_HEADLINE);
    ctx.text_runs.push(TextRun {
        x: crumb_x,
        y: bar_rect.y + 14.0,
        text: active_sub_section.into(),
        font_size: theme::FONT_HEADLINE,
        color: theme::LT_TEXT_PRIMARY,
        bold: true,
        ..Default::default()
    });
}
```

**TASK-074 — Sidebar footer avatar** (`ui/launcher/nav.rs`):

At the bottom of the sidebar, render on ALL tabs:

```rust
let footer_y = rect.y + rect.h - 56.0;

// Avatar circle (36px)
ctx.glass_quads.push(GlassQuad {
    rect: Rect::new(rect.x + 14.0, footer_y, 36.0, 36.0),
    tint: [0.75, 0.82, 0.90, 1.0],
    corner_radius: 18.0,
    ..Default::default()
});

// "Verified Dev" label
ctx.text_runs.push(TextRun {
    x: rect.x + 58.0,
    y: footer_y + 10.0,
    text: "Verified Dev".into(),
    font_size: theme::FONT_LABEL,
    color: theme::LT_TEXT_PRIMARY,
    bold: true,
    ..Default::default()
});
```

> **Commit:** `git add -A && git commit -m "feat(launcher): Phase 10 — Settings sub-nav + breadcrumbs + toggle cards + audio layout + footer avatar"`

---

### Phase 11: Verification Pass

- GOAL-011: Full build, test, and visual verification against all screen targets.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-078 | `cargo test -p prism-client` — all tests pass. | | |
| TASK-079 | Visual verification checklist — **Home screen**: (1) Hero title "Quick Connect" renders in bold weight, (2) "Connect" button is `#0F6CBD` blue with white text, (3) Input + button are centered at ~480px max width with ~8px rounded-rectangle corners (not pills), (4) No subtitle under "Quick Connect" or page title, (5) Recent Connections shows populated rows with status chips and refresh icons when servers exist, (6) Vertical scroll works when rows exceed visible area. | | |
| TASK-080 | Visual verification checklist — **Sidebar & Header**: (7) Sidebar is flush against top/left/bottom with no rounded corners, (8) Active nav item is edge-to-edge with a `PRIMARY_BLUE` left-border highlight (not a floating pill), (9) Sidebar top shows hamburger icon (+ PRISM branding when Settings active) per TASK-032, (10) Sidebar nav items have Material Symbols icons, (11) Page title sits in the header bar aligned with PRISM logo/avatar (not floating in content area), (12) Sidebar footer shows avatar + "Verified Dev" on all tabs, (13) Tab/Shift+Tab cycles focus ring through interactive widgets. | | |
| TASK-081 | Visual verification checklist — **Saved Connections**: (14) Subtitle "Browse saved desktops..." renders below the header bar above the filter pills, (15) Filter bar has `PRIMARY_BLUE` active pill + outlined inactive pills + right-aligned Filter/Sort buttons, (16) Tag-based filter pills display alongside time-based filters, (17) Card grid renders in 2–3 columns, (18) "Add New Connection" is the **last** grid item with frosted glass + dashed border + white circle + blue "+" icon, (19) Data cards show placeholder hero area + status/tag badges + heart icon + kebab menu (⋮) + server info (OS/IP/latency) + contextual action button + edit button, (20) Unreachable cards are visually dimmed, (21) FAB is visible bottom-right with blue circle + white "+", (22) Vertical scroll works for many cards. | | |
| TASK-082 | Visual verification checklist — **Profiles**: (23) "Presets" label + "+" button sit above the white list card on the gradient, (24) List items have profile-type icons, (25) Active item has pure white bg + blue left-border + green "Active" dot, (26) Editor header shows large profile icon + "ACTIVE" green chip, (27) "Save Changes" is a sharp-rectangle `PRIMARY_BLUE` button and "Discard" is text-only, (28) Section headers are uppercase with blue icons, (29) Segmented control is a unified container with solid blue active block, (30) Dropdowns in 2-column grid with chevron icons, (31) Toggle cards in 2-column grid with title + description + toggle, (32) Bitrate slider shows large value display + range labels ("5 MBPS" / "50 MBPS"), (33) Header bar shows a search input with `ICON_SEARCH` prefix and "Search profiles..." placeholder when Profiles tab is active, (34) Editor panel scrolls when content overflows. | | |
| TASK-083 | Visual verification — **Settings**: (35) Sidebar shows "SETTINGS" sub-header with sub-nav items and icons, (36) Active sub-nav item has `PRIMARY_BLUE` left-border, (37) Header bar renders breadcrumb "Settings / Identity & Security", (38) Content title and subtitle are present, (39) Dropdowns are constrained width with `ColorMode::Light` and chevrons, (40) "Last verified..." metadata renders beside trust chip, (41) Toggle cards are wrapped in `launcher_toggle_card_surface()`, (42) Audio labels are uppercase above dropdowns with speaker/mic icons. | | |
| TASK-084 | Visual verification — **Overlay**: (43) Overlay UI is unchanged (dark glass capsule + dropdowns). | | |
| TASK-085 | Visual verification — **Server Form Modal**: (44) Form uses light-mode surfaces, `PRIMARY_BLUE` primary button, 8px rounded inputs, `LT_TEXT_*` colors (per TASK-033 audit). | | |
| TASK-086 | Merge branch: `git checkout main && git merge feat/launcher-ui-polish`. Verify `main` builds: `cargo check -p prism-client`. | | |
| TASK-086a | **Capture updated screenshots:** Run the launcher and capture new `Implementation.png` screenshots for each screen (Home, Saved Connections, Profiles, Settings). Save them alongside the existing ones in `Theme/*/` so the next plan iteration has updated baselines for gap analysis. | | |

---

## 3. Alternatives

- **ALT-001**: **Texture atlas for icons** — Load a `.png` sprite sheet and render icon quads via UV mapping. Rejected: requires a separate texture pipeline, UV coordinate management, and prevents dynamic recoloring without shader changes. The `.ttf` approach reuses the existing text rendering pipeline and supports per-icon color via the standard `TextRun.color` field.
- **ALT-002**: **SVG icon rendering** — Parse and rasterize `.svg` icons at runtime. Rejected: adds a heavy dependency (e.g. `resvg`) and runtime rasterization cost. The `.ttf` approach is more aligned with the existing GPU text atlas workflow.
- **ALT-003**: **Independent per-corner radii for sidebar** — Set left corners to `0.0` and right corners to `8.0` for a subtle edge. Deferred: requires verifying that `GlassQuad` / the quad renderer supports independent corner radii. Setting all corners to `0.0` is the fastest fix; per-corner radii can be a follow-up polish item.
- **ALT-004**: **Keep sidebar floating with reduced margin** — Shrink `SIDEBAR_PAD` to `4.0` instead of `0.0`. Rejected: the Stitch design clearly shows an edge-to-edge structural panel, not a card with thin margins.
- **ALT-005**: **Radial gradient primary buttons** — `DESIGN.md` specifies a radial gradient (`#a6c8ff` → `#0067c0`) for primary CTAs. Rejected for this plan: the approved Stitch `screen.png` mockups show a flat `PRIMARY_BLUE` (`#0F6CBD`). `GlassQuad` doesn't natively support radial gradients, so this would require shader work. If the design team reintroduces the gradient, it can be a follow-up as a specialized `GradientQuad` type.

---

## 4. Scope

### This plan modifies

| # | File | Changes |
|---|------|---------|
| 1 | `crates/prism-client/src/ui/widgets/mod.rs` | Add `bold: bool` + `icon: bool` fields to `TextRun`, manual `Default` impls for `TextRun` and `GlassQuad`, register `icon` module |
| 2 | `crates/prism-client/src/ui/widgets/icon.rs` | **New file** — `Icon` widget wrapping Material Symbols codepoints |
| 3 | `crates/prism-client/src/ui/widgets/button.rs` | Audit `PRIMARY_BLUE` light path; add `ButtonStyle::Text` (no bg/border); add `.with_radius()` builder |
| 4 | `crates/prism-client/src/ui/widgets/segmented.rs` | Refactor light-mode to unified container quad with solid active block |
| 5 | `crates/prism-client/src/ui/launcher/quick_connect.rs` | Constrain input/button widths; bold text; remove subtitle; add Recent Connections list |
| 6 | `crates/prism-client/src/ui/launcher/shell.rs` | Sidebar rect `(0,0)` flush; add header bar with page title + branding + avatar; focus nav |
| 7 | `crates/prism-client/src/ui/launcher/nav.rs` | Icon rendering on nav items; flush active rect + blue left-bar; Settings sub-nav; sidebar footer avatar |
| 8 | `crates/prism-client/src/ui/theme.rs` | `launcher_sidebar_surface()` radius → `0.0`; add status chip color helpers; add divider surface helper |
| 9 | `crates/prism-client/src/renderer/text_renderer.rs` | Load bold font variant + icon `.ttf`; thread `bold`/`icon_family` into `Attrs` |
| 10 | `crates/prism-client/assets/fonts/` | `MaterialSymbolsOutlined.ttf` already present (≈ 38 KB, subsetted, Apache 2.0); may re-subset if missing codepoints (TASK-021a) |
| 11 | `crates/prism-client/src/ui/launcher/card_grid.rs` | Filter bar tag pills; multi-column grid algorithm; Add card dashed border; FAB; scroll |
| 12 | `crates/prism-client/src/ui/launcher/server_card.rs` | Card hero placeholder; status/tag/heart overlays; kebab menu; body rows; contextual footer; unreachable dimming |
| 13 | `crates/prism-client/src/ui/launcher/profiles.rs` | Presets header above list; list icons + active styling; section grouping; 2-col grids; slider display; scroll |
| 14 | `crates/prism-client/src/ui/launcher/settings.rs` | Dropdown width constraint; toggle card descriptions; audio section labels + icons; trust metadata |
| 15 | `crates/prism-client/src/config/servers.rs` | `ServerStatus` enum; `derived_status()`; add `os_label`, `tags`, `wol_supported`, `last_latency_ms` fields |
| 16 | `crates/prism-client/src/ui/launcher/server_form.rs` | Light-mode audit only (TASK-033) |
| 17 | `crates/prism-client/src/ui/launcher/layout_helpers.rs` | **New file** — `two_column_grid()` reusable layout helper (extracted from TASK-055) |
| 18 | `crates/prism-client/src/ui/widgets/dropdown.rs` | Add `.with_trailing_icon()` builder for chevron/speaker/mic icons inside dropdowns (TASK-025b) |

### This plan does NOT modify

| # | Component | Reason |
|---|-----------|--------|
| 1 | `crates/prism-client/src/ui/overlay/*` | Overlay is a dark glass capsule with its own color scheme (CON-001) |
| 2 | `crates/prism-server/` | Server-side code is unrelated to launcher UI |
| 3 | `crates/prism-display/` | Display encoding pipeline is unrelated |
| 4 | `crates/prism-transport/` | Transport layer is unrelated |
| 5 | `crates/prism-protocol/` | Protocol definitions are unrelated |
| 6 | `crates/prism-security/` | Security layer is unrelated |
| 7 | `crates/prism-session/` | Session management is unrelated |
| 8 | `crates/prism-metrics/` | Metrics collection is unrelated |
| 9 | `crates/prism-observability/` | Observability/feedback is unrelated |
| 10 | `crates/prism-platform-windows/` | Platform integration layer is unrelated |
| 11 | GPU shaders / render pipeline | No shader modifications — all changes are geometry/color/text level |
| 12 | Network protocols / handshake | No changes to connection establishment |
| 13 | `servers.json` on-disk format | New fields use `#[serde(default)]` — backward compatible |

---

## 5. Dependencies

- **DEP-001**: `MaterialSymbolsOutlined.ttf` (subsetted, ~38 KB) — already present in `crates/prism-client/assets/fonts/`. Subset coverage verified in TASK-021a; if missing codepoints, re-subset from the full variable font (~2.7 MB) available from [Google Fonts](https://fonts.google.com/icons) under Apache 2.0.
- **DEP-002**: `SavedServer` data from `crate::config::servers` — Phase 0 (TASK-P01–P04) extends this type with `ServerStatus`, `os_label`, `tags`, `wol_supported`, and `last_latency_ms`. Phase 4 and Phase 8 depend on these fields being present. The existing struct provides `id`, `display_name`, `address`, `noise_public_key`, `default_profile`, `accent_color`, `last_connected`, `last_resolution`, `last_codec`, `created_at` — but **no** `status`, `os_label`, `tags`, `wol_supported`, or `last_latency_ms`. These must be added in Phase 0.

---

## 6. Files

- **FILE-001**: `crates/prism-client/src/ui/widgets/mod.rs` — Add `bold` + `icon` fields to `TextRun`, manual `Default` impls for `TextRun` and `GlassQuad` (Phase 0), register `icon` module (Phase 5).
- **FILE-002**: `crates/prism-client/src/ui/widgets/icon.rs` — **New file.** Icon widget wrapping Material Symbols codepoints.
- **FILE-003**: `crates/prism-client/src/ui/widgets/button.rs` — Audit and fix `PRIMARY_BLUE` usage in light-mode primary branch. Also add `ButtonStyle::Text` (no bg/border) and `.with_radius()` for sharp-rectangle buttons (consolidates FILE-014).
- **FILE-004**: `crates/prism-client/src/ui/launcher/quick_connect.rs` — Constrain input layout, add bold text runs, implement connections list rendering.
- **FILE-005**: `crates/prism-client/src/ui/launcher/shell.rs` — Remove sidebar padding, fix sidebar rect to `(0,0,W,H)`, add header bar, focus navigation stub.
- **FILE-006**: `crates/prism-client/src/ui/launcher/nav.rs` — Add icon rendering to nav items, adjust item layout. Also Settings sub-nav section, sub-nav active state, sidebar footer avatar (consolidates FILE-016).
- **FILE-007**: `crates/prism-client/src/ui/theme.rs` — Change `launcher_sidebar_surface()` radius from `SIDEBAR_RADIUS` to `0.0`. Add divider reconciliation comment.
- **FILE-008**: `crates/prism-client/src/renderer/` — Load bold font variant and icon font; support font face selection.
- **FILE-009**: `crates/prism-client/assets/fonts/` — Add `MaterialSymbolsOutlined.ttf` and bold UI font variant.
- **FILE-010**: `crates/prism-client/src/ui/launcher/card_grid.rs` — Filter bar styling, grid layout algorithm, "Add New Connection" card, FAB rendering, scroll support.
- **FILE-011**: `crates/prism-client/src/ui/launcher/server_card.rs` — Complex card internal layout (hero placeholder, badges, kebab menu, body, contextual footer buttons).
- **FILE-012**: `crates/prism-client/src/ui/launcher/profiles.rs` — Presets header extraction, list item icons, active styling, editor header icon/chip, section grouping, 2-column grid layouts, slider display labels, scroll support.
- **FILE-013**: `crates/prism-client/src/ui/widgets/segmented.rs` — Refactor light-mode segmented control to unified container with solid active block.
- **FILE-014**: `crates/prism-client/src/ui/launcher/settings.rs` — Content title/subtitle, dropdown constraints, toggle card surfaces, audio section layout, device trust metadata.
- **FILE-015**: `crates/prism-client/src/config/servers.rs` — Add `ServerStatus` enum, `derived_status()`, schema extensions (`os_label`, `tags`, `wol_supported`, `last_latency_ms`).
- **FILE-016**: `crates/prism-client/src/ui/launcher/server_form.rs` — Light-mode audit/polish (TASK-033).
- **FILE-017**: `crates/prism-client/src/ui/launcher/layout_helpers.rs` — **New file.** Reusable `two_column_grid()` layout helper (Phase 9 TASK-055).
- **FILE-018**: `crates/prism-client/src/ui/widgets/dropdown.rs` — Add `.with_trailing_icon()` builder for rendering icons inside dropdowns (Phase 5 TASK-025b).

---

## 7. Testing

- **TEST-001**: `cargo check -p prism-client` after each phase — zero compilation errors.
- **TEST-002**: `cargo test -p prism-client` at Phase 11 — all existing tests pass, no regressions.
- **TEST-003**: Visual regression check: compare rendered output against `Theme/Home/screen.png` for (a) sidebar geometry, (b) hero panel typography, (c) button color, (d) input centering, (e) connections list rows, (f) icon rendering, (g) scroll behavior in list.
- **TEST-004**: Visual regression check: compare rendered output against `Theme/Connections/screen.png` for (a) filter bar pill styling (incl. tag-based pills), (b) card grid layout columns, (c) "Add" card dashed border + icon (last grid position), (d) data card hero/badges/kebab menu/body/footer, (e) unreachable card dimming, (f) FAB position and styling, (g) scroll behavior.
- **TEST-005**: Overlay mode check: toggling to overlay confirms dark glass capsule and dropdowns are unaffected by launcher polish changes.
- **TEST-006**: Visual regression check: compare rendered Profiles screen against `Theme/Profiles/screen.png` for (a) presets header placement, (b) list item icons and active styling, (c) editor header icon and chip, (d) section grouping with icons, (e) segmented control style, (f) 2-column grid layouts, (g) toggle card containment, (h) slider range display labels, (i) editor scroll behavior.
- **TEST-007**: Visual regression check: compare rendered Settings screen against `Theme/Settings/screen.png` for (a) sidebar sub-nav with icons, (b) breadcrumb header, (c) constrained light-mode dropdowns, (d) toggle card surfaces, (e) audio section uppercase labels, (f) sidebar footer avatar.
- **TEST-008**: Server form modal check: opening the Add Connection form (via FAB or Add card) renders with light-mode surfaces.
- **TEST-009**: Keyboard navigation check: Tab/Shift+Tab cycles focus ring through interactive widgets. Escape dismisses modal.
- **TEST-010**: Data-layer check: `cargo test -p prism-client` verifies `SavedServer` serialization/deserialization with new optional fields (`os_label`, `tags`, `wol_supported`, `last_latency_ms`) remains backward-compatible with existing `servers.json` snapshots.
- **TEST-011**: Icon font subset check: verify all codepoints from TASK-025 render correctly (no missing-glyph squares/tofu) at 16px, 20px, and 24px sizes against a light background.

---

## 8. Risks & Assumptions

- **ASSUMPTION-001**: The `GlassQuad` renderer accepts `radius: 0.0` without visual artifacts (no NaN or division-by-zero in rounded-rect shaders).
- **RISK-001**: `GlassQuad` may not support dashed borders natively. Mitigation: approximate with 4 thin dashed-line quads per edge, or fall back to a solid reduced-opacity white border (`[1.0,1.0,1.0,0.60]`) until a dashed-line primitive is added.
- **RISK-002**: The card hero placeholder area ideally needs independent top/bottom corner radii (rounded top, flat bottom). If the quad renderer only supports uniform radius, use `CARD_RADIUS` uniformly and accept the slight visual mismatch until per-corner radius support is added.
- **ASSUMPTION-002**: The grid layout can be computed in `card_grid.rs` without a dedicated layout engine — simple row-by-row column packing with fixed card widths is sufficient.
- **RISK-003**: Adding `ButtonStyle::Text` (no background/border) is a new variant that may need to pass through event handling code that assumes buttons always have a visible rect. Mitigation: ensure hit-testing uses the text bounding rect even when no background quad is rendered.
- **RISK-004**: The 2-column grid layout in the Profiles editor depends on sufficient `editor_body_w`. On very narrow windows, the 2-column layout could overflow. Mitigation: fall back to single-column when `editor_body_w < 400.0`.
- **RISK-005**: Italic text style (TASK-071) may not be available for the current UI font if only Regular and Bold weights are loaded. Mitigation: if `Style::Italic` produces no visible change, fall back to normal style — the metadata text is still useful without italics.
- **RISK-006**: The sidebar sub-nav adds vertical items that could overflow the sidebar on short windows. Mitigation: only render sub-nav items when the Settings tab is active, collapsing them when other tabs are selected.
- **RISK-007**: **`text_width()` approximation** — `theme.rs` uses `text.len() * font_size * 0.52` as a heuristic. This will be inaccurate for bold text (wider glyphs), icon codepoints (single char at variable width), and non-ASCII characters. Tasks that depend on precise centering or alignment (TASK-007 input centering, TASK-029 header layout, TASK-046 FAB positioning, TASK-055 2-column grid) may have visual misalignment. Mitigation: TASK-025a adds a `text_width_exact(font_system, text, font_size, bold) -> f32` helper in `text_renderer.rs` that queries `glyphon`'s `Buffer` after `set_text()` + `shape_until_scroll()` to get actual run width. Use `text_width_exact()` for critical centering tasks; use the heuristic only for non-critical layout.
- **RISK-008**: **Card hero images** — TASK-041 renders a flat placeholder gradient. The design shows actual desktop screenshots. Adding real image support requires a texture loading pipeline (image decode → GPU texture → UV-mapped quad), image caching, and lazy loading. This is out of scope for this plan. Tracked as a follow-up.
- **RISK-009**: **Server form modal polish** — TASK-033 audits `server_form.rs` for light-mode style compliance. If the form is deeply entangled with dark-mode assumptions, the audit scope may expand. Mitigation: limit to surface/color changes; defer structural form redesign.
- **RISK-010**: **Animation & transitions** — The `DESIGN.md` spec mentions "soft pulsing glow effect" for Sleeping chips and hover transitions. This plan only addresses static rendering. GPU-driven animations (glow pulse, hover ease-in/out) require a frame-time delta system and animation state tracking. **Deferred** to a follow-up plan. Static hover-state changes (immediate tint swap on `hovered` flag) are in scope.
- **RISK-011**: **Backward compatibility of `SavedServer` schema** — TASK-P03 adds optional fields. Existing `servers.json` snapshots must deserialize correctly without these fields. Mitigation: all new fields use `Option<T>` with `#[serde(default)]`, or `Vec<T>` (defaults to empty). `ServerStore::compact()` will re-serialize with the new schema on next write. Add a unit test verifying old JSON still loads.
- **ASSUMPTION-003**: The `ServerStatus` heuristic in `derived_status()` (TASK-P02) is a temporary stand-in. Real-time status requires either (a) periodic ICMP/TCP pings from the client, or (b) a discovery protocol response from the server. Both are out of scope for this UI-polish plan.

---

## 9. Related Specifications / Further Reading

- [2026-04-02 Launcher Light-Theme Fix](2026-04-02-launcher-light-theme-fix.md) — Prior plan establishing the light-mode color system.
- `Theme/Home/screen.png` — Stitch design target for the Home screen.
- `Theme/Home/code.html` — Stitch HTML/CSS reference for exact CSS values.
- `Theme/Connections/DESIGN.md` — Design spec for Saved Connections screen.
- `Theme/Connections/screen.png` — Stitch design target for the Saved Connections screen.
- `Theme/Profiles/screen.png` — Stitch design target for the Profiles screen.
- `Theme/Profiles/DESIGN.md` — Design spec for Profiles screen.
- `Theme/Settings/screen.png` — Stitch design target for the Settings screen.
- `Theme/Settings/DESIGN.md` — Design spec for Settings screen.
- [Material Symbols — Google Fonts](https://fonts.google.com/icons) — Icon font source (Apache 2.0 license).
- [glyphon](https://github.com/grovesNL/glyphon) — GPU text rendering crate (assumed text renderer).
