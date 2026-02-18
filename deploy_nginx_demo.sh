#!/bin/bash
set -e

# Configuration
API_URL="http://127.0.0.1:8443"
API_KEY="change-me-to-a-secure-random-string"
PODMAN_SOCK="/tmp/podman_demo_$USER.sock"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${CYAN}🚀 NeoxAgent: Full Nginx Deployment Demo${NC}"
echo "============================================"

# --- Prerequisites: Start Podman Socket & Agent ---

echo -e "${BLUE}🔌 Starting Infrastructure...${NC}"
# 0. Cleanup residuals from previous runs
echo -e "${BLUE}🧹 Cleaning up previous runs...${NC}"
# CRITICAL: Kill any old NeoxAgent instances on port 8443
pkill -f 'neox.agent' 2>/dev/null || true
pkill -f 'target/debug/neox' 2>/dev/null || true
# Wait for port to free up
sleep 1
fuser -k 8443/tcp 2>/dev/null || true
sleep 1
podman pod rm -f web-server >/dev/null 2>&1 || true
podman network rm -f neox-net >/dev/null 2>&1 || true

# 1. Start Podman Socket
rm -f "$PODMAN_SOCK"
podman system service --time 0 "unix://$PODMAN_SOCK" > /dev/null 2>&1 &
PODMAN_PID=$!
sleep 1

# 2. Update Config
sed -i "s|socket = \".*\"|socket = \"$PODMAN_SOCK\"|" config.toml

# Try to load cargo environment if available
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

# Find Cargo Binary
if [ -f "$HOME/.cargo/bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
elif [ -f "/home/dani626/.cargo/bin/cargo" ]; then
    CARGO_BIN="/home/dani626/.cargo/bin/cargo"
else
    CARGO_BIN="cargo"
fi

echo "Using Cargo: $CARGO_BIN"

# 3. Start Agent
$CARGO_BIN build --quiet 2>/dev/null
$CARGO_BIN run --quiet 2>&1 | grep -v '^warning:' > agent.log 2>&1 &
AGENT_PID=$!

echo -n "⏳ Waiting for API..."
for i in {1..120}; do
    if grep -q "listening on\|Listening on\|NeoxAgent listening" agent.log 2>/dev/null; then
        echo -e "${GREEN} Ready!${NC}"
        break
    fi
    if grep -q "Address already in use" agent.log; then
        echo -e "${RED} FAILED! Port 8443 is still in use.${NC}"
        cat agent.log
        exit 1
    fi
    if ! kill -0 $AGENT_PID 2>/dev/null; then
        echo -e "${RED} FAILED! Agent crashed.${NC}"
        cat agent.log
        exit 1
    fi
    sleep 1
done

# Helper function
api_call() {
    local method=$1
    local endpoint=$2
    local data=$3
    
    # -sS: Silent but show errors
    if [ -z "$data" ]; then
        curl -sS -X "$method" \
            -H "Authorization: Bearer $API_KEY" \
            -H "Content-Type: application/json" \
            "$API_URL$endpoint"
    else
        curl -sS -X "$method" \
            -H "Authorization: Bearer $API_KEY" \
            -H "Content-Type: application/json" \
            -d "$data" \
            "$API_URL$endpoint"
    fi
}

# --- Step 1: Pull Image (Phase 7) ---
echo -e "\n${YELLOW}[Phase 7] Pulling Nginx Image...${NC}"
PULL_RES=$(api_call POST "/api/images/pull" '{"image":"docker.io/library/nginx:alpine"}')
echo "✅ Image Pulled"

# --- Step 2: Create Network (Phase 3) ---
echo -e "\n${YELLOW}[Phase 3] Creating Network 'neox-net'...${NC}"
api_call POST "/api/networks" '{"name":"neox-net","driver":"bridge"}' > /dev/null
echo "✅ Network Created"

# --- Step 3: Create Pod (Phase 3 + 1) ---
echo -e "\n${YELLOW}[Phase 3] Creating Pod 'web-server'...${NC}"
mkdir -p /home/dani626/neox_servers/web-server
chmod 777 /home/dani626/neox_servers/web-server

# Mounting a volume 'www' to /usr/share/nginx/html is key for File Manager
POD_JSON='{
    "name": "web-server",
    "network": "neox-net",
    "containers": [
        {
            "name": "nginx",
            "image": "docker.io/library/nginx:alpine",
            "ports": [{"host": 9090, "container": 80, "protocol": "tcp"}],
            "volumes": [{"host_path": "/home/dani626/neox_servers/web-server", "container_path": "/usr/share/nginx/html"}]
        }
    ]
}'
POD_RES=$(api_call POST "/api/pods" "$POD_JSON")
echo "DEBUG POD RESPONSE: $POD_RES"

