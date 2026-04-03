# Launcher Light-Theme Corrective Plan

> **Prior art:** `docs/superpowers/plans/2026-04-01-launcher-ui-redesign.md`
> built the right layout structure (sidebar + hero + cards/rows + modals)
> but applied the **wrong color system** — dark overlay glass everywhere
> instead of the Stitch-designed light Mica/frosted-white launcher palette.

**Problem:**
Every surface helper in `crates/prism-client/src/ui/theme.rs` paints dark
tinted glass (`rgba(0.10-0.17, 0.14-0.22, 0.19-0.29, 0.72-0.94)`). This
produces a look identical to the Overlay design (Theme/Overlay), which is
intended to float on top of a live remote stream.

The four launcher screens (Home, Connections, Profiles, Settings) were
designed in Stitch with a completely different visual system:
- Light blue gradient background (`#8faee0` base with radial highlights)
- White frosted glass panels (`rgba(255,255,255, 0.45-0.85)`)
- Cream sidebar (`#EBF1F6`)
- Dark text on light surfaces (`#0f172a` / `#475569`)
- Corporate blue primary button (`#0F6CBD`)

The Overlay screen is the only one that should use translucent panels over
an arbitrary background (the remote desktop stream). The launcher has its
own fixed gradient backdrop and needs opaque-ish light surfaces.

**Goal:** Introduce a dual-palette system in theme.rs so PRISM can render
light Mica surfaces for the launcher and keep the existing dark glass
surfaces for the overlay. No layout/widget/routing/persistence changes.

---

## Screen-by-Screen Gap Analysis

> Each section compares the current implementation (`Theme/*/Implementation.png`)
> against the Stitch design target (`Theme/*/screen.png` + `code.html`).
> The same systemic issue — dark overlay glass instead of light Mica — affects
> every screen, but each screen also has unique visual gaps.

### Screen 1: Home

**Implementation (current):**
- Dark slate-gray background fills the entire window
- Dark glass sidebar (`rgba ~0.10,0.14,0.19`) with light text
- Dark glass hero panel with "Quick Connect" in light/white text
- Light cyan/ice-blue Connect button with muted text
- Dark glass input field with light placeholder text
- "Recent Connections" label in small muted light text
- Dark separator line
- Empty state "No saved desktops match this filter." in light muted text

**Design target (Stitch):**
- Light blue gradient background (`#8faee0` with radial highlights)
- Cream/light sidebar (`#EBF1F6`) with dark text and icons
- White frosted glass hero panel (`rgba(255,255,255,0.45)`) with DARK bold text
- Deep blue Connect button (`#0F6CBD`) with white text
- White/light input field (`bg-white/80`) with gray placeholder
- "Recent Connections" as dark semibold section heading (`text-xl`)
- White frosted list container (`rgba(255,255,255,0.85)`) with row items
- Rows show: numbered index, dark server name, pastel status chips (green/yellow/red with dark text), last-connected timestamp, outlined Reconnect button

**Key gaps:**
| Element | Current | Target | Fix |
|---------|---------|--------|-----|
| Window background | Near-black `[0.047,0.063,0.094]` | Light blue gradient `#8faee0` | `LAUNCHER_BACKDROP` |
| Sidebar | Dark glass, light text | Cream `#EBF1F6`, dark text | `launcher_sidebar_surface()` + `LT_TEXT_*` |
| Hero panel | Dark glass `[0.15,0.19,0.26,0.76]` | White glass `rgba(255,255,255,0.45)` | `launcher_hero_surface()` |
| Hero title | Light text `TEXT_PRIMARY` | Dark bold text `#0f172a` | `LT_TEXT_PRIMARY` |
| Input field | Dark surface, light text | White surface `bg-white/80`, dark text | `launcher_control_surface()` + `LT_TEXT_*` |
| Connect button | Cyan accent glow | Deep blue `#0F6CBD` | `PRIMARY_BLUE` + `ColorMode::Light` |
| Section header | 13px muted light | xl semibold dark `#1e293b` | `LT_TEXT_PRIMARY`, larger size |
| Status chips | Translucent tint, light text | Pastel opaque bg, dark text | `launcher_status_chip()` + `launcher_chip_text_color()` |
| Row container | Dark glass rows | White glass list container | `launcher_list_surface()` |
| Row hover | Dark glass highlight | `rgba(255,255,255,0.40)` overlay | `launcher_row_surface()` |
| Nav icons | Missing (text only) | Font Awesome icons | Deferred (no icon primitive) |
| Top-right bar | Missing | PRISM logo + avatar + window controls | Deferred (layout work) |

---

### Screen 2: Saved Connections

**Implementation (current):**
- Same dark background/sidebar as Home
- "Saved Connections" title + subtitle in light text
- Filter chip bar: "All Hosts (0)" active in cyan, "Recent"/"Dormant"/"New" in dark glass chips with light text
- Empty state "No saved desktops match this filter." in muted light text
- "Add New Connection" card: dark glass card with dashed border, light text, "+" circle
- No populated server cards visible (empty state)

**Design target (Stitch):**
- Light blue gradient background
- Light sidebar with icons
- Dark bold title "Saved Connections" on light bg
- Filter chips: "All Hosts (14)" in solid blue `#0F6CBD`, "Work"/"Personal"/"Linux Servers" as outlined light chips with dark text
- Right-aligned "Filter" and "Last Connected" sort buttons
- Rich server cards on white glass: hero image, status chips (ONLINE green / SLEEPING yellow / UNREACHABLE gray), tag chips (WORK/PERSONAL), server name in dark bold, OS + IP subtitle, last-connected time, latency badge, action buttons (Connect/Wake & Connect/Retry Discovery + edit icon)
- Dashed "Add New Connection" card in white glass with dashed border
- FAB button (blue "+" circle) in bottom-right corner

