## Task

Implement **Phase 6 — Sidebar Nav Icons & Header Bar** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 6 section. Phases 0–3 and 5 are already merged. The `Icon` widget from Phase 5 is now available.

## Tasks (8)

| Task | Summary |
|------|---------|
| TASK-026 | Add `Icon` to each sidebar nav item (Home, Connections, Profiles, Settings) |
| TASK-027 | Style active nav icon with `PRIMARY_BLUE` tint |
| TASK-028 | Finalize hamburger → `Icon::new(ICON_MENU)` (cleanup TODO from Phase 3) |
| TASK-029 | Build header bar layout — page title + right-side controls |
| TASK-030 | Add PRISM logo/avatar to header bar right side |
| TASK-031 | Style header bar separator line |
| TASK-032 | Sidebar branding — conditional PRISM text on Settings tab |
| TASK-033 | Light-mode audit of `server_form.rs` |

## Constraints

- After each task, run `cargo check -p prism-client`.
- After all tasks, run `cargo test --workspace`.
- Update the Progress Tracker: check off `Phase 6`. If Phases 5–6 are both done, also check off `Pass 2`.
- Single commit: `feat(client): Phase 6 — sidebar nav icons & header bar`
- Branch: `launcher-polish/phase-6`
- Do NOT modify files outside `crates/prism-client/`.

## Labels

`launcher-polish`, `phase-6`, `pass-2`
