## Task

Implement **Phase 11 — Cross-Screen Verification** from the launcher UI polish plan.

## Plan Reference

`docs/superpowers/plans/2026-04-03-launcher-ui-polish.md`

Read the plan's Phase 11 section. **All implementation phases (0–10) are already merged.** This phase verifies the final result.

## Tasks (6)

| Task | Summary |
|------|---------|
| TASK-082 | Visual audit — compare each screen against `screen.png` targets |
| TASK-083 | Verify all icon codepoints render (no tofu/missing glyphs) |
| TASK-084 | Verify bold text renders at correct weight |
| TASK-085 | Verify sidebar geometry is edge-to-edge flush |
| TASK-086 | Run full `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` |
| TASK-086a | Capture updated `Implementation.png` screenshots for each screen |

## Constraints

- This is a verification phase — fix any issues found during the audit.
- Update the Progress Tracker: check off `Phase 11` and `Verification`.
- After fixing any issues, run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`.
- Single commit: `chore(client): Phase 11 — cross-screen verification`
- Branch: `launcher-polish/phase-11`
- Do NOT modify files outside `crates/prism-client/` and `Theme/`.

## Labels

`launcher-polish`, `phase-11`, `pass-5`