**Key gaps:**
| Element | Current | Target | Fix |
|---------|---------|--------|-----|
| Background | Dark | Light blue gradient | `LAUNCHER_BACKDROP` |
| Filter chips | Dark glass, cyan active | White outlined, blue active | `launcher_status_chip()` variants + `PRIMARY_BLUE` |
| Filter labels | Light text | Dark text on light chips | `LT_TEXT_*` |
| Card surface | Dark glass `[0.14,0.18,0.24,0.72]` | White glass `rgba(255,255,255,0.65)` | `launcher_card_surface()` |
| Card text | Light on dark | Dark on light | `LT_TEXT_PRIMARY` / `LT_TEXT_SECONDARY` |
| Card hover | Dark accent overlay | White opacity increase | `launcher_card_hover()` |
| Add card | Dark glass, dashed | White glass, dashed | `launcher_card_surface()` + dashed border |
| Status chips | Translucent tint, light text | Pastel opaque bg, dark text | `launcher_status_chip()` |
| Hero images | Not supported | Stock photos per server | Deferred (no image widget) |
| Tag chips (WORK etc) | Not supported | Chip with dark text | Deferred (data model) |
| Sort/filter toolbar | Not present | Filter + Last Connected buttons | Deferred (layout) |
| FAB button | Not present | Blue "+" bottom-right | Deferred (layout) |
| Card action variants | Connect/Edit/Del | Connect/Wake/Retry/Edit | Contextual button style per status (see below) |
| Active filter chip | Cyan bg | Solid `PRIMARY_BLUE` bg + white text | Primary button surface, NOT pastel ChipTone |
| Inactive filter chips | Dark glass | `bg-white/50 border-white/60` + dark text | `launcher_control_surface()` + `LT_TEXT_SECONDARY` |
| Unreachable card dimming | Same as other cards | `opacity-80`, gray title, disabled button | Reduced alpha card + `LT_TEXT_MUTED` title + disabled button style |

**Contextual action buttons per card status:**

| Status | Button | Style |
|--------|--------|-------|
| Online | "Connect" | Solid `bg-primary text-white font-semibold rounded-lg` |
| Sleeping | "Wake & Connect" | Outlined `bg-white text-slate-800 border-slate-300` with bolt icon |
| Unreachable | "Retry Discovery" | Disabled `bg-slate-200/50 text-slate-500 cursor-not-allowed` |

---

### Screen 3: Profiles

**Implementation (current):**
- Same dark background/sidebar
- "Profiles" title + subtitle in light text
- Two-column layout: left preset list, right editor panel — structure is correct
- Left list: "Presets" header, four entries (Gaming/Coding/Balanced/Low Bandwidth) with summary, selected item has cyan-tinted dark glass highlight
- Right editor: "Gaming" profile name in large light text, "SYSTEM" chip, Discard/Save buttons
- Editor sections: Bitrate Preference (slider), Max FPS (dropdown), Latency vs Quality (segmented control), Audio Mode (dropdown), toggles for Native Scaling/Exclusive Input/Prefer AV1/Touch Mode/Auto Reconnect
- All surfaces are dark glass, all text is light-on-dark
- Segmented control: cyan active segment, dark inactive segments

**Design target (Stitch):**
- Light blue gradient background, light sidebar with icon indicators
- Search bar in header area
- "Presets" header with "+" add button
- Left list: white glass panel, items have profile icons, name in dark bold, subtitle in dark muted, active indicator (green dot "Active")
- Right editor: white glass panel, profile icon, "Gaming" in dark bold, "ACTIVE" chip in dark blue, "Save Changes" in blue button, "Discard" as text button
- Section headers ("PERFORMANCE SETTINGS", "DISPLAY & AUDIO", "INPUT & CONNECTIVITY") in dark muted uppercase with icon
- Bitrate: large numeric display "35 MBPS", slider with min/max labels
- Latency vs Quality: rounded segmented control, blue active segment
- Dropdowns (Display Scaling, Audio Mode, Codec Preference): white bordered, dark text, chevron
- Toggle cards: white rounded cards with title, description, toggle — "Exclusive Input"/"Touch Mode"/"Auto-Reconnect"
- All text is dark on light surfaces

**Key gaps:**
| Element | Current | Target | Fix |
|---------|---------|--------|-----|
| Background | Dark | Light blue gradient | `LAUNCHER_BACKDROP` |
| Left list panel | Dark glass | White glass `rgba(255,255,255,0.85)` | `launcher_list_surface()` |
| List item highlight | Cyan-tinted dark glass | Blue left-bar + white bg | `launcher_nav_item_surface()` |
| Right editor panel | Dark glass | White glass `rgba(255,255,255,0.45)` | `launcher_hero_surface()` |
| Profile name text | Light text | Dark bold text | `LT_TEXT_PRIMARY` |
| "SYSTEM" chip | Translucent tint | Opaque blue "ACTIVE" chip | `launcher_status_chip(Accent)` |
| Save button | Cyan accent | Deep blue `#0F6CBD` | `PRIMARY_BLUE` + `ColorMode::Light` |
| Section headers | Light muted text | Dark uppercase muted with icon dividers | `LT_TEXT_MUTED` + uppercase + `tracking-[0.2em]` + separator |
| Segmented control | Cyan active on dark | `bg-white/50 border-white rounded-xl` container | `SEGMENTED_CONTAINER_LIGHT` + `SEGMENTED_ACTIVE_LIGHT` |
| Dropdowns | Dark glass | `bg-white/80 border-white rounded-xl` with chevron | `launcher_control_surface()` |
| Toggle cards | Dark glass toggles | `bg-white/60 border-white rounded-2xl p-5` with title+desc | `launcher_toggle_card_surface()` (distinct from inputs) |
| Slider | Cyan on dark track | Light blue track `rgba(primary,0.1)`, blue thumb, white border | `SLIDER_TRACK_LIGHT` + `SLIDER_THUMB_LIGHT` |
| Slider min/max labels | Not present | "5 MBPS" / "50 MBPS" below slider | `LT_TEXT_MUTED` at 11px, bold uppercase |
| Bitrate readout | Small monochrome text | Large `text-2xl font-bold text-primary` "35" + small "MBPS" | `PRIMARY_BLUE` large text + `LT_TEXT_MUTED` unit label |
| Editor header zone | Same as editor panel | `bg-white/30 border-b border-white/40` sub-surface | Layered white overlay + `launcher_inner_separator()` |
| Advanced Config section | Not present | Expandable section: separator + uppercase header + chevron | `launcher_separator()` + collapsible header row |
| Active list item | Cyan-tinted highlight | `border-l-4 border-primary bg-white/60` + green "Active" dot | Blue left-bar + white bg + green dot |
| Profile icons | Not supported | Gamepad/code/balance/bandwidth icons | Deferred (no icon primitive) |
| Search bar | Not present | Search input in header | Deferred (layout) |
| "+" add profile | Not present | "+" button in Presets header | Deferred (layout) |

---

### Screen 4: Settings

**Implementation (current):**
- Same dark background/sidebar
- "Settings" title + subtitle in light text
- Single scrollable page with "Identity & Security" section header in large light text
- Identity Path: label + description in light text, value shown in a chip/pill (truncated path)
- Device Trust: label + "Trusted Device" green chip
- Streaming Defaults: dropdown ("Balanced")
- Input: "Exclusive Keyboard Capture" and "Relative Mouse Movement" shown as labeled toggle rows in lighter glass containers
- Audio: "System Default" dropdowns for Remote Output and Local Mic Path
- "PRISM Professional Edition • 0.1.0" footer text
- Everything is dark glass surfaces with light text
- Toggle rows use a slightly lighter gray glass — readable but still dark

