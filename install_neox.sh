#!/bin/bash
set -e

# ═══════════════════════════════════════════════════════════════════
#  neoxagent All-in-One Installer v0.4
#  Installs Rust, compiles NeoxAgent (with native TLS/SSL),
#  installs ipt2socks, configures ZRAM, systemd, kernel, firewall.
#
#  Usage:
#    git clone https://github.com/dani626/NeoxAgent.git
#    cd NeoxAgent && sudo ./install_neox.sh
#
#  With SSL (Let's Encrypt):
#    sudo NEOX_DOMAIN=nodo1.tudominio.com ./install_neox.sh
#
#  Custom repo:
#    sudo NEOX_REPO=https://github.com/user/repo.git ./install_neox.sh
# ═══════════════════════════════════════════════════════════════════

# ─── Configuration ──────────────────────────────────────────────────
REPO_URL="${NEOX_REPO:-https://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@github.com/dani626/NeoxAgent.git}"
BRANCH="${NEOX_BRANCH:-main}"
DOMAIN="${NEOX_DOMAIN:-}"           # If set, enables SSL with Let's Encrypt
EMAIL="${NEOX_EMAIL:-}"             # Email for Let's Encrypt (optional)
BUILD_DIR="/tmp/neoxagent_build"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/neoxagent"
CONFIG_DEST="$CONFIG_DIR/config.toml"
CERTS_DIR="/etc/neoxagent/certs"
DATA_DIR="/srv/neox"
SERVICE_NAME="neoxagent"
IPT2SOCKS_DIR="/tmp/ipt2socks_build"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ─── Helper Functions ──────────────────────────────────────────────
print_header() {
    echo -e "${CYAN}"
    echo "  ███╗   ██╗███████╗ ██████╗ ██╗  ██╗"
    echo "  ████╗  ██║██╔════╝██╔═══██╗╚██╗██╔╝"
    echo "  ██╔██╗ ██║█████╗  ██║   ██║ ╚███╔╝ "
    echo "  ██║╚██╗██║██╔══╝  ██║   ██║ ██╔██╗ "
    echo "  ██║ ╚████║███████╗╚██████╔╝██╔╝ ██╗"
    echo "  ╚═╝  ╚═══╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝"
    echo -e "${NC}"
    echo -e "${GREEN}🚀 neoxagent All-in-One Installer v0.4${NC}"
    echo "═══════════════════════════════════════════"
    if [ -n "$DOMAIN" ]; then
        echo -e "  ${CYAN}SSL Mode:${NC} Let's Encrypt → $DOMAIN"
    else
        echo -e "  ${CYAN}SSL Mode:${NC} Disabled (HTTP)"
    fi
    echo ""
}

log_step()  { echo -e "  ${GREEN}[✓]${NC} $1"; }
log_warn()  { echo -e "  ${YELLOW}[!]${NC} $1"; }
log_error() { echo -e "  ${RED}[✗]${NC} $1"; }
log_info()  { echo -e "  ${CYAN}[→]${NC} $1"; }
log_section() { echo -e "\n${BOLD}── $1 ──${NC}"; }

get_total_ram_mb() {
    awk '/MemTotal/ {printf "%d", $2/1024}' /proc/meminfo
}

print_header

# ═══════════════════════════════════════════════════════════════════
#  STEP 1: Pre-flight Checks
# ═══════════════════════════════════════════════════════════════════
log_section "Pre-flight Checks"

if [[ $EUID -ne 0 ]]; then
    log_error "This script must be run as root."
    echo "       Run: sudo ./install_neox.sh"
    exit 1
fi
log_step "Running as root"

ARCH=$(uname -m)
if [[ "$ARCH" != "x86_64" && "$ARCH" != "aarch64" ]]; then
    log_error "Unsupported architecture: $ARCH (need x86_64 or aarch64)"
    exit 1
fi
log_step "Architecture: $ARCH"

