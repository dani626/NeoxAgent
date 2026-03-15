#!/bin/bash
set -euo pipefail

# ═══════════════════════════════════════════════════════════════════
#  NeoxAgent Reinstaller v1.0
#  Reinstalls NeoxAgent from latest source code.
#  Ensures all build dependencies are present, compiles from scratch,
#  replaces the binary, and restarts the service.
#
#  Usage:
#    sudo ./reinstall_neox.sh
#
#  Options (env vars):
#    NEOX_BRANCH=master        Branch to clone (default: master)
#    NEOX_REPO=https://...     Custom repo URL
#    NEOX_SKIP_DEPS=1          Skip dependency check (faster)
#    NEOX_SKIP_TPROXY=1        Skip hev-socks5-tproxy recompilation
#    NEOX_SKIP_SIDECAR=1       Skip sidecar image rebuild
# ═══════════════════════════════════════════════════════════════════

# ─── Configuration ──────────────────────────────────────────────────
REPO_URL="${NEOX_REPO:-https://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@github.com/dani626/NeoxAgent.git}"
BRANCH="${NEOX_BRANCH:-master}"
BUILD_DIR="/tmp/neoxagent_build"
INSTALL_DIR="/usr/local/bin"
SERVICE_NAME="neoxagent"
HEV_TPROXY_DIR="/tmp/hev-socks5-tproxy_build"
SKIP_DEPS="${NEOX_SKIP_DEPS:-0}"
SKIP_TPROXY="${NEOX_SKIP_TPROXY:-1}"  # Default to 1 to save time
SKIP_SIDECAR="${NEOX_SKIP_SIDECAR:-1}" # Default to 1
USE_PRECOMPILED="${NEOX_USE_PRECOMPILED:-1}" # New: Default to download instead of build
GITHUB_TOKEN="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"
REPO_NAME="dani626/NeoxAgent"

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
    echo -e "${GREEN}🔄 NeoxAgent Reinstaller v1.0${NC}"
    echo "═══════════════════════════════════════════"
    echo -e "  ${CYAN}Branch:${NC} $BRANCH"
    echo ""
}

log_step()    { echo -e "  ${GREEN}[✓]${NC} $1"; }
log_warn()    { echo -e "  ${YELLOW}[!]${NC} $1"; }
log_error()   { echo -e "  ${RED}[✗]${NC} $1"; }
log_info()    { echo -e "  ${CYAN}[→]${NC} $1"; }
log_section() { echo -e "\n${BOLD}── $1 ──${NC}"; }

elapsed_since() {
    local start=$1
    local now
    now=$(date +%s)
    local diff=$((now - start))
    printf "%dm %ds" $((diff / 60)) $((diff % 60))
}

print_header

TOTAL_START=$(date +%s)

# ═══════════════════════════════════════════════════════════════════
#  STEP 1: Pre-flight Checks
# ═══════════════════════════════════════════════════════════════════
log_section "Pre-flight Checks"

if [[ $EUID -ne 0 ]]; then
    log_error "This script must be run as root."
    echo "       Run: sudo ./reinstall_neox.sh"
    exit 1
fi
log_step "Running as root"

ARCH=$(uname -m)
if [[ "$ARCH" != "x86_64" && "$ARCH" != "aarch64" ]]; then
    log_error "Unsupported architecture: $ARCH"
    exit 1
fi
log_step "Architecture: $ARCH"

if [ -f /etc/os-release ]; then
    . /etc/os-release
    log_step "OS: $PRETTY_NAME"
fi

TOTAL_RAM=$(awk '/MemTotal/ {printf "%d", $2/1024}' /proc/meminfo)
log_step "RAM: ${TOTAL_RAM}MB"

# Check if neoxagent is currently installed
if [ -f "$INSTALL_DIR/neoxagent" ]; then
    OLD_SIZE=$(du -h "$INSTALL_DIR/neoxagent" | awk '{print $1}')
    OLD_DATE=$(stat -c '%y' "$INSTALL_DIR/neoxagent" 2>/dev/null | cut -d'.' -f1)
    log_step "Current binary: $OLD_SIZE (modified: $OLD_DATE)"
