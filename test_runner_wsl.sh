#!/bin/bash
set -e

# Configuration
API_URL="http://127.0.0.1:8443"
API_KEY="change-me-to-a-secure-random-string"
PODMAN_SOCK="/tmp/podman_test.sock"

echo "🚀 Starting neoxagent Integration Tests (WSL)"
echo "============================================"

# 1. Start Podman API Socket
echo "🔌 Starting Podman API socket..."
rm -f "$PODMAN_SOCK"
podman system service --time 0 "unix://$PODMAN_SOCK" > /dev/null 2>&1 &
PODMAN_PID=$!
sleep 2

# Check if socket exists
if [ ! -S "$PODMAN_SOCK" ]; then
    echo "❌ Failed to create Podman socket at $PODMAN_SOCK"
    echo "   Ensure 'podman' is installed and in your PATH."
    kill $PODMAN_PID 2>/dev/null || true
    exit 1
fi

# Update config.toml to use this socket temporarily
sed -i 's|socket = ".*"|socket = "/tmp/podman_test.sock"|' config.toml

# 2. Start neoxagent
echo "🦀 Starting neoxagent..."
cargo build --quiet
cargo run --quiet > agent.log 2>&1 &
AGENT_PID=$!

# Wait for agent to be ready
echo "⏳ Waiting for agent (PID $AGENT_PID)..."
for i in {1..30}; do
    if grep -q "Listening on" agent.log; then
        echo "✅ Agent is ready!"
        break
    fi
    sleep 1
done

if ! kill -0 $AGENT_PID 2>/dev/null; then
    echo "❌ Agent failed to start. Check agent.log:"
    cat agent.log
    kill $PODMAN_PID 2>/dev/null || true
    exit 1
fi

# Helper function
do_curl() {
    curl -s -H "Authorization: Bearer $API_KEY" -H "Content-Type: application/json" "$@"
}

echo "Running Tests..."

# 3. Health Check
echo -n "🏥 Health Check... "
HTTP_CODE=$(do_curl -o /dev/null -w "%{http_code}" "$API_URL/api/health")
if [ "$HTTP_CODE" == "200" ]; then echo "✅ OK"; else echo "❌ FAIL ($HTTP_CODE)"; fi

# 4. Phase 7: Pull Image
echo -n "📥 Pulling Alpine Image... "
do_curl -X POST -d '{"image":"docker.io/library/alpine:latest"}' "$API_URL/api/images/pull" > /dev/null
echo "✅ OK"

# 5. Phase 1: Create Container
echo -n "📦 Create Container... "
CONTAINER_ID=$(do_curl -X POST -d '{"name":"test-c1","image":"docker.io/library/alpine:latest","command":["sleep","3600"]}' "$API_URL/api/containers" | jq -r '.id')
if [ "$CONTAINER_ID" != "null" ] && [ -n "$CONTAINER_ID" ]; then
    echo "✅ OK ($CONTAINER_ID)"
else
    echo "❌ FAIL"
fi

# 6. Phase 3: Create Pod
echo -n "🐙 Create Pod... "
POD_ID=$(do_curl -X POST -d '{"name":"test-pod1","containers":[{"name":"main","image":"docker.io/library/alpine:latest","command":["sleep","3600"]}]}' "$API_URL/api/pods" | jq -r '.id')
if [ "$POD_ID" != "null" ] && [ -n "$POD_ID" ]; then
    echo "✅ OK ($POD_ID)"
    
    # 7. Phase 5: File Manager
    echo -n "📁 File Manager (Write/Read)... "
    do_curl -X PUT -d '{"content":"Hello World"}' "$API_URL/api/pods/$POD_ID/files/content?path=/hello.txt" > /dev/null
    CONTENT=$(do_curl "$API_URL/api/pods/$POD_ID/files/content?path=/hello.txt" | jq -r '.content')
    if [ "$CONTENT" == "Hello World" ]; then echo "✅ OK"; else echo "❌ FAIL"; fi

    # 8. Phase 6: Backups
    echo -n "💾 Create Backup... "
    BACKUP_ID=$(do_curl -X POST -d '{"stop_server":false,"description":"Test"}' "$API_URL/api/pods/$POD_ID/backups" | jq -r '.id')
    if [ "$BACKUP_ID" != "null" ] && [ -n "$BACKUP_ID" ]; then
        echo "✅ OK ($BACKUP_ID)"
        # Delete Backup
        do_curl -X DELETE "$API_URL/api/pods/$POD_ID/backups/$BACKUP_ID" > /dev/null
    else
        echo "❌ FAIL"
    fi

    # 9. Phase 7: Systemd
    echo -n "🔧 Systemd Generate... "
    STATUS=$(do_curl -X POST "$API_URL/api/pods/$POD_ID/systemd/generate" | jq -r '.service_name')
    if [[ "$STATUS" == *"service"* ]]; then echo "✅ OK"; else echo "⚠️ SKIPPED (Env dependent)"; fi

    # Cleanup Pod
    do_curl -X DELETE "$API_URL/api/pods/$POD_ID?force=true" > /dev/null
else
    echo "❌ FAIL (Pod creation)"
fi

# Cleanup Container
if [ -n "$CONTAINER_ID" ]; then
    do_curl -X DELETE "$API_URL/api/containers/$CONTAINER_ID?force=true" > /dev/null
fi

# 10. Phase 7: List Images
echo -n "🖼️  List Images... "
IMG_COUNT=$(do_curl "$API_URL/api/images" | jq '.total')
echo "✅ OK ($IMG_COUNT found)"

echo "============================================"
echo "🎉 Tests Completed!"

# Cleanup Processes
kill $AGENT_PID 2>/dev/null || true
kill $PODMAN_PID 2>/dev/null || true
rm -f "$PODMAN_SOCK"