**Design target (Stitch):**
- Light blue gradient background, light sidebar
- Settings has its own **sidebar sub-navigation**: General, Identity & Security (active, with blue left-bar), Streaming, Input, Audio — with section icons
- Breadcrumb "Settings / Identity & Security" in header
- Large dark bold "Identity & Security" title + subtitle
- Content in white frosted glass container
- Identity Path: dark label, value in light blue/lavender rounded field
- Device Trust: green dot + "Trusted Device" green chip + "Last verified 2 hours ago" italic text
- Streaming Defaults: white bordered dropdown with dark text
- Input: white rounded cards with title, description, blue toggle — same card treatment as Profiles
- Audio: "REMOTE OUTPUT" / "LOCAL MIC PATH" uppercase labels, white bordered fields with icons
- Bottom-left: user avatar + "Verified Dev" name
- All text is dark on light

**Key gaps:**
| Element | Current | Target | Fix |
|---------|---------|--------|-----|
| Background | Dark | Light blue gradient | `LAUNCHER_BACKDROP` |
| Settings sidebar | Not present (single page) | Sub-nav with sections | **Approved decision: keep single page** — adapt visually only |
| Breadcrumb | Not present | "Settings / Identity & Security" | Deferred (decided: single page) |
| Content container | No wrapper panel | White frosted glass panel | `launcher_hero_surface()` wrapping all settings |
| Section headers | Large light text | Dark bold text | `LT_TEXT_PRIMARY` |
| Setting labels | Light text | Dark text + muted description | `LT_TEXT_PRIMARY` + `LT_TEXT_SECONDARY` |
| Identity path field | Chip with truncated path | Lavender rounded field | `launcher_control_surface()` with accent tint |
| Trust chip | Green translucent tint | Green pastel opaque bg + dark text | `launcher_status_chip(Success)` |
| Trust metadata | Not present | "Last verified 2 hours ago" | Deferred (data enrichment) |
| Dropdowns | Dark glass | White bordered with chevron | `launcher_control_surface()` + `ColorMode::Light` |
| Toggle rows | Lighter dark glass | `bg-white/30 border-white/40 rounded-lg` cards with title+desc | `launcher_toggle_card_surface()` at 0.30 alpha (lighter than Profiles) |
| Toggles | Cyan/dark track | Blue/light track | `ColorMode::Light` toggle |
| In-panel dividers | Not distinct | `bg-black/5` (subtle on white glass) | `launcher_inner_separator()` (NOT `launcher_separator()`) |
| Audio labels | Basic labels | Uppercase "REMOTE OUTPUT" / "LOCAL MIC PATH" | `LT_TEXT_MUTED` at 10px, bold uppercase, `tracking-[0.2em]` |
| Audio field icons | Not present | Headphone/mic icons | Deferred (no icon primitive) |
| User avatar + name | Not present | Bottom-left "Verified Dev" | Deferred (identity display) |
| Section icons | Not present | Shield, streaming, keyboard, speaker icons | Deferred (no icon primitive) |

---

### Cross-Screen Summary

**The systemic root cause is the same for all 4 launcher screens:**
Every surface helper (`hero_surface`, `card_surface`, `sidebar_surface`,
`list_row_surface`, `control_surface`, etc.) produces dark tinted glass
intended for the overlay. The launcher needs its own set of **light Mica**
surface helpers.

**Changes that fix all screens at once (high leverage):**
1. `LAUNCHER_BACKDROP` — flips the window from dark to light blue
2. `launcher_sidebar_surface()` — cream sidebar for all screens
3. `LT_TEXT_PRIMARY` / `LT_TEXT_SECONDARY` / `LT_TEXT_MUTED` — dark text palette
4. `launcher_hero_surface()` — white frosted glass for hero panels
5. `launcher_card_surface()` / `launcher_card_hover()` — white glass cards
6. `launcher_list_surface()` — white glass list containers
7. `launcher_control_surface()` — white inputs/dropdowns with gray border
8. `launcher_status_chip()` / `launcher_chip_text_color()` — pastel opaque chips
9. `PRIMARY_BLUE` — deep blue button color replacing cyan accent
10. `ColorMode::Light` on widgets — propagates light rendering through buttons/inputs/toggles/dropdowns
11. `launcher_toggle_card_surface()` — white toggle cards (distinct from inputs)
12. `launcher_inner_separator()` — subtle in-panel dividers (`bg-black/5`)
13. `SLIDER_TRACK_LIGHT` / `SLIDER_THUMB_LIGHT` — light-mode slider tokens
14. `SEGMENTED_CONTAINER_LIGHT` / `SEGMENTED_ACTIVE_LIGHT` — light-mode segmented control tokens

**Per-screen items that remain deferred:**
- Icons throughout (no icon widget primitive)
- Top-right header bar (PRISM branding + avatar + window controls)
- Server card hero images (no image widget)
- Tag/category system for connections (data model)
- Settings sub-navigation sidebar (approved decision: keep single page)
- Profile search bar (layout change)
- User identity display in sidebar footer
- FAB button (circular button + z-layer)

**Polish items (optional, capability-dependent):**
- Fluent-input bottom-accent variant for Settings controls
- Shadow vocabulary tokens (if `GlowRect` supports colored shadows)
- Per-surface blur radius (if `GlassQuad` supports configurable blur)
- Section header letter-spacing (if `TextRun` supports letter-spacing)
- Blue-tinted primary button shadow
- Card hover Y-translation lift effect (if layout supports animated offsets)

---

## Scope

This plan modifies:
- `crates/prism-client/src/ui/theme.rs` — add light-mode constants, launcher surface helpers, launcher backdrop, launcher text colors
- `crates/prism-client/src/app.rs` — use light backdrop color when in Launcher state
- `crates/prism-client/src/ui/launcher/shell.rs` — switch to light surface helpers
- `crates/prism-client/src/ui/launcher/nav.rs` — switch to light sidebar/nav helpers
- `crates/prism-client/src/ui/launcher/quick_connect.rs` — switch to light hero surface, dark text
- `crates/prism-client/src/ui/launcher/card_grid.rs` — switch to light card/row surfaces, dark text
- `crates/prism-client/src/ui/launcher/server_card.rs` — switch to light card/row surfaces, dark text
- `crates/prism-client/src/ui/launcher/server_form.rs` — switch to light modal surface, dark text
- `crates/prism-client/src/ui/launcher/profiles.rs` — switch to light panel/list surfaces, dark text
- `crates/prism-client/src/ui/launcher/settings.rs` — switch to light panel/list surfaces, dark text
- `crates/prism-client/src/ui/widgets/button.rs` — support a `Primary` button color matching `#0F6CBD` rather than cyan accent
- `crates/prism-client/src/ui/widgets/text_input.rs` — support light-mode input surface
- `crates/prism-client/src/ui/widgets/toggle.rs` — support light-mode toggle track
- `crates/prism-client/src/ui/widgets/dropdown.rs` — support light-mode dropdown surface