else
    log_warn "No existing binary found at $INSTALL_DIR/neoxagent (fresh install)"
fi

# Check if config exists
if [ -f /etc/neoxagent/config.toml ]; then
    log_step "Config: /etc/neoxagent/config.toml exists (will be preserved)"
else
    log_warn "No config found — you may need to run install_neox.sh for first-time setup"
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 2: Ensure Dependencies
# ═══════════════════════════════════════════════════════════════════
if [ "$SKIP_DEPS" != "1" ]; then
    log_section "Dependencies"

    # Detect package manager
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

    # Check and install missing packages
    REQUIRED_CMDS="gcc make pkg-config git curl openssl"
    MISSING_PKGS=""

    for cmd in $REQUIRED_CMDS; do
        if ! command -v "$cmd" &>/dev/null; then
            MISSING_PKGS="$MISSING_PKGS $cmd"
        fi
    done

    # Check for libssl-dev (needed for compilation)
    if [ "$PKG_MANAGER" = "apt" ]; then
        if ! dpkg -s libssl-dev &>/dev/null 2>&1; then
            MISSING_PKGS="$MISSING_PKGS libssl-dev"
        fi
    elif [ "$PKG_MANAGER" = "dnf" ] || [ "$PKG_MANAGER" = "yum" ]; then
        if ! rpm -q openssl-devel &>/dev/null 2>&1; then
            MISSING_PKGS="$MISSING_PKGS openssl-devel"
        fi
    fi

    if [ -n "$MISSING_PKGS" ]; then
        log_info "Installing missing packages:$MISSING_PKGS"
        if [ "$PKG_MANAGER" = "apt" ]; then
            apt-get update -qq
            apt-get install -y -qq $MISSING_PKGS
        elif [ "$PKG_MANAGER" = "dnf" ]; then
            dnf install -y -q $MISSING_PKGS
        elif [ "$PKG_MANAGER" = "yum" ]; then
            yum install -y -q $MISSING_PKGS
        fi
        log_step "Missing packages installed"
    else
        log_step "All build dependencies present"
    fi

    # Ensure Rust is installed
    if command -v rustc &>/dev/null; then
        RUST_VER=$(rustc --version | awk '{print $2}')
        log_step "Rust: $RUST_VER"
    else
        log_info "Installing Rust (stable)..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable 2>/dev/null
        log_step "Rust installed"
    fi

    # Ensure Podman is available
    if command -v podman &>/dev/null; then
        PODMAN_VER=$(podman --version | awk '{print $3}')
        log_step "Podman: v$PODMAN_VER"
    else
        log_warn "Podman not found! Installing..."
        if [ "$PKG_MANAGER" = "apt" ]; then
            apt-get install -y -qq podman
        else
            $PKG_MANAGER install -y -q podman
        fi
        log_step "Podman installed"
    fi
else
    log_section "Dependencies (skipped)"
    log_info "NEOX_SKIP_DEPS=1 — assuming all dependencies are present"
fi

export PATH="$HOME/.cargo/bin:$PATH"

# ═══════════════════════════════════════════════════════════════════
#  STEP 3: Stop Current Service
# ═══════════════════════════════════════════════════════════════════
log_section "Stop Current Service"

if systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
    systemctl stop "$SERVICE_NAME"
    log_step "Service stopped"
else
    log_info "Service was not running"
fi

# Backup current binary
if [ -f "$INSTALL_DIR/neoxagent" ]; then
    cp "$INSTALL_DIR/neoxagent" "$INSTALL_DIR/neoxagent.bak"
    log_step "Current binary backed up → neoxagent.bak"
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 4: Clone & Compile
# ═══════════════════════════════════════════════════════════════════
log_section "Installation Mode"

if [ "$USE_PRECOMPILED" = "1" ]; then
    log_info "Mode: Download pre-compiled binary from GitHub"
else
    log_info "Mode: Compile from source on VPS"
fi

