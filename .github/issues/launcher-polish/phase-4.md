## Task

Implement **Phase 4 — Home Screen: Recent Connections** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 4 section. Phases 0–3, 5–6 are already merged. The `Icon` widget and bold text are available.

**Key context:** The Home tab currently delegates Recent Connections rendering to `card_grid` in `Rows` mode. TASK-014 refactors this — build a dedicated lightweight row list directly in `quick_connect.rs` and remove the delegation.

## Tasks (4)

| Task | Summary |
|------|---------|
| TASK-014 | Refactor Home→CardGrid delegation — build standalone Recent Connections list |
| TASK-016 | Render each recent connection as a row with name, status chip, timestamp |
| TASK-017 | Add "Reconnect" button per row |
| TASK-019 | Style empty state when no recent connections exist |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 4`.
- Single commit: `feat(client): Phase 4 — home screen recent connections`
- Branch: `launcher-polish/phase-4`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-4`, `pass-3`
