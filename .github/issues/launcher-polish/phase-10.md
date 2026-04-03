## Task

Implement **Phase 10 — Settings Panel Polish** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 10 section. Phases 0–9 are already merged. All primitives (icons, bold text, dropdowns, layout helpers) are available.

## Tasks (16)

| Task | Summary |
|------|---------|
| TASK-066 | Settings sub-nav (sidebar or tab-based section navigation) |
| TASK-067 | Settings section headers with icons |
| TASK-068 | Breadcrumb header for Settings |
| TASK-069 | Dropdown width constraints |
| TASK-070 | Display settings — 2-column dropdown grid with icons |
| TASK-071 | Toggle card surfaces for boolean settings |
| TASK-072 | Slider control for numeric settings |
| TASK-073 | Audio section — input/output dropdowns with `ICON_SPEAKER`/`ICON_MIC` |
| TASK-074 | Audio test buttons |
| TASK-075 | Keyboard shortcut display |
| TASK-076 | Security section layout |
| TASK-077 | About/version info section |
| TASK-078 | Sidebar footer — user avatar circle |
| TASK-079 | Avatar initials rendering |
| TASK-080 | Settings save/apply flow |
| TASK-081 | Settings reset to defaults |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 10` and `Pass 4`.
- Single commit: `feat(client): Phase 10 — settings panel polish`
- Branch: `launcher-polish/phase-10`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-10`, `pass-4`
