#!/bin/bash
set -e
API_KEY="change-me-to-a-secure-random-string"
API_URL="http://127.0.0.1:8443"

echo "🚀 Deploying Nginx on Port 80..."

# 1. Delete previous pods if exist
curl -s -X DELETE "$API_URL/api/pods/web-server-vps?force=true" -H "Authorization: Bearer $API_KEY" > /dev/null 2>&1 || true
curl -s -X DELETE "$API_URL/api/pods/web-server-vps-80?force=true" -H "Authorization: Bearer $API_KEY" > /dev/null 2>&1 || true
sleep 2

# 2. Deploy on Port 80
# Note: We reuse the same volume path so the index.html is still there!
curl -sS -X POST "$API_URL/api/pods" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "web-server-80",
    "network": "neox-net",
    "containers": [
        {
            "name": "nginx",
            "image": "docker.io/library/nginx:alpine",
            "ports": [{"host": 80, "container": 80, "protocol": "tcp"}],
            "volumes": [{"host_path": "/root/neox_servers/web-server", "container_path": "/usr/share/nginx/html"}]
        }
    ]
}' | jq .

echo -e "\n\n✅ Pod Created on Port 80. Checking connectivity..."

# 3. Check if listening
if ss -tuln | grep ":80 " > /dev/null; then
    echo "✅ Server is LISTENING on Port 80"
else
    echo "❌ Server is NOT listening on Port 80. Something went wrong."
fi

# 4. Opening Firewall just in case (Port 80)
echo "🛡️ Ensuring Port 80 is open in IPTables..."
iptables -I INPUT -p tcp --dport 80 -j ACCEPT