This plan does NOT modify:
- Overlay files (`crates/prism-client/src/ui/overlay/*`) — they keep dark glass
- `UiAction` / `SessionBridge` / persistence / routing contracts
- Layout geometry or widget structure
- `crates/prism-server/` or `crates/prism-session/`

---

## Stitch Design Language Reference (Launcher Screens)

All four launcher screens share:

| Token | CSS Value | Normalized RGBA |
|-------|-----------|-----------------|
| Background base | `#8faee0` | `[0.561, 0.682, 0.878]` |
| Background gradient TL | `rgba(161,196,253,0.8)` | `[0.631, 0.769, 0.992, 0.8]` |
| Background gradient BR | `rgba(194,233,251,0.8)` | `[0.761, 0.914, 0.984, 0.8]` |
| Sidebar bg | `#EBF1F6` | `[0.922, 0.945, 0.965, 1.0]` |
| Glass panel | `rgba(255,255,255,0.45)` blur 20 | `[1.0, 1.0, 1.0, 0.45]` |
| Glass card | `rgba(255,255,255,0.65)` blur 15 | `[1.0, 1.0, 1.0, 0.65]` |
| Glass card hover | `rgba(255,255,255,0.85)` | `[1.0, 1.0, 1.0, 0.85]` |
| Glass list | `rgba(255,255,255,0.85)` blur 10 | `[1.0, 1.0, 1.0, 0.85]` |
| Panel border | `rgba(255,255,255,0.5)` | `[1.0, 1.0, 1.0, 0.50]` |
| Text primary | `#0f172a` | `[0.059, 0.090, 0.165, 1.0]` |
| Text secondary | `#475569` | `[0.278, 0.333, 0.412, 1.0]` |
| Text muted | `#94a3b8` | `[0.580, 0.639, 0.722, 1.0]` |
| Primary button | `#0F6CBD` | `[0.059, 0.424, 0.741, 1.0]` |
| Primary hover | `#1D5BA0` approx | `[0.114, 0.357, 0.627, 1.0]` |
| Chip green bg | `bg-green-100` / `#dcfce7` | `[0.863, 0.988, 0.906, 1.0]` |
| Chip green text | `text-green-800` / `#166534` | `[0.086, 0.396, 0.204, 1.0]` |
| Chip yellow bg | `bg-yellow-100` / `#fef9c3` | `[0.996, 0.976, 0.765, 1.0]` |
| Chip yellow text | `text-yellow-800` / `#854d0e` | `[0.522, 0.302, 0.055, 1.0]` |
| Chip red bg | `bg-red-100` / `#fee2e2` | `[0.996, 0.886, 0.886, 1.0]` |
| Chip red text | `text-red-800` / `#991b1b` | `[0.600, 0.106, 0.106, 1.0]` |
| Active nav item bg | `bg-gray-200/60` | `[0.898, 0.898, 0.898, 0.60]` |
| Active nav indicator | `bg-primary` left bar | `[0.059, 0.424, 0.741, 1.0]` |
| Control border | `border-gray-300` | `[0.831, 0.843, 0.863, 1.0]` |
| Control bg | `bg-white/80` | `[1.0, 1.0, 1.0, 0.80]` |
| Section divider | `divide-gray-200/50` | `[0.898, 0.898, 0.898, 0.50]` |
| Inner-panel divider | `bg-black/5` | `[0.0, 0.0, 0.0, 0.05]` |
| Toggle card (Profiles) | `bg-white/60 border-white` | `[1.0, 1.0, 1.0, 0.60]` |
| Toggle card (Settings) | `bg-white/30 border-white/40` | `[1.0, 1.0, 1.0, 0.30]` |
| Editor header sub-surface | `bg-white/30 border-b border-white/40` | `[1.0, 1.0, 1.0, 0.30]` |
| Slider track | `rgba(15,108,189,0.1)` | `[0.059, 0.424, 0.741, 0.10]` |
| Slider thumb | `#0F6CBD` + `2px solid white` | `[0.059, 0.424, 0.741, 1.0]` |
| Segmented container | `bg-white/50 border-white rounded-xl` | `[1.0, 1.0, 1.0, 0.50]` |
| Segmented active | `bg-primary rounded-lg shadow-sm` | `[0.059, 0.424, 0.741, 1.0]` |
| Section header tracking | `tracking-[0.2em]` | 0.2em letter-spacing |

The Overlay screen is different — it uses `glass-panel-light` with
`rgba(255,255,255,0.45)` over an arbitrary stream background, but the
overlay already keeps PRISM's existing dark-glass capsule + dropdown
model per the approved decision. No changes needed there.

---

## Task 1: Light-Mode Theme Tokens

**File:** `crates/prism-client/src/ui/theme.rs`

- [ ] **Step 1: Add launcher backdrop constant**

```rust
/// Launcher background — light blue base matching Stitch gradient.
/// Used as the wgpu clear-color when UiState == Launcher.
pub const LAUNCHER_BACKDROP: [f64; 3] = [0.561, 0.682, 0.878];
```

- [ ] **Step 2: Add launcher text color constants**

```rust
// Light-mode text (dark on light surfaces) — launcher only
pub const LT_TEXT_PRIMARY: [f32; 4]   = [0.059, 0.090, 0.165, 1.0];  // #0f172a
pub const LT_TEXT_SECONDARY: [f32; 4] = [0.278, 0.333, 0.412, 1.0];  // #475569
pub const LT_TEXT_MUTED: [f32; 4]     = [0.580, 0.639, 0.722, 1.0];  // #94a3b8
```

- [ ] **Step 3: Add launcher primary button color**

```rust
pub const PRIMARY_BLUE: [f32; 4] = [0.059, 0.424, 0.741, 1.0]; // #0F6CBD
```

- [ ] **Step 4: Add launcher surface helpers**

