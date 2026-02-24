# Runbook protocol v1

This is the stable boundary between the three repos:

- **runbook-actions-plugin** (C#)
- **runbookd** (Rust)
- **runbook-vscode** (TypeScript)

## Versioning

- `protocol`: integer, bumped only on breaking changes.
- Additive changes (new message variants, optional fields) are allowed without bumping.
- All JSON uses **snake_case** field names (enforced by Rust serde, must be respected by C#/TS).

## Transport

- Interactive clients connect via WebSocket: `ws://127.0.0.1:29381/ws`
- Claude Code hooks are forwarded over HTTP: `POST http://127.0.0.1:29381/hook`

## Handshake

On WebSocket connect, daemon sends:

```json
{ "type": "hello", "protocol": 1, "daemon_version": "0.1.0" }
```

Client should reply with:

```json
{ "type": "hello", "client": "logi", "protocol": 1, "version": "0.1.0", "capabilities": ["keypad"] }
```

## Message catalog

### Client → daemon

| Type                   | Purpose              | Key fields                          |
|------------------------|----------------------|-------------------------------------|
| `hello`                | Identify client      | `client`, `protocol`, `version`, `capabilities` |
| `keypad_press`         | Arm a prompt         | `prompt_id`                         |
| `dialpad_button_press` | Button event         | `button` (ctrl_c/export/esc/enter)  |
| `adjustment`           | Dial/roller delta    | `kind` (dial/roller), `delta`       |
| `page_nav`             | Page prev/next       | `direction` (prev/next)             |
| `hook_event`           | Hook lifecycle event | `hook`, `matcher`, `session_id`, `payload` |

### Daemon → client

| Type             | Purpose          | Key fields                                    |
|------------------|------------------|-----------------------------------------------|
| `hello`          | Ack + version    | `protocol`, `daemon_version`                  |
| `render`         | UI model         | `agent_state`, `armed`, `keypad`, `page_index`, `page_count`, `hooks_connected` |
| `vscode_command` | Editor command   | `kind`, `target`, `payload`                   |
| `notice`         | Debug/info toast | `message`                                     |

### Hook event → daemon (HTTP)

POST body is a `hook_event` object:

```json
{
  "hook": "UserPromptSubmit",
  "matcher": null,
  "session_id": "abc123",
  "payload": { "prompt": "..." }
}
```

## Agent states

| State                | Source                              | Meaning                        |
|----------------------|-------------------------------------|--------------------------------|
| `unknown`            | Default / no hooks                  | Cannot determine agent state   |
| `idle`               | `Notification/idle_prompt`          | Ready for next prompt          |
| `running`            | `UserPromptSubmit` / `PreToolUse`   | Agent is working               |
| `waiting_permission` | `Notification/permission_prompt`    | Blocked on human permission    |
| `waiting_input`      | `Notification/elicitation_dialog`   | Blocked on clarification       |
| `complete`           | `TaskCompleted`                     | Task finished                  |
| `settled`            | `Stop`                              | Agent stopped                  |
| `ended`              | `SessionEnd`                        | Session terminated             |
| `blocked`            | PreToolUse deny                     | Tool call blocked by policy    |

See Rust types in `crates/runbook-protocol`.