if [ "$USE_PRECOMPILED" = "1" ]; then
    log_info "Fetching latest artifact from GitHub..."
    
    # Get the latest successful run ID
    RUN_ID=$(curl -s -H "Authorization: token $GITHUB_TOKEN" \
        "https://api.github.com/repos/$REPO_NAME/actions/runs?status=success&branch=$BRANCH" \
        | jq -r '.workflow_runs[0].id')

    if [ "$RUN_ID" != "null" ] && [ -n "$RUN_ID" ]; then
        log_step "Latest successful run found: $RUN_ID"
        
        # Get artifact download URL
        ARTIFACT_URL=$(curl -s -H "Authorization: token $GITHUB_TOKEN" \
            "https://api.github.com/repos/$REPO_NAME/actions/runs/$RUN_ID/artifacts" \
            | jq -r '.artifacts[] | select(.name == "neoxagent-linux-x64") | .archive_download_url')

        if [ -n "$ARTIFACT_URL" ]; then
            log_info "Downloading binary (zip)..."
            curl -L -H "Authorization: token $GITHUB_TOKEN" -o /tmp/neoxagent.zip "$ARTIFACT_URL"
            
            log_info "Extracting..."
            apt-get install -y unzip >/dev/null 2>&1 || yum install -y unzip >/dev/null 2>&1
            unzip -o /tmp/neoxagent.zip -d /tmp/neox_extracted
            
            if [ -f "/tmp/neox_extracted/neoxagent" ]; then
                mkdir -p "$BUILD_DIR/target/release"
                mv "/tmp/neox_extracted/neoxagent" "$BUILD_DIR/target/release/neoxagent"
                chmod +x "$BUILD_DIR/target/release/neoxagent"
                log_step "Download successful!"
                rm -rf /tmp/neoxagent.zip /tmp/neox_extracted
            else
                log_warn "Extracted file not found. Falling back to compilation..."
                USE_PRECOMPILED=0
            fi
        else
            log_warn "Artifact URL not found. Falling back to compilation..."
            USE_PRECOMPILED=0
        fi
    else
        log_warn "No successful GitHub Actions runs found. Falling back to compilation..."
        USE_PRECOMPILED=0
    fi
fi

if [ "$USE_PRECOMPILED" != "1" ]; then
    # Clean old build
    rm -rf "$BUILD_DIR"
    log_info "Cloning branch '$BRANCH'..."

    if git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$BUILD_DIR" 2>/dev/null; then
        log_step "Repository cloned"
    else
        # Try without specifying branch (fallback)
        log_warn "Branch '$BRANCH' not found, trying default branch..."
        git clone --depth 1 "$REPO_URL" "$BUILD_DIR" 2>/dev/null
        log_step "Repository cloned (default branch)"
    fi

    cd "$BUILD_DIR"
    COMMIT=$(git log --oneline -1 2>/dev/null || echo "unknown")
    log_step "Latest commit: $COMMIT"

    log_info "Compiling in release mode (this may take 2-15 min)..."
    BUILD_START=$(date +%s)

    if cargo build --release 2>&1; then
        BUILD_TIME=$(elapsed_since "$BUILD_START")
        NEW_SIZE=$(du -h target/release/neoxagent | awk '{print $1}')
        log_step "Build successful! Binary: $NEW_SIZE (took $BUILD_TIME)"
    else
        log_error "Compilation failed!"
        log_info "Restoring backup..."
        if [ -f "$INSTALL_DIR/neoxagent.bak" ]; then
            mv "$INSTALL_DIR/neoxagent.bak" "$INSTALL_DIR/neoxagent"
            systemctl start "$SERVICE_NAME" 2>/dev/null || true
            log_warn "Previous version restored and service started"
        fi
        exit 1
    fi
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 5: Install Binary
# ═══════════════════════════════════════════════════════════════════
log_section "Install Binary"

install -m 755 "$BUILD_DIR/target/release/neoxagent" "$INSTALL_DIR/neoxagent"
setcap 'cap_net_admin+ep' "$INSTALL_DIR/neoxagent" 2>/dev/null || true
log_step "Binary installed → $INSTALL_DIR/neoxagent"

