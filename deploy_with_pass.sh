#!/bin/bash
set -e

# Configuration
VPS_IP="172.82.64.27"
VPS_USER="root"
VPS_PORT="51821"
# Export password for sshpass (handles special chars properly)
export SSHPASS='Greedisgood&1'

# Commands with sshpass
SSH_CMD="sshpass -e ssh -p $VPS_PORT -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
SCP_CMD="sshpass -e scp -P $VPS_PORT -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"

# Paths
BINARY_LOCAL="target/release/neoxagent"
BINARY_REMOTE="/usr/local/bin/neoxagent"
CONFIG_REMOTE="/etc/neoxagent.toml"

echo -e "\033[0;34m🚀 Starting Deployment to $VPS_IP using sshpass...\033[0m"

# 1. Upload Files
echo -e "\033[0;34m📦 Uploading binary...\033[0m"
$SCP_CMD $BINARY_LOCAL $VPS_USER@$VPS_IP:$BINARY_REMOTE

echo -e "\033[0;34m📄 Uploading config & scripts...\033[0m"
$SCP_CMD config.toml $VPS_USER@$VPS_IP:$CONFIG_REMOTE
$SCP_CMD deploy_nginx_demo.sh $VPS_USER@$VPS_IP:/root/deploy_nginx_demo.sh

# 2. Remote Configuration
echo -e "\033[0;34m⚙️ Configuring remote server...\033[0m"
$SSH_CMD $VPS_USER@$VPS_IP "bash -s" << 'EOF'
    set -e
    
    # Install Podman if missing
    if ! command -v podman &> /dev/null; then
        echo "📦 Installing Podman..."
        apt-get update -qq
        apt-get install -y -qq podman
    fi

    # Permissions
    chmod +x /usr/local/bin/neoxagent
    chmod +x /root/deploy_nginx_demo.sh
    
    # Point demo script to correct config file location
    sed -i 's|config.toml|/etc/neoxagent.toml|g' /root/deploy_nginx_demo.sh
    
    # Remove press enter wait from demo script for automation
    sed -i 's/read -r//g' /root/deploy_nginx_demo.sh

    # Create Service
    echo "📜 Creating Systemd Service..."
    cat > /etc/systemd/system/neoxagent.service <<SERVICE
[Unit]
Description=neoxagent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/neoxagent
WorkingDirectory=/root
Restart=always
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
SERVICE

    # Restart Service
    systemctl daemon-reload
    systemctl enable --now neoxagent
    systemctl restart neoxagent
    sleep 2
    
    echo "✅ neoxagent Service Running!"
    systemctl status neoxagent --no-pager | head -n 5
EOF

# 3. Validation
echo -e "\033[0;32m✨ Deployment Complete! Running remote demo...\033[0m"
$SSH_CMD $VPS_USER@$VPS_IP "/root/deploy_nginx_demo.sh"
