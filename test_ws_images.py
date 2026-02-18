import asyncio
import websockets
import json
import sys

# Configuration
WS_URL = "ws://127.0.0.1:8443/api/images/pull/stream"

async def test_image_pull_stream():
    print(f"🔌 Connecting to {WS_URL}...")
    try:
        async with websockets.connect(WS_URL) as websocket:
            print("✅ Connected!")

            # 1. Send Pull Request
            image_name = "docker.io/library/busybox:latest"
            request = {"image": image_name}
            print(f"📤 Sending pull request: {request}")
            await websocket.send(json.dumps(request))

            # 2. Receive Messages
            print("📥 Waiting for progress updates...")
            while True:
                try:
                    message = await websocket.recv()
                    data = json.loads(message)
                    
                    msg_type = data.get("type", "unknown")
                    
                    if msg_type == "start":
                        print(f"   [START] {data.get('status')}")
                    elif msg_type == "progress":
                        stream = data.get("stream", "").strip()
                        if stream:
                            print(f"   [PROGRESS] {stream}")
                    elif msg_type == "error":
                        print(f"❌ [ERROR] {data.get('error')}")
                        break
                    elif msg_type == "complete":
                        print(f"✅ [COMPLETE] {data.get('status')}")
                        break
                    else:
                        print(f"   [UNKNOWN] {data}")
                        
                except websockets.exceptions.ConnectionClosed:
                    print("🔌 Connection closed by server.")
                    break
                except Exception as e:
                    print(f"❌ Error receiving message: {e}")
                    break

    except Exception as e:
        print(f"❌ Failed to connect: {e}")

if __name__ == "__main__":
    try:
        asyncio.run(test_image_pull_stream())
    except KeyboardInterrupt:
        pass