if [ -f /etc/os-release ]; then
    . /etc/os-release
    log_step "OS: $PRETTY_NAME"
else
    log_error "Cannot detect OS."
    exit 1
fi

TOTAL_RAM=$(get_total_ram_mb)
log_step "RAM: ${TOTAL_RAM}MB"

if command -v apt-get &>/dev/null; then
    PKG_MANAGER="apt"
elif command -v dnf &>/dev/null; then
    PKG_MANAGER="dnf"
elif command -v yum &>/dev/null; then
    PKG_MANAGER="yum"
else
    log_error "No supported package manager found."
    exit 1
fi
log_step "Package manager: $PKG_MANAGER"

# ═══════════════════════════════════════════════════════════════════
#  STEP 2: ZRAM (Compressed RAM — critical for 1GB nodes)
# ═══════════════════════════════════════════════════════════════════
log_section "ZRAM (Memory Optimization)"

if [ "$TOTAL_RAM" -le 2048 ]; then
    log_info "Low RAM detected (${TOTAL_RAM}MB). Configuring ZRAM..."

    # Load zram module
    modprobe zram num_devices=1 2>/dev/null || true

    if [ -e /sys/block/zram0 ]; then
        # Check if already in use and disable it before reset
        if grep -q "/dev/zram0" /proc/swaps; then
            swapoff /dev/zram0 2>/dev/null || true
        fi
        
        # Reset if already configured
        echo 1 > /sys/block/zram0/reset 2>/dev/null || true

        # Set ZRAM size to 50% of RAM (compressed, effective ~2x)
        ZRAM_SIZE=$((TOTAL_RAM / 2))M
        echo lz4 > /sys/block/zram0/comp_algorithm 2>/dev/null || \
            echo lzo > /sys/block/zram0/comp_algorithm 2>/dev/null || true
        echo "${ZRAM_SIZE}" > /sys/block/zram0/disksize

        mkswap /dev/zram0 >/dev/null 2>&1
        swapon -p 100 /dev/zram0 2>/dev/null

        log_step "ZRAM enabled: ${ZRAM_SIZE} compressed swap (priority 100)"

        # Make ZRAM persistent across reboots
        cat > /etc/systemd/system/zram-swap.service <<'ZRAM_SERVICE'
[Unit]
Description=ZRAM Compressed Swap
After=local-fs.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/bin/bash -c 'modprobe zram num_devices=1 && echo lz4 > /sys/block/zram0/comp_algorithm 2>/dev/null || echo lzo > /sys/block/zram0/comp_algorithm && echo ZRAM_SIZE_PLACEHOLDER > /sys/block/zram0/disksize && mkswap /dev/zram0 && swapon -p 100 /dev/zram0'
ExecStop=/bin/bash -c 'swapoff /dev/zram0 2>/dev/null; echo 1 > /sys/block/zram0/reset'

[Install]
WantedBy=multi-user.target
ZRAM_SERVICE

        sed -i "s|ZRAM_SIZE_PLACEHOLDER|${ZRAM_SIZE}|g" /etc/systemd/system/zram-swap.service
        systemctl daemon-reload
        systemctl enable zram-swap.service 2>/dev/null
        log_step "ZRAM service enabled (persists across reboots)"

        # Tune vm.swappiness for better ZRAM usage
        echo "vm.swappiness = 150" > /etc/sysctl.d/99-zram.conf 2>/dev/null || true
        sysctl -p /etc/sysctl.d/99-zram.conf >/dev/null 2>&1 || true
    else
        log_warn "ZRAM kernel module not available, skipping"
    fi
else
    log_step "RAM is sufficient (${TOTAL_RAM}MB), ZRAM not needed"
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 3: Install System Dependencies
# ═══════════════════════════════════════════════════════════════════
log_section "System Dependencies"

log_info "Installing packages..."

