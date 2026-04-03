## Task

Implement **Phase 9 — Profiles Editor Polish** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 9 section. Phases 0–8 are already merged. This is the largest phase (14 tasks) — work through them sequentially.

## Tasks (14)

| Task | Summary |
|------|---------|
| TASK-051 | Add scroll support to Profiles (currently missing — Settings has it, Profiles does not) |
| TASK-052 | Profile card layout — name, description, icon |
| TASK-053 | Profile selector — segmented control or card grid |
| TASK-054 | Section headers within profile editor |
| TASK-055 | 2-column grid for dropdowns (extract `layout_helpers.rs`) |
| TASK-056 | Toggle card surfaces for boolean settings |
| TASK-057 | Profile-specific icon rendering |
| TASK-058 | Header search bar for profiles |
| TASK-059 | Profile create/delete actions |
| TASK-060 | Profile duplicate action |
| TASK-061 | Profile rename inline editing |
| TASK-062 | Design spec reconciliation — primary button style (add code comment) |
| TASK-063 | Profile editor validation states |
| TASK-064 | Profile editor save/cancel flow |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 9`. If Phases 4, 7–9 are all done, also check off `Pass 3`.
- Single commit: `feat(client): Phase 9 — profiles editor polish`
- Branch: `launcher-polish/phase-9`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-9`, `pass-3`
