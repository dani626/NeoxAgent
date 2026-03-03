#!/bin/bash
set -e

# ==============================================================================
# NeoxAgent & System Requirements Auto-Installer
# Compatible with: Debian 11/12, Ubuntu 20.04/22.04/24.04
# ==============================================================================

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}=================================================${NC}"
echo -e "${GREEN}  NeoxAgent VPS Setup & Dependency Installer  ${NC}"
echo -e "${BLUE}=================================================${NC}"

# 1. Check for root privileges
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}❌ Error: This script must be run as root (or using sudo).${NC}" 
   exit 1
fi

echo -e "\n${YELLOW}▶ Step 1: Updating system packages...${NC}"
apt-get update -qq
# apt-get upgrade -y -qq # Opcional: descomentar si quieres forzar actualización del sistema completo

echo -e "\n${YELLOW}▶ Step 2: Installing core dependencies (Podman, network tools, DBus)...${NC}"
apt-get install -y -qq podman curl jq dbus-user-session iptables iproute2 wget unzip tar xz-utils sudo

# 2. Configurar reenvío de IP (IPv4 forwarding) necesario para Podman y Proxies
echo -e "\n${YELLOW}▶ Step 3: Enabling IPv4 Forwarding...${NC}"
sysctl -w net.ipv4.ip_forward=1 > /dev/null
if ! grep -q "^net.ipv4.ip_forward=1" /etc/sysctl.conf; then
    echo "net.ipv4.ip_forward=1" >> /etc/sysctl.conf
fi
sysctl -p > /dev/null

# 3. Descargar e instalar ipt2socks
echo -e "\n${YELLOW}▶ Step 4: Installing ipt2socks (for container proxying)...${NC}"
IPT2SOCKS_VERSION=$(curl -s "https://api.github.com/repos/zfl9/ipt2socks/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$IPT2SOCKS_VERSION" ]; then
    IPT2SOCKS_VERSION="v1.1.3" # Fallback version si falla la API de GitHub
fi

IPT2SOCKS_URL="https://github.com/zfl9/ipt2socks/releases/download/${IPT2SOCKS_VERSION}/ipt2socks-linux-amd64.zip"

echo "Downloading ipt2socks ${IPT2SOCKS_VERSION}..."
wget -qO /tmp/ipt2socks.zip "$IPT2SOCKS_URL"
unzip -qo /tmp/ipt2socks.zip -d /tmp/
mv /tmp/ipt2socks /usr/local/bin/ipt2socks
chmod +x /usr/local/bin/ipt2socks
rm -f /tmp/ipt2socks.zip
echo -e "${GREEN}✔ ipt2socks installed successfully!${NC}"

# 4. Crear estructura de directorios de NeoxAgent
echo -e "\n${YELLOW}▶ Step 5: Creating NeoxAgent directories...${NC}"
mkdir -p /srv/neox/{data,volumes,configs,backups}
mkdir -p /etc/neoxagent
chmod -R 755 /srv/neox
echo -e "${GREEN}✔ Directories created inside /srv/neox${NC}"

# 5. Configurar Podman Socket y persistencia
echo -e "\n${YELLOW}▶ Step 6: Enabling and starting Podman Socket...${NC}"
systemctl enable --now podman.socket
systemctl enable --now podman.service

# Validar que podman responde
if podman info > /dev/null 2>&1; then
    echo -e "${GREEN}✔ Podman is running correctly!${NC}"
else
    echo -e "${RED}❌ Warning: Podman might not be running properly. Check 'systemctl status podman'.${NC}"
fi

# 6. Preparar firewall genérico (Aceptar reenvío)
echo -e "\n${YELLOW}▶ Step 7: Configuring generic Firewall Rules...${NC}"
iptables -P FORWARD ACCEPT || true

# 7. Finalizando y dando instrucciones para el binario
echo -e "\n${BLUE}=================================================${NC}"
echo -e "${GREEN}✅ VPS Dependencies Setup Complete!${NC}"
echo -e "${BLUE}=================================================${NC}"
echo -e "You are now ready to install the actual NeoxAgent binary."
echo -e "To complete the installation of the agent:"
echo -e "  1. Upload 'neoxagent' binary to /usr/local/bin/"
echo -e "  2. Upload 'config.toml' to /etc/neoxagent/config.toml"
echo -e "  3. Create the systemd service (neoxagent.service)"
echo -e "     and run: systemctl enable --now neoxagent"
echo -e "\nIf you have the binary ready, do you want to start it now? (Make sure you upload it!)"