if [ "$PKG_MANAGER" = "apt" ]; then
    apt-get update -qq
    PACKAGES="podman curl jq git gcc make pkg-config libssl-dev iproute2 iptables dbus-user-session libcap2-bin openssl ca-certificates"
    # Add certbot if SSL domain is specified
    if [ -n "$DOMAIN" ]; then
        PACKAGES="$PACKAGES certbot"
    fi
    apt-get install -y -qq $PACKAGES
elif [ "$PKG_MANAGER" = "dnf" ]; then
    PACKAGES="podman curl jq git gcc make pkg-config openssl-devel iproute iptables libcap openssl ca-certificates"
    if [ -n "$DOMAIN" ]; then
        PACKAGES="$PACKAGES certbot"
    fi
    dnf install -y -q $PACKAGES
elif [ "$PKG_MANAGER" = "yum" ]; then
    PACKAGES="podman curl jq git gcc make pkgconfig openssl-devel iproute iptables libcap openssl ca-certificates"
    if [ -n "$DOMAIN" ]; then
        PACKAGES="$PACKAGES certbot"
    fi
    yum install -y -q $PACKAGES
fi

log_step "System dependencies installed"

# ═══════════════════════════════════════════════════════════════════
#  STEP 4: Install Rust Toolchain
# ═══════════════════════════════════════════════════════════════════
log_section "Rust Toolchain"

if command -v rustc &>/dev/null; then
    RUST_VER=$(rustc --version | awk '{print $2}')
    log_step "Rust already installed: $RUST_VER"
    rustup update stable 2>/dev/null || true
else
    log_info "Installing Rust (stable)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable 2>/dev/null
    export PATH="$HOME/.cargo/bin:$PATH"
    source "$HOME/.cargo/env" 2>/dev/null || true
    RUST_VER=$(rustc --version | awk '{print $2}')
    log_step "Rust installed: $RUST_VER"
fi

export PATH="$HOME/.cargo/bin:$PATH"

# ═══════════════════════════════════════════════════════════════════
#  STEP 5: Get Source Code & Compile
# ═══════════════════════════════════════════════════════════════════
log_section "NeoxAgent Source & Compilation"

if [ -f "Cargo.toml" ] && grep -q "neoxagent" Cargo.toml 2>/dev/null; then
    SOURCE_DIR="$(pwd)"
    log_step "Source found in current directory"
else
    log_info "Cloning from $REPO_URL..."
    rm -rf "$BUILD_DIR"
    git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$BUILD_DIR" 2>/dev/null || \
    git clone --depth 1 "$REPO_URL" "$BUILD_DIR"
    SOURCE_DIR="$BUILD_DIR"
    log_step "Source cloned"
fi

cd "$SOURCE_DIR"
log_info "Compiling in release mode (2-5 min on first build)..."

cargo build --release 2>&1 | tail -5

if [ ! -f "target/release/neoxagent" ]; then
    log_error "Compilation failed!"
    exit 1
fi

BINARY_SIZE=$(du -h target/release/neoxagent | awk '{print $1}')
log_step "Build successful! Binary: $BINARY_SIZE"

# Compress binary with UPX (reduces disk size ~60%)
if command -v upx &>/dev/null; then
    log_info "Compressing binary with UPX..."
    upx --best --lzma target/release/neoxagent >/dev/null 2>&1 || true
    COMPRESSED_SIZE=$(du -h target/release/neoxagent | awk '{print $1}')
    log_step "Compressed: $BINARY_SIZE → $COMPRESSED_SIZE"