```rust
/// Launcher sidebar — cream Mica tint.
pub fn launcher_sidebar_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.922, 0.945, 0.965, 0.92],  // #EBF1F6 at high opacity
        [1.0, 1.0, 1.0, 0.40],
        SIDEBAR_RADIUS,
    )
}

/// Launcher hero / glass-panel — white frosted glass.
pub fn launcher_hero_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.45],
        [1.0, 1.0, 1.0, 0.50],
        HERO_RADIUS,
    )
}

/// Launcher card — white glass, medium opacity.
pub fn launcher_card_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.65],
        [1.0, 1.0, 1.0, 0.70],
        CARD_RADIUS,
    )
}

/// Launcher card hover overlay.
pub fn launcher_card_hover(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.85],
        [1.0, 1.0, 1.0, 0.80],
        CARD_RADIUS,
    )
}

/// Launcher list container — nearly opaque white.
pub fn launcher_list_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.85],
        [1.0, 1.0, 1.0, 0.60],
        CONTROL_RADIUS,
    )
}

/// Launcher list row hover.
pub fn launcher_row_surface(rect: Rect, hovered: bool) -> GlassQuad {
    glass_quad(
        rect,
        if hovered {
            [1.0, 1.0, 1.0, 0.40]
        } else {
            [0.0, 0.0, 0.0, 0.0]   // transparent, list bg shows through
        },
        [0.0, 0.0, 0.0, 0.0],
        0.0,
    )
}

/// Launcher active nav item background.
pub fn launcher_nav_item_surface(rect: Rect, active: bool, hovered: bool) -> GlassQuad {
    glass_quad(
        rect,
        if active {
            [0.898, 0.898, 0.898, 0.60]
        } else if hovered {
            [0.898, 0.898, 0.898, 0.30]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        },
        [0.0, 0.0, 0.0, 0.0],
        CONTROL_RADIUS,
    )
}

/// Launcher control (text input, dropdown) — white with subtle border.
pub fn launcher_control_surface(rect: Rect, focused: bool) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.80],
        if focused {
            [PRIMARY_BLUE[0], PRIMARY_BLUE[1], PRIMARY_BLUE[2], 0.60]
        } else {
            [0.831, 0.843, 0.863, 1.0]  // border-gray-300
        },
        CONTROL_RADIUS,
    )
}

/// Launcher modal surface — white panel.
pub fn launcher_modal_surface(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, 0.92],
        [1.0, 1.0, 1.0, 0.60],
        MODAL_RADIUS,
    )
}

/// Launcher separator — light gray divider (between sections on gradient bg).
pub fn launcher_separator(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.898, 0.898, 0.898, 0.50],
        [0.0, 0.0, 0.0, 0.0],
        0.0,
    )
}

/// Subtle in-panel divider — used inside white glass containers.
/// Different from launcher_separator() which sits on the gradient background.
pub fn launcher_inner_separator(rect: Rect) -> GlassQuad {
    glass_quad(
        rect,
        [0.0, 0.0, 0.0, 0.05],  // bg-black/5
        [0.0, 0.0, 0.0, 0.0],
        0.0,
    )
}

/// Toggle card surface — Profiles uses ~60% white, Settings uses ~30% white.
/// Both are distinct from launcher_control_surface() (80% white for inputs).
pub fn launcher_toggle_card_surface(rect: Rect, alpha: f32) -> GlassQuad {
    glass_quad(
        rect,
        [1.0, 1.0, 1.0, alpha],  // 0.60 for Profiles, 0.30 for Settings
        [1.0, 1.0, 1.0, 0.80],
        CARD_RADIUS,  // rounded-2xl = 16px
    )
}

/// Light-mode status chip — opaque pastel bg with colored text.
pub fn launcher_status_chip(rect: Rect, tone: ChipTone) -> GlassQuad {
    let (tint, border) = match tone {
        ChipTone::Success => (
            [0.863, 0.988, 0.906, 1.0],  // bg-green-100
            [0.745, 0.933, 0.820, 1.0],  // border-green-200
        ),
        ChipTone::Warning => (
            [0.996, 0.976, 0.765, 1.0],  // bg-yellow-100
            [0.988, 0.933, 0.600, 1.0],  // border-yellow-200
        ),
        ChipTone::Danger => (
            [0.996, 0.886, 0.886, 1.0],  // bg-red-100
            [0.988, 0.808, 0.808, 1.0],  // border-red-200
        ),
        ChipTone::Accent => (
            [0.855, 0.922, 0.996, 1.0],  // bg-blue-100
            [0.745, 0.867, 0.988, 1.0],  // border-blue-200
        ),
        ChipTone::Neutral => (
            [0.945, 0.949, 0.957, 1.0],  // bg-gray-100
            [0.898, 0.898, 0.898, 1.0],  // border-gray-200
        ),
    };
    glass_quad(rect, tint, border, CHIP_RADIUS)
}

/// Returns the text color for a light-mode status chip label.
pub fn launcher_chip_text_color(tone: ChipTone) -> [f32; 4] {
    match tone {
        ChipTone::Success => [0.086, 0.396, 0.204, 1.0],  // text-green-800
        ChipTone::Warning => [0.522, 0.302, 0.055, 1.0],  // text-yellow-800
        ChipTone::Danger  => [0.600, 0.106, 0.106, 1.0],  // text-red-800
        ChipTone::Accent  => [0.114, 0.357, 0.627, 1.0],  // text-blue-800
        ChipTone::Neutral => [0.278, 0.333, 0.412, 1.0],  // text-gray-600
    }
}
```

- [ ] **Step 5: Add slider and segmented control tokens**

```rust
// Light-mode slider
pub const SLIDER_TRACK_LIGHT: [f32; 4] = [0.059, 0.424, 0.741, 0.10]; // primary at 10%
pub const SLIDER_THUMB_LIGHT: [f32; 4] = PRIMARY_BLUE;
pub const SLIDER_THUMB_BORDER: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

// Light-mode segmented control
pub const SEGMENTED_CONTAINER_LIGHT: [f32; 4] = [1.0, 1.0, 1.0, 0.50]; // bg-white/50
pub const SEGMENTED_ACTIVE_LIGHT: [f32; 4] = PRIMARY_BLUE;  // bg
// Active text: white [1.0, 1.0, 1.0, 1.0]. Inactive text: LT_TEXT_MUTED.
```

- [ ] **Step 6: Run compilation check**

```bash
cargo check -p prism-client
```

- [ ] **Step 7: Commit**

```bash
git add crates/prism-client/src/ui/theme.rs
git commit -m "feat(theme): add light-mode launcher surface helpers and text colors"
```

---

## Task 2: Launcher Backdrop Switch

**File:** `crates/prism-client/src/app.rs`

- [ ] **Step 1: Use LAUNCHER_BACKDROP when rendering in Launcher state**

