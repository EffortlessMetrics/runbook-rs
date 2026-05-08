# runbookd

[![Coverage](https://github.com/EffortlessMetrics/runbook-rs/actions/workflows/coverage.yml/badge.svg)](https://github.com/EffortlessMetrics/runbook-rs/actions/workflows/coverage.yml)
[![Codecov](https://codecov.io/gh/EffortlessMetrics/runbook-rs/branch/main/graph/badge.svg)](https://codecov.io/gh/EffortlessMetrics/runbook-rs)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Runbook is a **physical control surface for the Claude Code agent loop**.

This repository contains the **Rust daemon** (`runbookd`) and the Claude Code **hook consumer** (`runbook-hooks`). The daemon is the *brain*: it owns state (`ARMED`, agent lifecycle state from hooks), and it bridges between:

- **Claude Code hooks** → `runbook-hooks` → `runbookd`
- **Logitech Options+ / Logi Actions plugin** (C#) ↔ `runbookd`
- **VS Code extension** (TypeScript) ↔ `runbookd`

## Why a daemon

The Logitech plugin and the VS Code extension are constrained runtimes.

The daemon gives us:

- A single source of truth for the operator loop
- Stable, versioned protocol between components
- A place to normalize Claude Code hook telemetry into a small, truthful UI model

## Components

- `crates/runbook-protocol` — versioned JSON protocol types (shared)
- `crates/runbookd` — the daemon server
- `crates/runbook-hooks` — hook consumer CLI invoked by Claude Code
- `claude-plugin/` — sample Claude Code plugin bundle (commands + hooks)

## Quick start (dev)

### 1) Start the daemon

```bash
# from repo root
runbookd --config ./runbook.yaml
```

### 2) Install the Claude Code plugin (dev)

Claude Code supports loading plugins from a local directory.

```bash
claude --plugin-dir ./claude-plugin
```

### 3) Point hooks at `runbook-hooks`

The plugin's `hooks/hooks.json` calls `runbook-hooks ...`.

Make sure the `runbook-hooks` binary is on your `PATH` (or adjust the command in `hooks.json` to a full path).

### 4) Simulate hooks (optional)

Use the Rust simulator to exercise the daemon without running Claude Code:

```bash
cargo run -p runbookd --example simulate_hooks
```

Set `DAEMON_BASE_URL` or pass `-- --daemon <URL>` when the daemon is not listening on `http://127.0.0.1:29381`.

### 5) Connect clients

- Install the VS Code extension (see `runbook-vscode` repo)
- Install the Logi Actions plugin (see `runbook-actions-plugin` repo)

## Config

`runbook.yaml` is the repo-tuned keypad layout.

- `keypad.pages[*].slots` **must** be exactly 9 entries (3×3 keypad)
- `command` is what gets sent to Claude Code when dispatched (typically a slash command)

See the sample `runbook.yaml` in repo root.

## Protocol

The daemon speaks JSON over WebSocket (for interactive clients) and accepts hook events over HTTP:

- `GET /ws` — WebSocket (Logi + VS Code clients)
- `POST /hook` — hook events from `runbook-hooks`

Protocol types are in `crates/runbook-protocol`.

## Status mapping

`runbookd` derives a coarse operator-facing state from Claude Code hooks:

- `Notification/idle_prompt` → `IDLE`
- `Notification/permission_prompt` → `WAITING`
- `UserPromptSubmit` → `RUNNING`
- `TaskCompleted` → `COMPLETE`
- `Stop` → `SETTLED`
- `SessionEnd` → `ENDED`

The point: **truthful state**, not inference.

## Coverage

Codecov is execution-surface telemetry only; see [Coverage](docs/ci/coverage.md) for what the badge does and does not claim.

## License

MIT OR Apache-2.0
