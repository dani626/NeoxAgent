#!/bin/bash
set -e

# Configuration
BINARY_NAME="neoxagent"
INSTALL_DIR="/usr/local/bin"
CONFIG_SRC="config.toml"
CONFIG_DEST="/etc/neoxagent.toml"
DATA_DIR="/srv/neox"
SERVICE_NAME="neoxagent"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}🚀 neoxagent Installer v0.1${NC}"
echo "============================="

# 1. Check Root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}This script must be run as root.${NC}" 
   exit 1
fi

# 2. Dependencies (Podman, curl, jq)
echo "📦 Installing Dependencies..."
apt-get update -qq
apt-get install -y -qq podman curl jq dbus-user-session

# 3. Setup Directories & User (Optional: can run as specific user, here root for simplicity)
mkdir -p "$DATA_DIR"
mkdir -p /root/neox_servers

# 4. Install Binary
if [ -f "$BINARY_NAME" ]; then
    echo "📦 Installing Binary..."
    install -m 755 "$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
else
    echo -e "${RED}Error: Binary '$BINARY_NAME' not found in current directory.${NC}"
    exit 1
fi

# 5. Config Setup
if [ -f "$CONFIG_SRC" ]; then
    echo "⚙️ Configuring..."
    install -m 644 "$CONFIG_SRC" "$CONFIG_DEST"
    # IMPORTANT: Determine Working Directory
    # The current agent implementation looks for config.toml in CWD
    # We will run the service in /root and symlink config
    ln -sf "$CONFIG_DEST" /root/config.toml
else
    echo -e "${RED}Error: Config '$CONFIG_SRC' not found.${NC}"
    exit 1
fi

# 6. Service Installation (Systemd)
echo "📜 Creating Systemd Service..."
cat > "/etc/systemd/system/$SERVICE_NAME.service" <<EOF
[Unit]
Description=neoxagent - Container Orchestrator
Documentation=https://neox.app/docs
After=network.target podman.socket
Requires=podman.socket

[Service]
Type=simple
User=root
WorkingDirectory=/root
ExecStart=$INSTALL_DIR/$BINARY_NAME
Restart=always
Environment=RUST_LOG=info
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

# 7. Enable & Start
systemctl daemon-reload
systemctl enable --now podman.socket
systemctl enable --now "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"

# 8. Firewall Cleanup (Allow Default Traffic)
echo "🛡️ Configuring Firewall (iptables)..."
# Flush rules and set default policy to ACCEPT (Use with caution in strict envs)
iptables -P INPUT ACCEPT
iptables -P FORWARD ACCEPT
iptables -P OUTPUT ACCEPT
iptables -F
iptables -X

# 9. Verify
sleep 2
if systemctl is-active --quiet "$SERVICE_NAME"; then
    echo -e "${GREEN}✅ Installation Successful!${NC}"
    echo "Service Status: Active (Running)"
    echo "API Port: Check config.toml (Default: 8443)"
else
    echo -e "${RED}❌ Service failed to start.${NC}"
    systemctl status "$SERVICE_NAME" --no-pager
    exit 1
fi

echo -e "\n${GREEN}Usage:${NC}"
echo "To check logs: journalctl -u $SERVICE_NAME -f"
echo "To stop: systemctl stop $SERVICE_NAME"
echo "API Check: curl http://127.0.0.1:8443/api/health"