Find the two places where `theme::BACKDROP` is used as the wgpu clear-color.
When `self.ui_state == UiState::Launcher`, use `theme::LAUNCHER_BACKDROP`
instead. When in `UiState::Overlay` or `UiState::Stream`, keep `theme::BACKDROP`.

This single change flips the entire window background from near-black to
light blue when the launcher is open.

- [ ] **Step 2: Verify visually and commit**

```bash
cargo check -p prism-client
git add crates/prism-client/src/app.rs
git commit -m "feat(app): use light blue backdrop for launcher state"
```

---

## Task 3: Widget Light-Mode Support

**Files:**
- `crates/prism-client/src/ui/widgets/mod.rs`
- `crates/prism-client/src/ui/widgets/button.rs`
- `crates/prism-client/src/ui/widgets/text_input.rs`
- `crates/prism-client/src/ui/widgets/dropdown.rs`
- `crates/prism-client/src/ui/widgets/toggle.rs`
- `crates/prism-client/src/ui/widgets/slider.rs`
- `crates/prism-client/src/ui/widgets/segmented.rs`

These widgets currently hardcode dark-glass surfaces and light text.
They need to accept a light-mode flag or detect it from context so the
same widget renders correctly on both light launcher and dark overlay backgrounds.

**Note:** Only `Button` currently has a builder pattern (`with_style()`).
All other widgets (`TextInput`, `Dropdown`, `Toggle`, `Slider`,
`SegmentedControl`) have NO builder methods — each needs: (1) add a
`color_mode: ColorMode` field defaulting to `Dark`, (2) add a
`with_color_mode(mode: ColorMode) -> Self` builder method, and
(3) branch on `self.color_mode` inside its `paint()` method.

- [ ] **Step 1: Add a `ColorMode` enum to widgets/mod.rs**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    #[default]
    Dark,
    Light,
}
```

- [ ] **Step 2: Add `with_color_mode(ColorMode)` builder to Button**

When `ColorMode::Light`:
- `ButtonStyle::Primary` uses `PRIMARY_BLUE` bg + white text
- `ButtonStyle::Secondary` uses white bg + dark border + dark text
- `ButtonStyle::Destructive` uses `DANGER` bg + white text

- [ ] **Step 3: Add `with_color_mode(ColorMode)` to TextInput**

When `ColorMode::Light`:
- Background: `launcher_control_surface()`
- Text color: `LT_TEXT_PRIMARY`
- Placeholder color: `LT_TEXT_MUTED`

- [ ] **Step 4: Add `with_color_mode(ColorMode)` to Dropdown**

When `ColorMode::Light`:
- Closed surface: `launcher_control_surface()`
- Open menu surface: `[1.0, 1.0, 1.0, 0.95]`
- Text color: `LT_TEXT_PRIMARY`

- [ ] **Step 5: Add `with_color_mode(ColorMode)` to Toggle**

When `ColorMode::Light`:
- Off track: light gray
- On track: `PRIMARY_BLUE`
- Thumb: white

- [ ] **Step 6: Add `with_color_mode(ColorMode)` to Slider**

When `ColorMode::Light`:
- Track: `SLIDER_TRACK_LIGHT` (`rgba(primary, 0.10)` — very light blue)
- Filled portion: `PRIMARY_BLUE` at partial opacity
- Thumb: `SLIDER_THUMB_LIGHT` (`PRIMARY_BLUE`) with `SLIDER_THUMB_BORDER` (white, 2px)
- Thumb shadow: subtle `rgba(0,0,0,0.1)`

- [ ] **Step 7: Add `with_color_mode(ColorMode)` to SegmentedControl**

When `ColorMode::Light`:
- Container: `SEGMENTED_CONTAINER_LIGHT` (`bg-white/50 border-white rounded-xl`)
- Active segment: `SEGMENTED_ACTIVE_LIGHT` (`PRIMARY_BLUE`) + white text + shadow-sm
- Inactive segments: transparent + `LT_TEXT_MUTED` text

- [ ] **Step 8: Run tests and commit**

```bash
cargo test -p prism-client
git add crates/prism-client/src/ui/widgets/
git commit -m "feat(widgets): add ColorMode::Light support for launcher screens"
```

---

## Task 4: Launcher Screen Migration (Home)

**Files:**
- `crates/prism-client/src/ui/launcher/quick_connect.rs`
- `crates/prism-client/src/ui/launcher/shell.rs`

- [ ] **Step 1: QuickConnect — switch to light surfaces and dark text**

Replace:
- `theme::hero_surface(…)` → `theme::launcher_hero_surface(…)`
- `theme::TEXT_PRIMARY` → `theme::LT_TEXT_PRIMARY`
- `theme::TEXT_MUTED` → `theme::LT_TEXT_MUTED`
- Set address_input and connect_button to `ColorMode::Light`

- [ ] **Step 2: Shell header — switch to dark text**

In `paint_header()`:
- `theme::TEXT_PRIMARY` → `theme::LT_TEXT_PRIMARY`
- `theme::TEXT_SECONDARY` → `theme::LT_TEXT_SECONDARY`
- `theme::status_chip(…, ChipTone::Accent)` → `theme::launcher_status_chip(…, ChipTone::Accent)`

- [ ] **Step 3: Shell Home tab — switch Recent Connections section**

In `paint_active_tab()` Home branch:
- `theme::TEXT_SECONDARY` → `LT_TEXT_SECONDARY`
- `theme::separator(…)` → `theme::launcher_separator(…)`
- `theme::modal_surface(…)` / `theme::modal_scrim(…)` — keep as-is (dark modal scrim is intentional)

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p prism-client
git commit -m "feat(home): switch to light-mode surfaces and text"
```

---

## Task 5: Launcher Screen Migration (Sidebar/Nav)

**Files:**
- `crates/prism-client/src/ui/launcher/nav.rs`

- [ ] **Step 1: Switch sidebar to cream surface**

Replace:
- `theme::sidebar_surface(…)` → `theme::launcher_sidebar_surface(…)`
- `theme::nav_item_surface(…)` → `theme::launcher_nav_item_surface(…)`
- Brand text "PRISM" / "Remote client" → `LT_TEXT_PRIMARY` / `LT_TEXT_SECONDARY`
- Nav item text → `LT_TEXT_PRIMARY` (active) / `LT_TEXT_SECONDARY` (inactive)

- [ ] **Step 2: Run tests and commit**

```bash
cargo test -p prism-client
git commit -m "feat(nav): switch sidebar to light cream surface"
```

---

## Task 6: Launcher Screen Migration (Saved Connections)

**Files:**
- `crates/prism-client/src/ui/launcher/card_grid.rs`
- `crates/prism-client/src/ui/launcher/server_card.rs`