if echo "$POD_RES" | jq -e . >/dev/null 2>&1; then
    POD_ID=$(echo "$POD_RES" | jq -r '.id')
    echo -e "✅ Pod Deployed (ID: ${GREEN}$POD_ID${NC}) mapped to port 9090"
    # CRITICAL FIX: Use Pod Name for subsequent calls to match volume directory structure
    POD_ID="web-server"
else
    echo -e "${RED}❌ FAILED TO CREATE POD. Response:${NC}"
    echo "$POD_RES"
    exit 1
fi

# --- Step 4: File Manager (Phase 5) ---
echo -e "\n${YELLOW}[Phase 5] Uploading Custom Website...${NC}"

# Create directory first (optional, but good for testing API)
CMD_RES=$(api_call POST "/api/pods/$POD_ID/files/create-dir?path=/assets")
echo "DEBUG CREATE DIR: $CMD_RES"

# Write index.html to ROOT to fix 403 Forbidden
HTML_CONTENT="<html><body style='background-color:#1a1a1a;color:#00ff00;text-align:center;font-family:monospace;display:flex;flex-direction:column;justify-content:center;height:100vh;'><h1>🚀 NeoxAgent Deployment Success!</h1><p>This file was uploaded via Phase 5 File Manager API.</p><p>Status: <strong>Active</strong></p></body></html>"
# Use a temp file to avoid JSON escaping hell in bash
echo "$HTML_CONTENT" > temp_index.html
# Use jq to construct JSON safely
JSON_PAYLOAD=$(jq -n --arg content "$(cat temp_index.html)" '{content: $content}')

UPLOAD_RES=$(api_call PUT "/api/pods/$POD_ID/files/content?path=/index.html" "$JSON_PAYLOAD")
echo "DEBUG UPLOAD RES: $UPLOAD_RES"
rm temp_index.html
echo "✅ Custom index.html uploaded via API"
chmod -R 755 /home/dani626/neox_servers/web-server
ls -laR /home/dani626/neox_servers/web-server

# --- Step 5: Verification ---
echo -e "\n${YELLOW}[Verification] Checking Website on Port 9090...${NC}"
sleep 2 # Wait for Nginx
HTTP_RESPONSE=$(curl -s http://localhost:9090)
if [[ "$HTTP_RESPONSE" == *"NeoxAgent"* ]]; then
    echo -e "${GREEN}✅ SUCCESS! Server responded with custom content.${NC}"
else
    echo -e "${RED}❌ FAILED. Response: $HTTP_RESPONSE${NC}"
fi

# --- Step 6: Backups (Phase 6) ---
echo -e "\n${YELLOW}[Phase 6] Creating Backup of Website...${NC}"
BACKUP_RES=$(api_call POST "/api/pods/$POD_ID/backups" '{"stop_server":false,"description":"Deployment V1"}')
echo "DEBUG BACKUP: $BACKUP_RES"
BACKUP_ID=$(echo "$BACKUP_RES" | jq -r '.id')
echo -e "✅ Backup Created (ID: ${GREEN}$BACKUP_ID${NC})"

# --- Step 7: Systemd (Phase 7) ---
echo -e "\n${YELLOW}[Phase 7] Generating Systemd Service...${NC}"
SYSTEMD_RES=$(api_call POST "/api/pods/$POD_ID/systemd/generate" '{}')
echo "DEBUG SYSTEMD: $SYSTEMD_RES"
SERVICE_NAME=$(echo "$SYSTEMD_RES" | jq -r '.service_name')
echo -e "✅ Service Generated: ${GREEN}$SERVICE_NAME${NC}"

# cleanup logic
echo -e "\n${GREEN}✨ Demo is live! Open your browser at http://localhost:9090${NC}"
echo -e "${BLUE}🧹 Press [ENTER] to stop the server and cleanup resources...${NC}"
read -r
echo "Removing resources..."
api_call DELETE "/api/pods/$POD_ID?force=true" > /dev/null
api_call DELETE "/api/networks/neox-net" > /dev/null
kill $AGENT_PID 2>/dev/null
kill $PODMAN_PID 2>/dev/null
rm -f "$PODMAN_SOCK"
echo "Done."