# ═══════════════════════════════════════════════════════════════════
#  STEP 6: Recompile hev-socks5-tproxy (optional)
# ═══════════════════════════════════════════════════════════════════
if [ "$SKIP_TPROXY" != "1" ]; then
    log_section "hev-socks5-tproxy"

    if command -v hev-socks5-tproxy &>/dev/null; then
        log_info "Already installed, recompiling for latest version..."
    else
        log_info "Not found, compiling..."
    fi

    rm -rf "$HEV_TPROXY_DIR"
    if git clone --depth 1 --recursive https://github.com/heiher/hev-socks5-tproxy.git "$HEV_TPROXY_DIR" 2>/dev/null; then
        cd "$HEV_TPROXY_DIR"
        if make -j"$(nproc)" 2>/dev/null; then
            install -m 755 bin/hev-socks5-tproxy /usr/local/bin/hev-socks5-tproxy
            setcap 'cap_net_bind_service,cap_net_admin+ep' /usr/local/bin/hev-socks5-tproxy 2>/dev/null || true
            log_step "hev-socks5-tproxy compiled and installed"
        else
            log_warn "hev-socks5-tproxy compilation failed (non-critical)"
        fi
        rm -rf "$HEV_TPROXY_DIR"
    else
        log_warn "Failed to clone hev-socks5-tproxy repo (non-critical)"
    fi
else
    log_section "hev-socks5-tproxy (skipped)"
    log_info "NEOX_SKIP_TPROXY=1"
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 7: Rebuild Sidecar Image (optional)
# ═══════════════════════════════════════════════════════════════════
if [ "$SKIP_SIDECAR" != "1" ]; then
    log_section "Sidecar Image"

    if podman image exists neox-tproxy-sidecar:latest 2>/dev/null; then
        log_info "Rebuilding neox-tproxy-sidecar image..."
    else
        log_info "Building neox-tproxy-sidecar image..."
    fi

    SIDECAR_DOCKERFILE=$(mktemp)
    cat > "$SIDECAR_DOCKERFILE" <<'SIDECAR_DF'