- [ ] **Step 1: CardGrid — switch filter chips and empty-state text to light**

Filter chips currently use inline `theme::glass_quad()` with `theme::ACCENT`
colors (not `theme::status_chip()`). Replace these inline constructions:
- Active chip: replace `theme::ACCENT`-based `glass_quad()` with solid
  `PRIMARY_BLUE` bg + white text
- Inactive chips: replace dark `glass_quad()` with `launcher_control_surface()`
  + `LT_TEXT_SECONDARY` text and `border-white/60`
- `theme::TEXT_SECONDARY` → `LT_TEXT_SECONDARY` for chip labels
- `theme::TEXT_MUTED` → `LT_TEXT_MUTED` for empty-state message
- `theme::TEXT_PRIMARY` → `LT_TEXT_PRIMARY` for "Add New Connection" text
- Replace `theme::ACCENT`-colored "+" and text in add-card with `PRIMARY_BLUE`
- Add-card `glass_quad()` → `launcher_card_surface()` with dashed border

- [ ] **Step 2: ServerCard — switch card and row rendering to light**

Card mode:
- `theme::card_surface(…)` → `theme::launcher_card_surface(…)`
- Hover overlay → `theme::launcher_card_hover(…)`
- `theme::status_chip(…)` → `theme::launcher_status_chip(…)`
- Chip text color → `theme::launcher_chip_text_color(…)`
- All text → `LT_TEXT_PRIMARY` / `LT_TEXT_SECONDARY` / `LT_TEXT_MUTED`

Row mode:
- `theme::list_row_surface(…)` → `theme::launcher_row_surface(…)`
- Same text/chip changes as card mode

- [ ] **Step 3: Contextual action buttons per card status**

Map button style to server status:
- **Online**: `ButtonStyle::Primary` + `ColorMode::Light` → solid `PRIMARY_BLUE`
  bg + white text ("Connect")
- **Sleeping**: `ButtonStyle::Secondary` + `ColorMode::Light` → white bg +
  dark border + dark text ("Wake & Connect")
- **Unreachable**: `ButtonStyle::Secondary` + disabled state → `bg-slate-200/50` +
  `LT_TEXT_MUTED` text, non-interactive ("Retry Discovery")

- [ ] **Step 4: Unreachable card visual dimming**

When server status is Unreachable/Offline:
- Card surface alpha reduced to 0.80 (apply `opacity-80` equivalent)
- Title text uses `LT_TEXT_MUTED` instead of `LT_TEXT_PRIMARY`
- Subtitle text uses `LT_TEXT_MUTED` (already muted, but ensure no bold)
- Action button rendered in disabled/neutral style (Step 3 above)
- Edit button remains interactive

- [ ] **Step 5: Run tests and commit**

```bash
cargo test -p prism-client
git commit -m "feat(connections): switch cards and rows to light surfaces"
```

---

## Task 7: Launcher Screen Migration (Profiles)

**File:** `crates/prism-client/src/ui/launcher/profiles.rs`

- [ ] **Step 1: Switch panel surfaces to light**

Both panels currently use `theme::floating_surface()`. Replace:
- `theme::floating_surface(list)` → `theme::launcher_list_surface(list)`
- `theme::floating_surface(editor)` → `theme::launcher_hero_surface(editor)`
- List item selection: `theme::nav_item_surface(*row, selected, false)` →
  `theme::launcher_nav_item_surface(*row, selected, false)`
- Accent-bar `glass_quad()` for selected item → keep but switch color to `PRIMARY_BLUE`
- All `theme::TEXT_PRIMARY` → `theme::LT_TEXT_PRIMARY`
- All `theme::TEXT_SECONDARY` → `theme::LT_TEXT_SECONDARY`
- All `theme::TEXT_MUTED` → `theme::LT_TEXT_MUTED`
- `theme::ACCENT` references → `theme::PRIMARY_BLUE`

- [ ] **Step 2: Editor header sub-surface**

Render the editor header zone with a layered white overlay (`[1.0, 1.0, 1.0, 0.30]`)
and a bottom border using `launcher_inner_separator()`. This creates the depth
visible in the Stitch design where the header sits inside the glass panel:
```
header p-8 bg-white/30 border-b border-white/40
```

- [ ] **Step 3: Active list item styling**

Selected profile item uses:
- `border-l-4 border-primary` (blue left bar, 4px)
- `bg-white/60` background
- Green "Active" dot (1.5px circle in `bg-green-500`) for the active profile
- Font semibold `LT_TEXT_PRIMARY` for name

Inactive items: transparent bg, `LT_TEXT_SECONDARY` text, hover `bg-white/40`.

- [ ] **Step 4: Section headers with tracking**

Section headers ("PERFORMANCE SETTINGS", "DISPLAY & AUDIO", "INPUT & CONNECTIVITY")
use: `text-xs font-bold uppercase tracking-[0.2em] text-slate-500`
- Color: `LT_TEXT_MUTED`
- Style: uppercase, bold, 0.2em letter-spacing (if TextRun supports it)
- Preceded by `launcher_inner_separator()` between sections

- [ ] **Step 5: Bitrate readout styling**

Render the bitrate value as two adjacent text runs:
- Large: `text-2xl font-bold text-primary` → `FONT_DISPLAY` size, bold, `PRIMARY_BLUE`
  color (e.g. "35")
- Small: `text-xs font-medium text-slate-400 uppercase` → small `LT_TEXT_MUTED`
  (e.g. "MBPS")
- Below slider: min/max labels "5 MBPS" / "50 MBPS" in `LT_TEXT_MUTED` at 11px bold

- [ ] **Step 6: Toggle card surfaces (distinct from inputs)**

Toggle items ("Exclusive Input", "Touch Mode", "Auto-Reconnect") use
`launcher_toggle_card_surface(0.60)` — NOT `launcher_control_surface()`.
Each card contains: title (`text-sm font-bold LT_TEXT_PRIMARY`),
description (`text-[11px] LT_TEXT_MUTED`), and toggle (`ColorMode::Light`).
Hover: `bg-white/80` transition.

- [ ] **Step 7: Controls to ColorMode::Light**

- Dropdowns (Display Scaling, Audio Mode, Codec Preference) → `ColorMode::Light`
- Segmented control (Latency vs Quality) → `ColorMode::Light`
- Slider (Bitrate) → `ColorMode::Light`
- Save button → `ButtonStyle::Primary` + `ColorMode::Light`
- Discard button → `ButtonStyle::Secondary` + `ColorMode::Light` (text-only style)

- [ ] **Step 8: Advanced Configuration expandable section**

