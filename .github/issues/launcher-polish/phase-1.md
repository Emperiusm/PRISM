## Task

Implement **Phase 1 — Bold Text Support** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 1 section, including the task table and implementation details. Phase 0 has already been completed — `TextRun` now has `bold: bool` and `icon: bool` fields with a manual `Default` impl.

## Tasks (3)

| Task | Summary |
|------|---------|
| TASK-002 | Thread `bold` flag through `TextRenderer::prepare()` — set `Weight::BOLD` when `run.bold` is true |
| TASK-003 | Set `bold: true` on hero title `TextRun` in `quick_connect.rs` |
| TASK-004 | Set `bold: true` on section headers, card titles, nav labels |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 1`.
- Single commit: `feat(client): Phase 1 — bold text support`
- Branch: `launcher-polish/phase-1`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-1`, `pass-1`
