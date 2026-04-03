## Task

Implement **Phase 5 — Icon Rendering Primitive** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 5 section. Phases 0–3 are already merged. **Note:** Phase 4 is deferred to Pass 3; Phase 5 comes next per execution order (Pass 2).

## Tasks (9)

| Task | Summary |
|------|---------|
| TASK-021 | ~~Acquire icon font~~ Already done — `MaterialSymbolsOutlined.ttf` is in `assets/fonts/` |
| TASK-021a | Icon font subset audit — verify all 28+ codepoints are present |
| TASK-022 | Load icon font in `TextRenderer` as a secondary `FontSystem` family |
| TASK-023 | Create `Icon` widget struct with `new()`, `with_size()`, `with_color()` |
| TASK-024 | Implement `Icon::paint()` — emit a `TextRun { icon: true, ... }` |
| TASK-025 | Define icon codepoint constants (`ICON_HOME`, `ICON_SETTINGS`, etc.) |
| TASK-025a | Add `text_width_exact()` helper in `text_renderer.rs` |
| TASK-025b | Add `Dropdown.with_trailing_icon()` builder in `dropdown.rs` |

## Constraints

- TASK-021a must pass before TASK-022 (font must have all codepoints before loading).
- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 5`.
- Single commit: `feat(client): Phase 5 — icon rendering primitive`
- Branch: `launcher-polish/phase-5`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-5`, `pass-2`