else
    # Try to install UPX
    log_info "Installing UPX for binary compression..."
    if [ "$PKG_MANAGER" = "apt" ]; then
        apt-get install -y -qq upx-ucl 2>/dev/null || apt-get install -y -qq upx 2>/dev/null || true
    elif [ "$PKG_MANAGER" = "dnf" ] || [ "$PKG_MANAGER" = "yum" ]; then
        $PKG_MANAGER install -y -q upx 2>/dev/null || true
    fi

    if command -v upx &>/dev/null; then
        log_info "Compressing binary with UPX..."
        upx --best --lzma target/release/neoxagent >/dev/null 2>&1 || true
        COMPRESSED_SIZE=$(du -h target/release/neoxagent | awk '{print $1}')
        log_step "Compressed: $BINARY_SIZE → $COMPRESSED_SIZE"
    else
        log_warn "UPX not available, skipping compression"
    fi
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 6: Compile ipt2socks
# ═══════════════════════════════════════════════════════════════════
log_section "ipt2socks (SOCKS5 Transparent Proxy)"

rm -rf "$IPT2SOCKS_DIR"
log_info "Compiling ipt2socks..."

git clone --depth 1 https://github.com/zfl9/ipt2socks.git "$IPT2SOCKS_DIR" 2>/dev/null
cd "$IPT2SOCKS_DIR"
make -j"$(nproc)" 2>/dev/null
make install DESTDIR=/usr/local/bin 2>/dev/null || \
    install -m 755 ipt2socks /usr/local/bin/ipt2socks
setcap 'cap_net_bind_service,cap_net_admin+ep' /usr/local/bin/ipt2socks 2>/dev/null || true
rm -rf "$IPT2SOCKS_DIR"
cd "$SOURCE_DIR"

command -v ipt2socks &>/dev/null && log_step "ipt2socks installed" || log_warn "ipt2socks may have failed"

# ═══════════════════════════════════════════════════════════════════
#  STEP 7: SSL Certificates (if domain provided)
# ═══════════════════════════════════════════════════════════════════
TLS_ENABLED="false"
TLS_CERT=""
TLS_KEY=""

if [ -n "$DOMAIN" ]; then
    log_section "SSL Certificate (Let's Encrypt)"

    mkdir -p "$CERTS_DIR"

    # Check if certificates already exist
    if [ -f "/etc/letsencrypt/live/$DOMAIN/fullchain.pem" ]; then
        log_step "Certificates already exist for $DOMAIN"
        TLS_CERT="/etc/letsencrypt/live/$DOMAIN/fullchain.pem"
        TLS_KEY="/etc/letsencrypt/live/$DOMAIN/privkey.pem"
        TLS_ENABLED="true"
    else
        log_info "Requesting certificate for $DOMAIN..."
        log_warn "Make sure port 80 is open and $DOMAIN points to this server!"

        CERTBOT_ARGS="certonly --standalone --non-interactive --agree-tos -d $DOMAIN"
        if [ -n "$EMAIL" ]; then
            CERTBOT_ARGS="$CERTBOT_ARGS --email $EMAIL"
        else
            CERTBOT_ARGS="$CERTBOT_ARGS --register-unsafely-without-email"
        fi

        if certbot $CERTBOT_ARGS 2>/dev/null; then
            TLS_CERT="/etc/letsencrypt/live/$DOMAIN/fullchain.pem"
            TLS_KEY="/etc/letsencrypt/live/$DOMAIN/privkey.pem"
            TLS_ENABLED="true"
            log_step "SSL certificate obtained!"

            # Setup auto-renewal via a timer
            cat > /etc/systemd/system/certbot-renew.timer <<'TIMER'
[Unit]
Description=Certbot renewal timer

[Timer]
OnCalendar=*-*-* 03:00:00
RandomizedDelaySec=3600
Persistent=true

[Install]
WantedBy=timers.target
TIMER

            cat > /etc/systemd/system/certbot-renew.service <<RENEW
[Unit]
Description=Certbot renewal

