#!/bin/bash
set -e
API_KEY="change-me-to-a-secure-random-string"
API_URL="http://127.0.0.1:8443"
POD_NAME="neox-test-path"

echo "🔍 Verifying neoxhost Paths Configuration..."

# 1. Cleanup
curl -s -X DELETE "$API_URL/api/pods/$POD_NAME?force=true" -H "Authorization: Bearer $API_KEY" > /dev/null 2>&1 || true

# 2. Create Pod WITH VOLUME
# We mount the server's data directory to /data inside container
echo "creating pod..."
CREATE_RES=$(curl -s -X POST "$API_URL/api/pods" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "'"$POD_NAME"'",
    "network": "neox-net",
    "containers": [
        {
            "name": "nginx",
            "image": "docker.io/library/nginx:alpine",
            "ports": [{"host": 8080, "container": 80, "protocol": "tcp"}],
            "volumes": [
                {
                    "host_path": "/var/lib/neoxhost/volumes/'"$POD_NAME"'", 
                    "container_path": "/usr/share/nginx/html"
                }
            ]
        }
    ]
}')

POD_ID=$(echo "$CREATE_RES" | jq -r '.id')
echo "✅ Pod Created: $POD_ID"

# 3. Create a file via API
echo "📝 Writing file via API..."
# This should create /var/lib/neoxhost/volumes/neox-test-path/test.txt
curl -s -X PUR "$API_URL/api/pods/$POD_ID/files/content?path=/test.txt" \
  -H "Authorization: Bearer $API_KEY" \
  -d '{"content": "Hello from neoxhost"}' \
  -X PUT > /dev/null

# 4. Check Physical Path
FILES_LIST=$(curl -s -X GET "$API_URL/api/pods/$POD_ID/files?path=/" -H "Authorization: Bearer $API_KEY")

if echo "$FILES_LIST" | grep -q "test.txt"; then
    echo "✅ Files API is working (found test.txt)"
else
    echo "❌ Files API failed."
    echo "$FILES_LIST"
fi

echo "✨ Path verification complete."
