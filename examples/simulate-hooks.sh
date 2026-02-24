#!/usr/bin/env bash
set -euo pipefail

DAEMON_BASE_URL="${DAEMON_BASE_URL:-http://127.0.0.1:29381}"

post() {
  local hook="$1"
  local matcher="${2:-}"
  local payload="${3:-{}}"

  if [[ -n "$matcher" ]]; then
    curl -sS -X POST "$DAEMON_BASE_URL/hook" \
      -H 'Content-Type: application/json' \
      -d "{\"hook\":\"$hook\",\"matcher\":\"$matcher\",\"payload\":$payload}" >/dev/null
  else
    curl -sS -X POST "$DAEMON_BASE_URL/hook" \
      -H 'Content-Type: application/json' \
      -d "{\"hook\":\"$hook\",\"matcher\":null,\"payload\":$payload}" >/dev/null
  fi
}

echo "Simulating Claude Code lifecycle events…"

post "Notification" "idle_prompt" '{}'
sleep 0.3
post "UserPromptSubmit" "" '{"user_prompt":"/runbook:prep-pr"}'
sleep 0.3
post "Notification" "permission_prompt" '{"reason":"needs approval"}'
sleep 0.3
post "Notification" "idle_prompt" '{}'
sleep 0.3
post "TaskCompleted" "" '{"result":"ok"}'
sleep 0.3
post "Stop" "" '{}'

echo "done"