[Service]
Type=oneshot
ExecStart=/usr/bin/certbot renew --quiet
ExecStartPost=/bin/systemctl restart neoxagent
RENEW

            systemctl daemon-reload
            systemctl enable --now certbot-renew.timer 2>/dev/null
            log_step "Auto-renewal configured (daily check at 3am)"
        else
            log_warn "Certificate request failed. Installing without SSL."
            log_info "You can retry later: certbot certonly --standalone -d $DOMAIN"
        fi
    fi
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 8: Install Binary & Directories
# ═══════════════════════════════════════════════════════════════════
log_section "Installation"

install -m 755 "$SOURCE_DIR/target/release/neoxagent" "$INSTALL_DIR/neoxagent"
setcap 'cap_net_admin+ep' "$INSTALL_DIR/neoxagent" 2>/dev/null || true
log_step "Binary → $INSTALL_DIR/neoxagent"

mkdir -p "$CONFIG_DIR" "$CERTS_DIR" "$DATA_DIR"/{servers,backups,stacks} /var/log/neoxagent
log_step "Directories created"

# ═══════════════════════════════════════════════════════════════════
#  STEP 9: Configuration
# ═══════════════════════════════════════════════════════════════════
log_section "Configuration"

PODMAN_SOCKET="/run/podman/podman.sock"
[ -S "/run/user/0/podman/podman.sock" ] && PODMAN_SOCKET="/run/user/0/podman/podman.sock"

# Determine port based on TLS
if [ "$TLS_ENABLED" = "true" ]; then
    LISTEN_PORT=443
else
    LISTEN_PORT=8443
fi

if [ -f "$CONFIG_DEST" ]; then
    log_warn "Config already exists (keeping existing)"
else
    API_KEY=$(openssl rand -hex 32 2>/dev/null || head -c 64 /dev/urandom | base64 | tr -d '/+=' | head -c 64)

    cat > "$CONFIG_DEST" <<CONF
[agent]
host = "0.0.0.0"
port = $LISTEN_PORT
api_key = "$API_KEY"
data_dir = "/srv/neox"

[podman]
socket = "$PODMAN_SOCKET"
volumes_dir = "/srv/neox/servers"

[tls]
enabled = $TLS_ENABLED
cert_path = "$TLS_CERT"
key_path = "$TLS_KEY"

[defaults]
restart_policy = "always"
dns = ["1.1.1.1", "8.8.8.8"]

[backups]
max_per_server = 5
max_size_gb = 10
retention_days = 30
compression_level = 6
stop_server_before_backup = true
CONF

    log_step "Config generated at $CONFIG_DEST"
    echo ""
    echo -e "  ${YELLOW}╔══════════════════════════════════════════════════╗${NC}"
    echo -e "  ${YELLOW}║  🔑 API KEY (save this!):                       ║${NC}"
    echo -e "  ${YELLOW}║  $API_KEY  ║${NC}"
    echo -e "  ${YELLOW}╚══════════════════════════════════════════════════╝${NC}"
    echo ""
fi

ln -sf "$CONFIG_DEST" /root/config.toml

# ═══════════════════════════════════════════════════════════════════
#  STEP 10: Systemd Service
# ═══════════════════════════════════════════════════════════════════
log_section "Systemd Service"

cat > "/etc/systemd/system/$SERVICE_NAME.service" <<EOF
[Unit]
Description=neoxagent - Podman Container Orchestrator
After=network-online.target podman.socket
Wants=network-online.target
Requires=podman.socket

[Service]
Type=simple
User=root
WorkingDirectory=/root
ExecStart=$INSTALL_DIR/neoxagent
Restart=always
RestartSec=5
Environment=RUST_LOG=info
LimitNOFILE=65536
LimitNPROC=4096
AmbientCapabilities=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_NET_BIND_SERVICE
StandardOutput=journal
StandardError=journal
SyslogIdentifier=neoxagent

[Install]
WantedBy=multi-user.target
EOF

log_step "Service created"

# ═══════════════════════════════════════════════════════════════════
#  STEP 11: Kernel & Network
# ═══════════════════════════════════════════════════════════════════
log_section "Kernel & Network"

