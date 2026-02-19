#!/bin/bash
set -e

# Config
API_URL="http://127.0.0.1:8443"
API_KEY="change-me-to-a-secure-random-string"
POD_ID="web-server-80"
CONTAINER_NAME="web-server-80-nginx" # Usually podname-containername

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}🩺 neoxagent: Lifecycle & Features Test${NC}"
echo "========================================="

# Helper function
api_call() {
    local method=$1
    local endpoint=$2
    local data=$3
    
    if [ -z "$data" ]; then
        curl -s -X "$method" \
            -H "Authorization: Bearer $API_KEY" \
            -H "Content-Type: application/json" \
            "$API_URL$endpoint"
    else
        curl -s -X "$method" \
            -H "Authorization: Bearer $API_KEY" \
            -H "Content-Type: application/json" \
            -d "$data" \
            "$API_URL$endpoint"
    fi
}

check_port_80() {
    if ss -tuln | grep ":80 " > /dev/null; then
        return 0 # UP
    else
        return 1 # DOWN
    fi
}

# --- TEST 1: File Reading ---
echo -e "\n${YELLOW}[Test 1] Reading /index.html via API...${NC}"
# Note: Path must be absolute inside container
FILE_RES=$(api_call GET "/api/pods/$POD_ID/files/content?path=/usr/share/nginx/html/index.html")

if echo "$FILE_RES" | grep -q "neoxagent: IT WORKS"; then
    echo -e "${GREEN}✅ PASS: File content retrieved successfully.${NC}"
else
    echo -e "${RED}❌ FAIL: Could not read file content.${NC}"
    echo "Response: $FILE_RES"
fi

# --- TEST 2: Stop Pod ---
echo -e "\n${YELLOW}[Test 2] Stopping Pod '$POD_ID'...${NC}"
STOP_RES=$(api_call POST "/api/pods/$POD_ID/stop")
echo "   Agent Response: $STOP_RES"

echo "   Waiting 3s for shutdown..."
sleep 3

if check_port_80; then
    echo -e "${RED}❌ FAIL: Port 80 is still open! Pod did not stop.${NC}"
else
    echo -e "${GREEN}✅ PASS: Port 80 is closed. Pod stopped.${NC}"
fi

# --- TEST 3: Start Pod ---
echo -e "\n${YELLOW}[Test 3] Starting Pod '$POD_ID'...${NC}"
START_RES=$(api_call POST "/api/pods/$POD_ID/start")
echo "   Agent Response: $START_RES"

echo "   Waiting 3s for startup..."
sleep 3

if check_port_80; then
    echo -e "${GREEN}✅ PASS: Port 80 is open again. Pod started.${NC}"
else
    echo -e "${RED}❌ FAIL: Port 80 is closed. Pod did not start.${NC}"
fi

# --- TEST 4: Generate Systemd Unit ---
echo -e "\n${YELLOW}[Test 4] Generating Systemd Unit...${NC}"
SYSTEMD_RES=$(api_call POST "/api/pods/$POD_ID/systemd/generate")

if echo "$SYSTEMD_RES" | grep -q "Unit file created"; then
    echo -e "${GREEN}✅ PASS: Systemd unit generated.${NC}"
    # Verify file existence on host
    if [ -f "/etc/systemd/system/pod-$POD_ID.service" ]; then
        echo -e "${GREEN}✅ PASS: Service file found at /etc/systemd/system/pod-$POD_ID.service${NC}"
    else
        echo -e "${RED}❌ FAIL: Service file not found on disk.${NC}"
    fi
else
    echo -e "${RED}❌ FAIL: API Error.${NC}"
    echo "Response: $SYSTEMD_RES"
fi

# --- TEST 5: Container Logs (Snapshot) ---
echo -e "\n${YELLOW}[Test 5] Fetching Container Logs (Snapshot)...${NC}"
# Currently we don't have a snapshot log endpoint, usually it's websocket.
# But we can try to inspect the container via API to ensure it's "running"
CONTAINER_INFO=$(api_call GET "/api/containers")
if echo "$CONTAINER_INFO" | grep -q "nginx"; then
     echo -e "${GREEN}✅ PASS: Container list returned nginx instance.${NC}"
else
     echo -e "${RED}❌ FAIL: Container not found in list.${NC}"
fi

echo -e "\n========================================="
echo -e "${GREEN}🎉 All Tests Completed!${NC}"
