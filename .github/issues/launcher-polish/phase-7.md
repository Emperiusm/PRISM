## Task

Implement **Phase 7 — Saved Connections: Filter Bar & Card Grid** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 7 section. Phases 0–6 and Phase 4 are already merged.

## Tasks (11)

| Task | Summary |
|------|---------|
| TASK-034 | Build filter bar container layout |
| TASK-035 | Implement `CardFilter` chip buttons (All, Recent, Dormant, New) |
| TASK-036 | Style active filter chip with `PRIMARY_BLUE` |
| TASK-037 | Add search input to filter bar |
| TASK-038 | Implement card grid layout (responsive columns) |
| TASK-039 | Render `ServerCard` with name, status, OS label |
| TASK-040 | Add status dot/chip to each card |
| TASK-041 | Add last-connected timestamp to cards |
| TASK-042 | Implement card hover state |
| TASK-043 | Add card click → connect action |
| TASK-044 | Implement grid filtering based on active `CardFilter` |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 7`.
- Single commit: `feat(client): Phase 7 — filter bar & card grid`
- Branch: `launcher-polish/phase-7`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-7`, `pass-3`