cat > /etc/sysctl.d/99-neoxagent.conf <<SYSCTL
net.ipv4.ip_forward = 1
net.ipv4.conf.all.route_localnet = 1
net.ipv6.conf.all.forwarding = 1
net.core.somaxconn = 4096
net.ipv4.tcp_max_syn_backlog = 4096
SYSCTL

sysctl -p /etc/sysctl.d/99-neoxagent.conf >/dev/null 2>&1
log_step "Kernel parameters configured"

# Firewall
if command -v ufw &>/dev/null; then
    ufw allow $LISTEN_PORT/tcp comment "neoxagent" 2>/dev/null || true
    [ -n "$DOMAIN" ] && ufw allow 80/tcp comment "certbot" 2>/dev/null || true
    log_step "UFW: Port $LISTEN_PORT opened"
else
    iptables -C INPUT -p tcp --dport $LISTEN_PORT -j ACCEPT 2>/dev/null || \
        iptables -A INPUT -p tcp --dport $LISTEN_PORT -j ACCEPT 2>/dev/null || true
    log_step "iptables: Port $LISTEN_PORT opened"
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 12: Podman Optimization (critical for 1GB nodes)
# ═══════════════════════════════════════════════════════════════════
log_section "Podman Optimization"

# 1. Containers config: lightweight event logger + log size limits
mkdir -p /etc/containers
cat > /etc/containers/containers.conf <<'CONTAINERS'
[engine]
# Use "file" logger instead of "journald" — saves ~10MB RAM
events_logger = "file"

# Limit Podman internal log retention
num_locks = 2048

[engine.service_destinations]

[containers]
# Default log driver: k8s-file is lighter than journald
log_driver = "k8s-file"

# Cap container logs to 5MB max (prevents disk fill)
log_size_max = 5242880

# Use host DNS by default
dns_servers = ["1.1.1.1", "8.8.8.8"]
CONTAINERS

log_step "Podman event logger → file (saves ~10MB RAM)"

# 2. Delete existing invalid storage configs to avoid 'runroot must be set' errors in Podman 5.0+
if [ -f /etc/containers/storage.conf ]; then
    rm -f /etc/containers/storage.conf
    log_step "Removed legacy storage config to use Podman default"
fi

# 3. Disable Podman auto-update (uses RAM for no benefit here)
systemctl disable --now podman-auto-update.timer 2>/dev/null || true
systemctl disable --now podman-auto-update.service 2>/dev/null || true
systemctl disable --now podman-restart.service 2>/dev/null || true
log_step "Disabled unnecessary Podman services"

# 4. Clean up any leftover containers/images to free space
podman system prune -f 2>/dev/null || true
log_step "Cleaned old container artifacts"

# ═══════════════════════════════════════════════════════════════════
#  STEP 12: Start Services
# ═══════════════════════════════════════════════════════════════════
log_section "Starting Services"

systemctl daemon-reload
systemctl enable --now podman.socket 2>/dev/null || log_warn "podman.socket not available"
systemctl enable --now "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"
log_step "All services started"

# ═══════════════════════════════════════════════════════════════════
#  STEP 13: Verification
# ═══════════════════════════════════════════════════════════════════
log_section "Verification"

sleep 3
ERRORS=0

if systemctl is-active --quiet "$SERVICE_NAME"; then
    log_step "neoxagent → RUNNING"
else
    log_error "neoxagent → FAILED"
    echo "       Logs: journalctl -u neoxagent -n 30 --no-pager"
    ERRORS=$((ERRORS + 1))
fi

# Check API
if [ "$TLS_ENABLED" = "true" ]; then
    HEALTH_URL="https://127.0.0.1:$LISTEN_PORT/api/health"
    CURL_OPTS="-sk"
else
    HEALTH_URL="http://127.0.0.1:$LISTEN_PORT/api/health"
    CURL_OPTS="-s"
fi

