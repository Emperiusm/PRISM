## Task

Implement **Phase 2 — Primary Button Color Fix** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 2 section. Phases 0–1 are already merged.

## Tasks (3)

| Task | Summary |
|------|---------|
| TASK-005 | Audit `Button::paint()` — verify `ColorMode::Light` + `Primary` uses `PRIMARY_BLUE` |
| TASK-006 | Verify secondary/ghost button styles match design |
| TASK-007 | Center-align button label text using `text_width()` |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 2`.
- Single commit: `feat(client): Phase 2 — primary button color fix`
- Branch: `launcher-polish/phase-2`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-2`, `pass-1`
