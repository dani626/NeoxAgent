#!/bin/bash
set -e
API_URL="http://127.0.0.1:8443"
API_KEY="change-me-to-a-secure-random-string"

echo "🔧 Fixing permissions and re-deploying..."

# 1. Setup Public Directory
mkdir -p /srv/neox_web
echo '<html><body style="background:#111;color:#0f0;display:flex;justify-content:center;align-items:center;height:100vh;font-family:monospace;"><h1>🚀 neoxagent: IT WORKS!</h1></body></html>' > /srv/neox_web/index.html
chmod -R 755 /srv/neox_web
chown -R root:root /srv/neox_web # Ensure root owns it, readable by others

# 2. Cleanup Old Pod
curl -s -X DELETE "$API_URL/api/pods/web-server-80?force=true" -H "Authorization: Bearer $API_KEY" > /dev/null 2>&1 || true

# 3. Create Pod
# Using /srv/neox_web which is definitely readable
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
            "volumes": [{"host_path": "/srv/neox_web", "container_path": "/usr/share/nginx/html"}]
        }
    ]
}' | jq .

echo "✅ Done! Try accessing http://172.82.64.27"
