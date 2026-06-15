#!/bin/bash
# Moved from root — runs neoxagent tests from WSL environment.
# Usage: bash scripts/test_runner_wsl.sh

set -euo pipefail

API_KEY="${API_KEY:-change-me-to-a-secure-random-string}"
PORT="${PORT:-8443}"
BASE="http://127.0.0.1:${PORT}"

echo "[WSL Test Runner] Targeting $BASE"
curl -sf "${BASE}/api/health" -H "Authorization: Bearer ${API_KEY}" | jq .
echo "[WSL Test Runner] Done"
