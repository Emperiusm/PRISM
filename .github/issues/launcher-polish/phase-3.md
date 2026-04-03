## Task

Implement **Phase 3 — Sidebar Geometry Overhaul** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 3 section. Phases 0–2 are already merged. `GlassQuad` now has a `Default` impl (Phase 0 TASK-P05a), so `..Default::default()` shorthand is available.

## Tasks (7)

| Task | Summary |
|------|---------|
| TASK-008 | Move sidebar origin to `(0, 0)`, set `corner_radius: 0.0` |
| TASK-009 | Extend sidebar height to full window height |
| TASK-010 | Remove sidebar horizontal margin/padding |
| TASK-011 | Adjust content area `x` offset to account for flush sidebar |
| TASK-012a | Restyle active nav item — flush rect + 4px `PRIMARY_BLUE` left bar |
| TASK-012b | Replace sidebar branding with hamburger icon (conditional on Phase 5) |
| TASK-015 | Adjust nav item vertical spacing and padding |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 3`. If Phases 1–3 are all done, also check off `Pass 1`.
- Single commit: `feat(client): Phase 3 — sidebar geometry overhaul`
- Branch: `launcher-polish/phase-3`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-3`, `pass-1`