API_CODE=$(curl $CURL_OPTS -o /dev/null -w "%{http_code}" "$HEALTH_URL" 2>/dev/null || echo "000")
if [ "$API_CODE" = "200" ] || [ "$API_CODE" = "401" ]; then
    log_step "API → HTTP $API_CODE on port $LISTEN_PORT"
else
    log_warn "API → HTTP $API_CODE (may need a moment to start)"
fi

command -v ipt2socks &>/dev/null && log_step "ipt2socks → OK" || log_warn "ipt2socks → missing"
command -v tc &>/dev/null && log_step "tc → OK" || log_warn "tc → missing"

PODMAN_VER=$(podman --version 2>/dev/null | awk '{print $3}' || echo "?")
log_step "Podman → v$PODMAN_VER"

if [ "$TOTAL_RAM" -le 2048 ] && swapon --show | grep -q zram 2>/dev/null; then
    ZRAM_USED=$(swapon --show=NAME,SIZE | grep zram | awk '{print $2}')
    log_step "ZRAM → Active ($ZRAM_USED compressed swap)"
fi

# Cleanup build dir if we cloned
[ "$SOURCE_DIR" = "$BUILD_DIR" ] && rm -rf "$BUILD_DIR"

# ═══════════════════════════════════════════════════════════════════
#  Summary
# ═══════════════════════════════════════════════════════════════════
echo ""
echo -e "${CYAN}═══════════════════════════════════════════════════${NC}"
if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}  ✅ Installation Complete!${NC}"
else
    echo -e "${YELLOW}  ⚠️  Installed with $ERRORS issue(s)${NC}"
fi
echo -e "${CYAN}═══════════════════════════════════════════════════${NC}"
echo ""
echo -e "${BOLD}  Components:${NC}"
echo "    neoxagent      $INSTALL_DIR/neoxagent"
echo "    ipt2socks      /usr/local/bin/ipt2socks"
echo "    Config         $CONFIG_DEST"
echo ""
echo -e "${BOLD}  Network:${NC}"
if [ "$TLS_ENABLED" = "true" ]; then
    echo -e "    🔒 HTTPS (native TLS) on port $LISTEN_PORT"
    echo "    Certificate: $TLS_CERT"
    echo "    Auto-renewal: Enabled"
    echo "    Health: curl -sk https://127.0.0.1:$LISTEN_PORT/api/health"
else
    echo "    🔓 HTTP on port $LISTEN_PORT"
    echo "    Health: curl -s http://127.0.0.1:$LISTEN_PORT/api/health"
    echo ""
    echo -e "    ${YELLOW}To enable SSL later:${NC}"
    echo "    sudo NEOX_DOMAIN=nodo1.tudominio.com ./install_neox.sh"
fi
echo ""
if [ "$TOTAL_RAM" -le 2048 ]; then
    echo -e "${BOLD}  Memory (optimized for ${TOTAL_RAM}MB):${NC}"
    echo "    ZRAM:          Enabled (lz4 compression)"
    echo "    TLS:           Native (no Nginx needed)"
    echo "    Podman:        Optimized (file logger, no auto-update)"
    echo "    Agent RAM:     ~5-10MB"
    echo "    Available:     ~$((TOTAL_RAM - 140))MB for containers"
fi
echo ""
echo -e "${BOLD}  Commands:${NC}"
echo "    Status:    systemctl status neoxagent"
echo "    Logs:      journalctl -u neoxagent -f"
echo "    Restart:   systemctl restart neoxagent"
echo "    Config:    nano $CONFIG_DEST"
echo ""
echo -e "${BOLD}  Update:${NC}"
echo "    cd /path/to/NeoxAgent && git pull"
echo "    cargo build --release"
echo "    sudo cp target/release/neoxagent $INSTALL_DIR/"
echo "    sudo systemctl restart neoxagent"
echo ""
echo -e "${CYAN}═══════════════════════════════════════════════════${NC}"
