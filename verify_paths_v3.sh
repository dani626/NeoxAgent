#!/bin/bash
set -e
API_KEY="change-me-to-a-secure-random-string"
API_URL="http://127.0.0.1:8443"
POD_NAME="neox-test-path"
VOLUME_PATH="/var/lib/neoxhost/volumes/$POD_NAME"

echo "🔍 Verifying neoxhost Paths Configuration..."

# 1. Cleanup
curl -s -X DELETE "$API_URL/api/pods/$POD_NAME?force=true" -H "Authorization: Bearer $API_KEY" > /dev/null 2>&1 || true

# 2. Prepare Volume Directory explicitly
echo "📂 Creating volume directory at $VOLUME_PATH..."
mkdir -p "$VOLUME_PATH"

# 3. Create Pod
echo "Creating pod..."
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
                    "host_path": "'"$VOLUME_PATH"'",
                    "container_path": "/usr/share/nginx/html"
                }
            ]
        }
    ]
}')

# Check for error
if echo "$CREATE_RES" | grep -q "\"error\":true"; then
    echo "❌ Pod creation failed:"
    echo "$CREATE_RES"
    exit 1
fi

POD_ID=$(echo "$CREATE_RES" | jq -r '.id')
echo "✅ Pod Created: $POD_ID"

# 4. Write file via API
echo "📝 Writing file via API..."
curl -s -X PUT "$API_URL/api/pods/$POD_ID/files/content?path=/test.txt" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"content": "Hello from neoxhost"}' 

# 5. Verify file exists on Disk
if [ -f "$VOLUME_PATH/test.txt" ]; then
    echo "✅ SUCCESS: File found on physical disk: $VOLUME_PATH/test.txt"
    cat "$VOLUME_PATH/test.txt"
else
    echo "❌ FAIL: File not found on disk."
fi

echo "✨ Path verification complete."
