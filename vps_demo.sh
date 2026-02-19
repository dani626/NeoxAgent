#!/bin/bash
set -e

# Configuration
API_URL="http://127.0.0.1:8443"
API_KEY="change-me-to-a-secure-random-string"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}🚀 neoxagent: VPS Deployment Demo${NC}"
echo "============================================"

# Ensure jq is installed
if ! command -v jq &> /dev/null; then
    echo -e "${YELLOW}📦 Installing jq...${NC}"
    apt-get update -qq && apt-get install -y -qq jq
fi

# Helper function
api_call() {
    local method=$1
    local endpoint=$2
    local data=$3
    
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

echo -e "\n${YELLOW}📡 Checking API Status...${NC}"
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$API_URL/api/health" || echo "ERR")

if [ "$STATUS" != "200" ]; then
    echo -e "${RED}❌ neoxagent API is not reachable (Status: $STATUS).${NC}"
    echo "Check service status with: systemctl status neoxagent"
    exit 1
fi
echo "✅ API is Online!"

# --- Step 1: Pull Image ---
echo -e "\n${YELLOW}[Phase 7] Pulling Nginx Image...${NC}"
api_call POST "/api/images/pull" '{"image":"docker.io/library/nginx:alpine"}' > /dev/null
echo "✅ Image Pulled"

# --- Step 2: Create Network ---
echo -e "\n${YELLOW}[Phase 3] Creating Network 'neox-net'...${NC}"
# Ignore error if network exists
api_call POST "/api/networks" '{"name":"neox-net","driver":"bridge"}' > /dev/null 2>&1 || true
echo "✅ Network Configured"

# --- Step 3: Create Pod ---
echo -e "\n${YELLOW}[Phase 3] Creating Pod 'web-server'...${NC}"
POD_JSON='{
    "name": "web-server-vps",
    "network": "neox-net",
    "containers": [
        {
            "name": "nginx",
            "image": "docker.io/library/nginx:alpine",
            "ports": [{"host": 9090, "container": 80, "protocol": "tcp"}],
            "volumes": [{"host_path": "/root/neox_servers/web-server", "container_path": "/usr/share/nginx/html"}]
        }
    ]
}'
# Delete old pod if exists
api_call DELETE "/api/pods/web-server-vps?force=true" > /dev/null 2>&1 || true
sleep 2

POD_RES=$(api_call POST "/api/pods" "$POD_JSON")
if echo "$POD_RES" | jq -e .id >/dev/null; then
    POD_ID=$(echo "$POD_RES" | jq -r '.id')
    echo -e "✅ Pod Deployed (ID: ${GREEN}$POD_ID${NC})"
else
    echo -e "${RED}❌ Failed to create pod.${NC}"
    echo "$POD_RES"
    exit 1
fi

# Use pod name for file ops
POD_ID="web-server-vps"

# --- Step 4: File Manager ---
echo -e "\n${YELLOW}[Phase 5] Uploading Website...${NC}"

# Create assets dir
api_call POST "/api/pods/$POD_ID/files/create-dir?path=/assets" > /dev/null 2>&1 || true

# HTML Content
HTML_CONTENT="<html><body style='background-color:#0d1117;color:#c9d1d9;font-family:sans-serif;text-align:center;display:flex;flex-direction:column;justify-content:center;height:100vh;'>
<h1 style='color:#58a6ff;'>🚀 neoxagent on VPS</h1>
<p>Deployed successfully to <strong>172.82.64.27</strong></p>
<div style='border:1px solid #30363d;padding:20px;border-radius:6px;max-width:400px;margin:20px auto;background:#161b22;'>
<p>Status: <span style='color:#3fb950;font-weight:bold;'>Operational</span></p>
<p>Managed by: neoxagent v0.1.0</p>
</div>
</body></html>"

# Prepare JSON
JSON_PAYLOAD=$(jq -n --arg content "$HTML_CONTENT" '{content: $content}')

# Upload to root (fixes 403)
api_call PUT "/api/pods/$POD_ID/files/content?path=/index.html" "$JSON_PAYLOAD" > /dev/null
echo "✅ Custom index.html uploaded"

# --- Summary ---
echo -e "\n${GREEN}✨ SUCCESS! Website is live at:${NC}"
echo -e "${BLUE}👉 http://172.82.64.27:9090${NC}"