FROM docker.io/library/debian:bookworm-slim
RUN apt-get update -qq && \
    apt-get install -yq --no-install-recommends iptables iproute2 ca-certificates && \
    apt-get clean && rm -rf /var/lib/apt/lists/*
SIDECAR_DF

    if podman build -t neox-tproxy-sidecar:latest -f "$SIDECAR_DOCKERFILE" /tmp 2>/dev/null; then
        log_step "neox-tproxy-sidecar image built"
    else
        log_warn "Sidecar image build failed (non-critical, will use runtime install)"
    fi
    rm -f "$SIDECAR_DOCKERFILE"
else
    log_section "Sidecar Image (skipped)"
    log_info "NEOX_SKIP_SIDECAR=1"
fi

# ═══════════════════════════════════════════════════════════════════
#  STEP 8: Restart Service
# ═══════════════════════════════════════════════════════════════════
log_section "Start Service"

systemctl daemon-reload
systemctl enable --now podman.socket 2>/dev/null || true
systemctl start "$SERVICE_NAME"
log_step "Service started"

# ═══════════════════════════════════════════════════════════════════
#  STEP 9: Verification
# ═══════════════════════════════════════════════════════════════════
log_section "Verification"

sleep 3
ERRORS=0

# Check service
if systemctl is-active --quiet "$SERVICE_NAME"; then
    log_step "neoxagent → RUNNING"
else
    log_error "neoxagent → FAILED"
    echo "       Logs: journalctl -u neoxagent -n 30 --no-pager"
    ERRORS=$((ERRORS + 1))

    # Attempt rollback
    if [ -f "$INSTALL_DIR/neoxagent.bak" ]; then
        log_warn "Attempting rollback to previous version..."
        mv "$INSTALL_DIR/neoxagent.bak" "$INSTALL_DIR/neoxagent"
        systemctl start "$SERVICE_NAME" 2>/dev/null || true
        if systemctl is-active --quiet "$SERVICE_NAME"; then
            log_step "Rollback successful — previous version is running"
        else
            log_error "Rollback also failed!"
        fi
    fi
fi

# Check API health
LISTEN_PORT=$(grep -oP 'port\s*=\s*\K[0-9]+' /etc/neoxagent/config.toml 2>/dev/null || echo "8443")
TLS_ENABLED=$(grep -oP 'enabled\s*=\s*\K\w+' /etc/neoxagent/config.toml 2>/dev/null || echo "false")

if [ "$TLS_ENABLED" = "true" ]; then
    HEALTH_URL="https://127.0.0.1:$LISTEN_PORT/api/health"
    CURL_OPTS="-sk"
else
    HEALTH_URL="http://127.0.0.1:$LISTEN_PORT/api/health"
    CURL_OPTS="-s"
fi

HEALTH_RESPONSE=$(curl $CURL_OPTS "$HEALTH_URL" 2>/dev/null || echo "FAIL")
API_CODE=$(curl $CURL_OPTS -o /dev/null -w "%{http_code}" "$HEALTH_URL" 2>/dev/null || echo "000")

if [ "$API_CODE" = "200" ]; then
    log_step "API → HTTP $API_CODE ✓"
    log_info "Response: $HEALTH_RESPONSE"
elif [ "$API_CODE" = "401" ]; then
    log_step "API → HTTP $API_CODE (auth required — working)"
else
    log_warn "API → HTTP $API_CODE (may need a moment to start)"
    ERRORS=$((ERRORS + 1))
fi

# Check components
command -v hev-socks5-tproxy &>/dev/null && log_step "hev-socks5-tproxy → OK" || log_warn "hev-socks5-tproxy → missing"
podman image exists neox-tproxy-sidecar:latest 2>/dev/null && log_step "neox-tproxy-sidecar → OK" || log_warn "neox-tproxy-sidecar → missing"

PODMAN_VER=$(podman --version 2>/dev/null | awk '{print $3}' || echo "?")
log_step "Podman → v$PODMAN_VER"

# Memory usage
AGENT_MEM=$(ps -o rss= -p "$(pgrep -x neoxagent 2>/dev/null || echo 0)" 2>/dev/null | awk '{printf "%.1f", $1/1024}')
[ -n "$AGENT_MEM" ] && [ "$AGENT_MEM" != "0.0" ] && log_step "Agent memory → ${AGENT_MEM}MB"

# Cleanup
rm -rf "$BUILD_DIR"
log_step "Build directory cleaned"

# Remove backup if everything is OK
if [ $ERRORS -eq 0 ] && [ -f "$INSTALL_DIR/neoxagent.bak" ]; then
    rm -f "$INSTALL_DIR/neoxagent.bak"
    log_step "Old backup removed"
fi

TOTAL_TIME=$(elapsed_since "$TOTAL_START")

# ═══════════════════════════════════════════════════════════════════
#  Summary
# ═══════════════════════════════════════════════════════════════════
echo ""
echo -e "${CYAN}═══════════════════════════════════════════════════${NC}"
if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}  ✅ Reinstallation Complete! (${TOTAL_TIME})${NC}"
else
    echo -e "${YELLOW}  ⚠️  Reinstalled with $ERRORS issue(s) (${TOTAL_TIME})${NC}"
fi
echo -e "${CYAN}═══════════════════════════════════════════════════${NC}"
echo ""
echo -e "${BOLD}  Details:${NC}"
echo "    Commit:   $COMMIT"
echo "    Binary:   $INSTALL_DIR/neoxagent ($NEW_SIZE)"
echo "    Config:   /etc/neoxagent/config.toml (preserved)"
echo "    Port:     $LISTEN_PORT"
echo ""
echo -e "${BOLD}  Quick Commands:${NC}"
echo "    Status:    systemctl status neoxagent"
echo "    Logs:      journalctl -u neoxagent -f"
echo "    Restart:   systemctl restart neoxagent"
echo ""
echo -e "${BOLD}  Quick Reinstall:${NC}"
echo "    curl -sL <your-url>/reinstall_neox.sh | sudo bash"
echo "    # or: sudo NEOX_SKIP_DEPS=1 ./reinstall_neox.sh  (faster)"
echo ""
echo -e "${CYAN}═══════════════════════════════════════════════════${NC}"
