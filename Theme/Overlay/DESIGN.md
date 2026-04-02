# Design System Specification: The Ethereal Professional

## 1. Overview & Creative North Star
The Creative North Star for this design system is **"The Digital Architect."** 

We are moving away from the "flatness" of typical enterprise software to create a sense of depth, precision, and high-end utility. For a premium remote desktop client, the UI must feel like a sophisticated cockpit—one that is both powerful and weightless. By leveraging Windows 11 Fluent principles and the Mica material effect, we create an editorial layout that prioritizes content (the remote stream) while nesting controls in a hierarchy of translucent, intelligent layers. 

We break the "template" look through **intentional asymmetry**: sidebars are not just vertical strips, but floating panels; primary actions are not just buttons, but anchored focal points with tonal gradients.

---

## 2. Colors & Materiality
The palette is rooted in deep slates and luminous blues, designed to recede and let the remote session take center stage.

### The "No-Line" Rule
**Explicit Instruction:** Do not use 1px solid borders for sectioning. Structural boundaries must be defined solely through background color shifts or tonal transitions. To separate a sidebar from a main view, use `surface-container-low` against a `surface` background.

### Surface Hierarchy & Nesting
Treat the UI as a physical stack of glass sheets.
- **Base Layer:** `surface` (#131313) or `surface-dim`.
- **Primary Containers:** `surface-container` (#20201f) for main content areas.
- **Floating Controls:** `surface-container-highest` (#353535) for modals and pop-overs.
- **The Mica Effect:** All primary app backgrounds must utilize a backdrop-blur (minimum 30px) combined with a 70% opacity version of `surface`.

### The "Glass & Gradient" Rule
Standard buttons are prohibited for primary CTAs. Instead, use a subtle radial gradient transitioning from `primary` (#a6c8ff) to `primary_container` (#0067c0). This "Signature Texture" provides a jewel-like quality to main actions, such as "Connect."

---

## 3. Typography: Segoe UI Variable
Our typography is the backbone of the "Editorial" feel. We use the **Segoe UI Variable** axis to maximize legibility at small sizes and character at large sizes.

- **Display (The Statement):** Use `display-md` (2.75rem) with a `semibold` weight for dashboard greetings or empty states. This scale conveys authority.
- **Headline (The Navigation):** `headline-sm` (1.5rem) should be used for section titles. It creates a clear visual anchor.
- **Body (The Workhorse):** `body-md` (0.875rem) is the default for all metadata.
- **Labels (The Precision):** `label-sm` (0.6875rem) in `uppercase` with 0.05em tracking is used for technical specs (IP addresses, latency metrics).

---

## 4. Elevation & Depth
In this system, depth is a functional tool, not a stylistic flourish.

### The Layering Principle
Achieve hierarchy through **Tonal Layering**. 
- To highlight a specific remote machine in a list, do not add a border. Instead, shift its background from `surface-container-low` to `surface-container-high`.

### Ambient Shadows
When an element must float (e.g., a floating connection toolbar), use a "Signature Shadow":
- **Blur:** 32px
- **Spread:** -4px
- **Color:** `on-surface` (#e5e2e1) at 6% opacity. 
This mimics natural light dispersion rather than a harsh "drop shadow."

### The "Ghost Border" Fallback
If accessibility requires a container edge, use a **Ghost Border**: `outline-variant` (#414752) at 15% opacity. It should be felt, not seen.

---

## 5. Components

### Buttons & Connection States
- **Primary Action:** Rounded `md` (0.75rem). Uses the Blue Gradient.
- **Connection Status Chips:**
    - **Online:** `tertiary` (#4ae183) text on a 10% opacity `tertiary_container` background.
    - **Sleeping:** Amber/Orange (Custom Hue) with a soft pulsing glow effect (blur 8px).
    - **Unreachable:** `error` (#ffb4ab) with high-contrast `on_error_container` text.
- **Forbid Dividers:** Do not use lines to separate list items. Use 12px of vertical white space (Spacing Scale `3`) or subtle hover transitions using `surface-bright`.

### Remote Stream Frame
The viewport for the remote desktop should have a `lg` (1rem) corner radius. To create an "Editorial" look, the frame should have a 2px "Ghost Border" to separate the remote OS from the local client UI.

### Navigation Sidebar
A floating "Mica" panel. Use `surface-container-lowest` (#0e0e0e) at 80% opacity with a 40px backdrop blur. This ensures the sidebar feels like a premium utility tool rather than an afterthought.

---

## 6. Do’s and Don’ts

### Do:
- **Do** use `xl` (1.5rem) rounding for large containers to emphasize the "Soft Minimalism" of the system.
- **Do** use asymmetric layouts. For example, right-align technical metadata while left-aligning primary labels to create a sophisticated visual rhythm.
- **Do** utilize the `surface_tint` (#a6c8ff) at very low opacities (2-3%) over dark backgrounds to give the slate grays a "premium blue" undertone.

### Don’t:
- **Don’t** use 100% black (#000000). Always use `surface-container-lowest` to maintain tonal depth.
- **Don’t** use standard Windows 10 sharp corners. Everything must adhere to the `8px - 12px` rule to maintain the "Fluent" soul.
- **Don’t** clutter the screen. If a piece of information isn't vital to the current connection state, hide it behind a `surface-variant` hover state.