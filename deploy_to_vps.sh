#!/bin/bash
set -e

# Configuration
VPS_USER="root"
VPS_IP="172.82.64.27"
VPS_PORT="51821"
BINARY_PATH="target/release/neoxagent"
REMOTE_BIN="/usr/local/bin/neoxagent"
SERVICE_FILE="/etc/systemd/system/neoxagent.service"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🚀 Starting Deployment to $VPS_IP...${NC}"

# 1. Verify connection
echo -e "${BLUE}📡 Verifying SSH connection...${NC}"
SSH_OPTS="-p 51821 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
ssh $SSH_OPTS -o BatchMode=yes -o ConnectTimeout=5 $VPS_USER@$VPS_IP "echo '✅ Connection OK' && uname -a"

# 2. Upload Binaries & Config
echo -e "${BLUE}Pb Uploading neoxagent binary...${NC}"
scp -P 51821 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null $BINARY_PATH $VPS_USER@$VPS_IP:$REMOTE_BIN
scp -P 51821 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null config.toml $VPS_USER@$VPS_IP:/etc/neoxagent.toml
scp -P 51821 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null deploy_nginx_demo.sh $VPS_USER@$VPS_IP:/root/deploy_nginx_demo.sh

# 3. Setup Remote Environment (Install Podman, Systemd Service)
echo -e "${BLUE}⚙️ Configuring Remote Server (Podman + Systemd)...${NC}"
ssh $SSH_OPTS $VPS_USER@$VPS_IP "/bin/bash -s" << 'EOF'
    set -e
    
    # 3.1 Install Podman if missing
    if ! command -v podman &> /dev/null; then
        echo "📦 Installing Podman..."
        apt-get update -qq
        apt-get install -y -qq podman
    else
        echo "✅ Podman is already installed."
    fi

    # 3.2 Configure Permissions
    chmod +x /usr/local/bin/neoxagent
    chmod +x /root/deploy_nginx_demo.sh
    
    # Configure demo script to use production config path
    sed -i 's|config.toml|/etc/neoxagent.toml|g' /root/deploy_nginx_demo.sh

    # 3.3 Create Systemd Service for neoxagent
    echo "📜 Creating Systemd Service..."
    cat > /etc/systemd/system/neoxagent.service <<SERVICE
[Unit]
Description=neoxagent - High-performance Podman Wrapper
After=network.target podman.socket
Requires=podman.socket

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/neoxagent
WorkingDirectory=/root
Restart=always
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
SERVICE

    # 3.4 Enable and Start Service
    systemctl daemon-reload
    systemctl enable --now podman.socket
    systemctl enable --now neoxagent
    
    echo "✅ neoxagent Service Installed and Started!"
    systemctl status neoxagent --no-pager | head -n 10
EOF

echo -e "${GREEN}✨ Deployment Finished Successfully!${NC}"
echo -e "You can now run the Nginx demo on the VPS: ssh -p $VPS_PORT $VPS_USER@$VPS_IP '/root/deploy_nginx_demo.sh'"
