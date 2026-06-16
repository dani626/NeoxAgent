#!/bin/bash
# =============================================================================
# NeoxAgent — Pod/Container Lifecycle Tester
# Can be run locally or targeting a remote host.
#
# Usage:
#   API_KEY=xxx HOST=1.2.3.4 PORT=8443 PROTOCOL=https INSECURE=true bash scripts/test_lifecycle.sh
# =============================================================================
set -euo pipefail

API_KEY="${API_KEY:-change-me-to-a-secure-random-string}"
HOST="${HOST:-127.0.0.1}"
PORT="${PORT:-8443}"
PROTOCOL="${PROTOCOL:-http}"
INSECURE="${INSECURE:-false}"

BASE="${PROTOCOL}://${HOST}:${PORT}"

CURL_FLAGS=(-sf)
if [ "${INSECURE}" = "true" ]; then
  CURL_FLAGS+=(-k)
fi

HEADERS=(-H "Authorization: Bearer ${API_KEY}" -H "Content-Type: application/json")

ok()  { echo -e "\e[32m[OK]\e[0m   $*"; }
err() { echo -e "\e[31m[ERR]\e[0m  $*" >&2; exit 1; }
info(){ echo -e "\e[34m[INFO]\e[0m $*"; }

info "Testing neoxagent lifecycle at $BASE"

# Health
RES=$(curl "${CURL_FLAGS[@]}" "${BASE}/api/health" "${HEADERS[@]}")
ok "Health: $RES"

# Create pod
RES=$(curl "${CURL_FLAGS[@]}" -X POST "${BASE}/api/pods" "${HEADERS[@]}" -d '{
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
curl "${CURL_FLAGS[@]}" -X POST "${BASE}/api/pods/${POD_ID}/stop" "${HEADERS[@]}" > /dev/null
ok "Pod stopped"

# Delete
curl "${CURL_FLAGS[@]}" -X DELETE "${BASE}/api/pods/${POD_ID}?force=true" "${HEADERS[@]}" > /dev/null
ok "Pod deleted"

ok "All lifecycle tests passed!"
