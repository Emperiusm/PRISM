## Task

Implement **Phase 0 — Data-Layer Prerequisites** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the **full plan** first to understand the overall architecture, then focus on:
- Section 10 (Recommended Execution Order) — this is **Pass 0**
- Phase 0 task table (TASK-P01 through TASK-P06)
- Phase 0 implementation details (code blocks for each task)

## Tasks (6)

| Task | Summary |
|------|---------|
| TASK-P01 | Add `os_label`, `tags`, `wol_supported`, `last_latency_ms` to `SavedServer` |
| TASK-P02 | Add `ServerStatus` enum + `derived_status()` heuristic |
| TASK-P03 | Add `CardStatus` display mapping |
| TASK-P04 | Add `CardFilter::Tag(String)` variant |
| TASK-P05 | Add `bold`/`icon` fields to `TextRun`, manual `Default` impl, migrate construction sites |
| TASK-P05a | Implement `Default` for `GlassQuad` |
| TASK-P06 | Add `SavedServer::display_name()` helper |

## Constraints

- Follow the implementation details in the plan **precisely** — they contain verified code.
- After each task, run `cargo check -p prism-client` to verify compilation.
- After completing all tasks, run `cargo test --workspace` for a full check.
- Update the Progress Tracker in the plan: check off `Phase 0` and `Pass 0`.
- All changes go in a **single commit** with message: `feat(client): Phase 0 — data-layer prerequisites`
- Do NOT modify any files outside `crates/prism-client/`.
- Branch name: `launcher-polish/phase-0`

## Labels

`launcher-polish`, `phase-0`, `pass-0`
