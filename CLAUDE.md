# CLAUDE.md — PRISM Project Instructions

## Project Overview

PRISM (Protocol for Remote Interactive Streaming & Multiplexing) is a remote desktop application built in Rust. It consists of a server (`prism-server`) that captures and streams the desktop, and a client (`prism-client`) with a glassmorphism launcher UI.

## Build & Test

```bash
cargo build --release -p prism-server -p prism-client   # Build both binaries
cargo test --workspace                                    # Run all tests (~755)
cargo fmt --all -- --check                                # Check formatting
cargo clippy --workspace -- -D warnings                   # Lint (zero warnings required)
```

**All three checks (test, fmt, clippy) must pass before committing.**

## Commit Style

- Use conventional commits: `feat:`, `fix:`, `docs:`, `ci:`, `chore:`
- Scope with crate name when relevant: `feat(server):`, `fix(client):`
- Keep commit messages concise — one sentence describing the change

## Creating Releases

**IMPORTANT: Follow these steps exactly. Do not skip or improvise.**

### Step 1: Determine the next version

Check the latest existing tag:
```bash
git tag --sort=-creatordate | head -1
```

Increment appropriately:
- **Patch** (x.y.Z) — bug fixes, dependency updates, CI fixes
- **Minor** (x.Y.0) — new features, UI changes, protocol additions
- **Major** (X.0.0) — breaking protocol changes (not used yet)

### Step 2: Verify everything passes

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

**Do NOT proceed if any of these fail.**

### Step 3: Tag and push

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

The GitHub Actions release workflow (`.github/workflows/release.yml`) automatically:
1. Builds release binaries with static CRT (`-C target-feature=+crt-static`)
2. Embeds admin manifest in `prism-server.exe` via `mt.exe`
3. Zips `prism-server.exe` + `prism-client.exe` + README + LICENSE
4. Creates a GitHub Release with auto-generated changelog

### Step 4: Verify the release

```bash
gh run list --workflow Release --limit 1    # Check build status
gh release view vX.Y.Z                      # Verify release exists
```

### What NOT to do

- **Do NOT** create releases manually via `gh release create` — the workflow handles it
- **Do NOT** skip version numbers (if latest is v0.4.6, next is v0.4.7 or v0.5.0)
- **Do NOT** tag without running tests first
- **Do NOT** include CI/CLA/workflow changes in the release changelog title
- **Do NOT** modify the release workflow unless explicitly asked

## Architecture

### Workspace Crates

| Crate | Purpose |
|-------|---------|
| `prism-protocol` | Wire format, headers, channels, capabilities |
| `prism-metrics` | Lock-free AtomicHistogram, counters |
| `prism-security` | Identity, pairing, Noise IK, audit |
| `prism-transport` | QUIC connections, quality measurement |
| `prism-observability` | Frame tracing, feedback, overlay |
| `prism-session` | Channels, routing, tombstones, arbiter |
| `prism-display` | Pipeline types, frame classification |
| `prism-platform-windows` | DDA capture, NVENC, D3D11 |
| `prism-server` | Server binary — capture, encode, serve |
| `prism-client` | Client binary — decode, render, UI |
| `prism-tests` | Integration tests |

### Client Architecture

- **Renderer** (`renderer/`) — wgpu-based: `PrismRenderer`, `StreamTexture`, `BlurPipeline`, `UiRenderer`, `TextPipeline`
- **UI** (`ui/`) — glassmorphism widgets, launcher (cards, quick-connect), overlay (stats bar, panels)
- **Input** (`input/`) — double-tap detector, event coalescing, drag handling
- **Config** (`config/`) — CLI parsing, saved servers (append-only log)
- **SessionBridge** — typed channels between UI thread and async network tasks

### Server Architecture

- **ServerApp** — accept loop, connection handling, frame broadcasting
- **Auto-firewall** — creates Windows Firewall rule on startup (requires admin)
- **IP display** — prints local network addresses for easy client connection

## Key Files

| File | What it does |
|------|-------------|
| `crates/prism-client/src/app.rs` | Main winit event loop, render pipeline, UI state machine |
| `crates/prism-client/src/renderer/mod.rs` | wgpu device, surface, stream pipeline |
| `crates/prism-client/src/renderer/ui_renderer.rs` | Glass quad + glow rect + text rendering |
| `crates/prism-server/src/server_app.rs` | Server entry point, accept loop, firewall, IP detection |
| `.github/workflows/release.yml` | Release build pipeline (static CRT, admin manifest) |
| `.github/workflows/ci.yml` | CI checks (fmt, clippy, test) |

## Platform

- Windows 10+ only (server requires DXGI Desktop Duplication)
- Rust edition 2024 (1.85+)
- wgpu 24, winit 0.30, glyphon 0.8, quinn 0.11
