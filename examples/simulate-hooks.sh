#!/usr/bin/env bash
set -euo pipefail

DAEMON_BASE_URL="${DAEMON_BASE_URL:-http://127.0.0.1:29381}"

post() {
  local hook="$1"
  local matcher="${2:-}"
  local session_id="${3:-sess-demo-001}"
  local payload="${4:-{}}"

  local body
  if [[ -n "$matcher" ]]; then
    body="{\"hook\":\"$hook\",\"matcher\":\"$matcher\",\"session_id\":\"$session_id\",\"payload\":$payload}"
  else
    body="{\"hook\":\"$hook\",\"matcher\":null,\"session_id\":\"$session_id\",\"payload\":$payload}"
  fi

  echo "→ $hook${matcher:+/$matcher}"
  curl -sS -X POST "$DAEMON_BASE_URL/hook" \
    -H 'Content-Type: application/json' \
    -d "$body" >/dev/null
}

echo "Simulating Claude Code lifecycle events…"
echo ""

echo "--- Session start ---"
post "SessionStart" "" "sess-demo-001" '{}'
sleep 0.5

echo "--- Idle (waiting for prompt) ---"
post "Notification" "idle_prompt" "sess-demo-001" '{}'
sleep 1

echo "--- User submits prompt ---"
post "UserPromptSubmit" "" "sess-demo-001" '{"prompt":"/runbook:prep-pr"}'
sleep 1

echo "--- Permission prompt (agent needs approval) ---"
post "Notification" "permission_prompt" "sess-demo-001" '{"reason":"needs file write permission"}'
sleep 1.5

echo "--- Back to running ---"
post "Notification" "idle_prompt" "sess-demo-001" '{}'
sleep 0.5
post "UserPromptSubmit" "" "sess-demo-001" '{"prompt":"continue"}'
sleep 1

echo "--- Task completed ---"
post "TaskCompleted" "" "sess-demo-001" '{"result":"ok"}'
sleep 0.5

echo "--- Stop ---"
post "Stop" "" "sess-demo-001" '{}'
sleep 0.5

echo "--- Session end ---"
post "SessionEnd" "" "sess-demo-001" '{}'

echo ""
echo "✓ Simulation complete"