After Input & Connectivity, add:
- `launcher_separator()` (full-width divider)
- Collapsible header row: uppercase `LT_TEXT_MUTED` text "ADVANCED CONFIGURATION"
  with `tracking-[0.2em]`, right-aligned chevron indicator
- Content area collapses/expands (can be empty initially)

- [ ] **Step 9: Run tests and commit**

```bash
cargo test -p prism-client
git commit -m "feat(profiles): switch to light-mode surfaces"
```

---

## Task 8: Launcher Screen Migration (Settings)

**File:** `crates/prism-client/src/ui/launcher/settings.rs`

- [ ] **Step 1: Wrap all settings content in a white glass container**

The main card currently uses `theme::floating_surface(card_rect)`. Replace:
- `theme::floating_surface(card_rect)` → `theme::launcher_hero_surface(card_rect)`
- Page title → `LT_TEXT_PRIMARY` at `text-3xl font-bold`
- Page subtitle → `LT_TEXT_MUTED`

- [ ] **Step 2: Switch section labels and metadata to light**

- All `theme::TEXT_PRIMARY` → `theme::LT_TEXT_PRIMARY`
- All `theme::TEXT_MUTED` → `theme::LT_TEXT_MUTED`
- `theme::SUCCESS` references — keep for trust chip green dot
- `theme::ACCENT` references → `theme::PRIMARY_BLUE`
- Identity path badge `glass_quad()` → `launcher_control_surface()` with accent tint
- `theme::status_chip(trust_badge, ChipTone::Success)` →
  `theme::launcher_status_chip(trust_badge, ChipTone::Success)`

- [ ] **Step 3: Use inner separators (NOT the current inline glass_quad)**

Settings dividers currently use a `draw_separator` closure that calls
`theme::glass_quad()` inline with dark-tinted colors. Replace the closure body
with `theme::launcher_inner_separator()` (`bg-black/5`), since these dividers
sit INSIDE the white glass panel.

- [ ] **Step 4: Toggle cards at Settings opacity**

Input toggle cards ("Exclusive Keyboard Capture", "Relative Mouse Movement")
currently use inline `theme::glass_quad()` with dark-tinted colors for their
containers. Replace these with `launcher_toggle_card_surface(0.30)` — more
transparent than Profiles (0.60). Match the Stitch Settings pattern:
`bg-white/30 border border-white/40 rounded-lg p-4`.
Each card: title + description text + toggle (`ColorMode::Light`).

- [ ] **Step 5: Audio section uppercase labels**

"REMOTE OUTPUT" / "LOCAL MIC PATH" labels → `LT_TEXT_MUTED` at 10px,
bold uppercase, with `tracking-[0.2em]` letter-spacing (matches Settings code.html
pattern `text-[10px] font-bold text-textMuted uppercase tracking-wider`).

- [ ] **Step 6: All controls to ColorMode::Light**

- Dropdowns (Streaming Defaults, Remote Output, Local Mic Path) → `ColorMode::Light`
- Toggles → `ColorMode::Light`

- [ ] **Step 7: Version footer styling**

Footer text ("PRISM Professional Edition • v0.1.0") →
`LT_TEXT_PRIMARY` at 40% opacity, 10px, bold uppercase, `tracking-[0.2em]`.

- [ ] **Step 8: Run tests and commit**

```bash
cargo test -p prism-client
git commit -m "feat(settings): switch to light-mode surfaces"
```

---

## Task 9: Server Form Modal

**File:** `crates/prism-client/src/ui/launcher/server_form.rs`

- [ ] **Step 1: Switch modal surface and text to light**

- `theme::modal_surface(…)` → `theme::launcher_modal_surface(…)`
- All text → `LT_TEXT_*`
- Text inputs → `ColorMode::Light`
- Buttons → `ColorMode::Light`
- Scrim stays dark (modal_scrim is fine as-is — it's a dimmer)

- [ ] **Step 2: Run tests and commit**

```bash
cargo test -p prism-client
git commit -m "feat(server-form): switch modal to light surfaces"
```

---

## Task 10: Verification Pass

- [ ] **Step 1: Full cargo test**

```bash
cargo test -p prism-client
```

- [ ] **Step 2: Visual verification**

Build and run the client. Verify:
1. Launcher background is light blue gradient (not near-black)
2. Sidebar is cream/light gray (not dark glass)
3. Quick Connect panel is white frosted glass (not dark glass)
4. Text is dark on light (not light on dark)
5. Connect button is corporate blue `#0F6CBD` (not cyan accent)
6. Status chips use pastel backgrounds with dark text (not translucent tints)
7. Cards/rows are white glass (not dark glass)
8. Active filter chip is solid blue with white text (not pastel)
9. Unreachable cards appear dimmed (reduced opacity, muted title)
10. Profile editor has layered header zone (slightly lighter than panel)
11. Bitrate shows large blue number + small "MBPS" label
12. Toggle cards are semi-transparent (not opaque input-style)
13. Section headers use uppercase + tracking (if supported)
14. Segmented control has white container + blue active segment
15. Settings uses subtle inner dividers (not heavy gray separators)
16. Overlay (Ctrl+Ctrl toggle) still uses dark glass capsule + dropdowns
17. All existing CRUD / connect / profile / settings behavior unchanged

- [ ] **Step 3: Final commit**

```bash
git commit -m "feat(launcher): complete light-mode migration for all launcher screens"
```

---

## What This Plan Does NOT Cover

- Icons — Stitch uses Font Awesome + Material Symbols icons throughout; PRISM still
  has no icon primitive. That requires a separate widget addition (icon atlas or
  glyph rendering). Affects: sidebar nav icons, profile type icons, section header
  icons, audio field icons, filter/sort button icons, FAB "+" icon.
- Top-right header bar — Stitch shows PRISM gem icon + "PRISM" branding + user
  avatar circle + minimize/maximize/close in a header bar. This requires new
  layout work in shell.rs.
- Status vocabulary — Stitch uses Online/Sleeping/Unreachable; PRISM uses
  Recent/Dormant/New. This is a data model question, not a theming issue.
- Overlay visual changes — the existing dark-glass capsule+dropdown stays per the
  approved decision. Only launcher screens switch to light mode.
- Card hero images on Connections — Stitch places stock photos in each card header.
  No image loading/rendering widget exists yet.
- Tag/category system for connections — WORK/PERSONAL tags require data model
  additions beyond theming.
- Favorites/hearts on cards — requires data model (favorited boolean) and heart
  icon rendering.
- User profile avatar + name in sidebar footer — requires identity display widget.

These can be addressed in follow-up plans after the light-theme foundation is solid.
