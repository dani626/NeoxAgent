#!/bin/bash
# Moved from root — test pod/container lifecycle against a running neoxagent.
# Usage: API_KEY=xxx PORT=8443 bash scripts/test_lifecycle.sh

set -euo pipefail

API_KEY="${API_KEY:-change-me-to-a-secure-random-string}"
PORT="${PORT:-8443}"
BASE="http://127.0.0.1:${PORT}"
HEADERS=(-H "Authorization: Bearer ${API_KEY}" -H "Content-Type: application/json")

ok()  { echo -e "\e[32m[OK]\e[0m   $*"; }
err() { echo -e "\e[31m[ERR]\e[0m  $*" >&2; exit 1; }
info(){ echo -e "\e[34m[INFO]\e[0m $*"; }

info "Testing neoxagent at $BASE"

# Health
RES=$(curl -sf "${BASE}/api/health" "${HEADERS[@]}")
ok "Health: $RES"

# Create pod
RES=$(curl -sf -X POST "${BASE}/api/pods" "${HEADERS[@]}" -d '{
  "name": "test-lifecycle-pod",
  "containers": [{
    "name": "test-ctr",
    "image": "alpine:latest",
    "command": ["sleep", "3600"]
  }]
}')
POD_ID=$(echo "$RES" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
ok "Pod created: $POD_ID"

# Stop
curl -sf -X POST "${BASE}/api/pods/${POD_ID}/stop" "${HEADERS[@]}" > /dev/null
ok "Pod stopped"

# Delete
curl -sf -X DELETE "${BASE}/api/pods/${POD_ID}?force=true" "${HEADERS[@]}" > /dev/null
ok "Pod deleted"

ok "All lifecycle tests passed!"
