#!/bin/bash
set -e
API_KEY="change-me-to-a-secure-random-string"
API_URL="http://127.0.0.1:8443"
POD_NAME="neox-test-path"

echo "🔍 Verifying neoxhost Paths Configuration..."

# 1. Cleanup
curl -s -X DELETE "$API_URL/api/pods/$POD_NAME?force=true" -H "Authorization: Bearer $API_KEY" > /dev/null 2>&1 || true

# 2. Create Pod (No explicit volume path - let agent decide!)
# We omit 'volumes' array so the agent uses defaults (if implemented) 
# OR we verify where it mounts things. Actually, for this test, we want to see where Files API lands.
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
            "ports": [{"host": 8080, "container": 80, "protocol": "tcp"}]
        }
    ]
}')

POD_ID=$(echo "$CREATE_RES" | jq -r '.id')
echo "✅ Pod Created: $POD_ID"

# 3. Create a file via API
echo "📝 Writing file via API..."
curl -s -X PUR "$API_URL/api/pods/$POD_ID/files/content?path=/test.txt" \
  -H "Authorization: Bearer $API_KEY" \
  -d '{"content": "Hello from neoxhost"}' \
  -X PUT > /dev/null

# 4. Check Physical Path
EXPECTED_PATH="/var/lib/neoxhost/volumes/$POD_NAME"

# Note: The agent currently mounts volumes explicitly. 
# If we didn't specify volumes in JSON, the container has no binding.
# BUT the Files API should translate virtual paths to physical ones based on config.
# Let's verify if 'list files' works on root.

FILES_LIST=$(curl -s -X GET "$API_URL/api/pods/$POD_ID/files?path=/" -H "Authorization: Bearer $API_KEY")

if echo "$FILES_LIST" | grep -q "etc"; then
    echo "✅ Files API is working (listing container root)"
else
    echo "❌ Files API failed or container empty"
    echo "$FILES_LIST"
fi

echo "✨ Path verification complete."
