#!/bin/sh
# install-neox-guard.sh
#
# Bootstraps the neox-guard host-level IP leak protection on the VPS.
# Run once after deploying neoxagent.
#
# What this does:
#   1. Calls POST /api/guard/install on the local agent
#   2. Verifies the FORWARD DROP rule is active
#   3. Prints next steps
#
# Usage:
#   chmod +x scripts/install-neox-guard.sh
#   API_KEY=your-key ./scripts/install-neox-guard.sh
#   # or:
#   API_KEY=your-key PORT=8080 ./scripts/install-neox-guard.sh

set -e

API_KEY="${API_KEY:-}"
PORT="${PORT:-3000}"
BASE_URL="http://127.0.0.1:${PORT}"

if [ -z "$API_KEY" ]; then
  echo "[neox-guard] ERROR: API_KEY env var is required"
  echo "  Usage: API_KEY=your-key ./scripts/install-neox-guard.sh"
  exit 1
fi

echo "[neox-guard] Installing host-level guard via neoxagent..."

RESPONSE=$(curl -sf -X POST "${BASE_URL}/api/guard/install" \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json")

echo "[neox-guard] Response: $RESPONSE"

echo ""
echo "[neox-guard] Checking iptables FORWARD rule..."
if iptables -C FORWARD -m comment --comment "neox-guard-forward-drop" -j DROP 2>/dev/null; then
  echo "[neox-guard] ✅ DROP rule is ACTIVE in FORWARD chain."
else
  echo "[neox-guard] ⚠️  DROP rule not found. Check journalctl -u neox-guard.service"
  exit 1
fi

echo ""
echo "[neox-guard] ✅ Done! Host-level guard is active."
echo ""
echo "  Next steps:"
echo "  - Deploy a pod with proxy enabled via POST /api/pods"
echo "  - The sidecar will automatically call /api/guard/lift once hev is healthy"
echo "  - Or lift manually: curl -X POST ${BASE_URL}/api/guard/lift -H 'Authorization: Bearer <key>'"
echo ""
echo "  Status: curl ${BASE_URL}/api/guard/status -H 'Authorization: Bearer <key>'"
