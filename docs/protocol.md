# Runbook protocol v1

This is the stable boundary between the three repos:

- **runbook-actions-plugin** (C#)
- **runbookd** (Rust)
- **runbook-vscode** (TypeScript)

## Versioning

- `protocol`: integer, bumped only on breaking changes.
- Additive changes (new message variants, optional fields) are allowed without bumping.

## Transport

- Interactive clients connect via WebSocket: `ws://127.0.0.1:29381/ws`
- Claude Code hooks are forwarded over HTTP: `POST http://127.0.0.1:29381/hook`

## Message envelope

All WebSocket messages are JSON with a `type` tag.

### Client → daemon

- `hello`
- `keypad_press`
- `dialpad_button_press`
- `adjustment`

### Daemon → client

- `hello`
- `render`
- `vscode_command`
- `notice`

See Rust types in `crates/runbook-protocol`.
