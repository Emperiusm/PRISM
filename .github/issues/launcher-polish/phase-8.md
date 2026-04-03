## Task

Implement **Phase 8 — Saved Connections: FAB & Card Polish** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 8 section. Phases 0–7 and Phase 4 are already merged.

## Tasks (6)

| Task | Summary |
|------|---------|
| TASK-045 | Add dashed "Add Server" card at end of grid |
| TASK-046 | Implement FAB (Floating Action Button) with `+` icon |
| TASK-047 | Position FAB at bottom-right of content area |
| TASK-048 | FAB click → open server form |
| TASK-049 | Card context menu (kebab `⋮` icon) |
| TASK-050 | Card polish — border, shadow, corner radius per design tokens |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 8`.
- Single commit: `feat(client): Phase 8 — FAB & card polish`
- Branch: `launcher-polish/phase-8`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-8`, `pass-3`
