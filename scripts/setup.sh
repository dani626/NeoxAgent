#!/bin/bash
# =============================================================================
# NeoxAgent — Master Setup Script
# Install, reinstall or update on any VPS node automatically.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/dani626/NeoxAgent/master/scripts/setup.sh | bash
#   # or clone and run:
#   bash scripts/setup.sh [--update] [--reinstall]
#
# Flags:
#   (none)        Fresh install
#   --update      Pull latest code + recompile + restart (keeps config)
#   --reinstall   Wipe everything and start fresh
# =============================================================================
set -euo pipefail

# ─── Colors ───────────────────────────────────────────────────────────────────
RED='\e[0;31m'; GREEN='\e[0;32m'; YELLOW='\e[1;33m'
BLUE='\e[0;34m'; CYAN='\e[0;36m'; BOLD='\e[1m'; RESET='\e[0m'

info()   { echo -e "${BLUE}[INFO]${RESET} $*"; }
ok()     { echo -e "${GREEN}[OK]${RESET}   $*"; }
warn()   { echo -e "${YELLOW}[WARN]${RESET} $*"; }
err()    { echo -e "${RED}[ERR]${RESET}  $*" >&2; }
header() { echo -e "\n${BOLD}${CYAN}=== $* ===${RESET}"; }

# ─── Config ───────────────────────────────────────────────────────────────────
REPO_URL="https://github.com/dani626/NeoxAgent.git"
INSTALL_DIR="/opt/neoxagent"
BIN_PATH="/usr/local/bin/neoxagent"
SERVICE_NAME="neoxagent"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
CONFIG_FILE="${INSTALL_DIR}/config.toml"
MODE="install"

for arg in "$@"; do
  case "$arg" in
    --update)    MODE="update"    ;;
    --reinstall) MODE="reinstall" ;;
  esac
done

if [ "$(id -u)" -ne 0 ]; then
  err "This script must be run as root."
  exit 1
fi

echo -e ""
echo -e "${BOLD}${CYAN}"
echo -e "  ███╗   ██╗███████╗ ██████╗ ██╗  ██╗    █████╗  ██████╗ ███████╗███╗   ██╗████████╗"
echo -e "  ████╗  ██║██╔════╝██╔═══██╗╚██╗██╔╝   ██╔══██╗██╔════╝ ██╔════╝████╗  ██║╚══██╔══╝"
echo -e "  ██╔██╗ ██║█████╗  ██║   ██║ ╚███╔╝    ███████║██║  ███╗█████╗  ██╔██╗ ██║   ██║   "
echo -e "  ██║╚██╗██║██╔══╝  ██║   ██║ ██╔██╗    ██╔══██║██║   ██║██╔══╝  ██║╚██╗██║   ██║   "
echo -e "  ██║ ╚████║███████╗╚██████╔╝██╔╝ ██╗   ██║  ██║╚██████╔╝███████╗██║ ╚████║   ██║   "
echo -e "  ╚═╝  ╚═══╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═══╝   ╚═╝   "
echo -e "${RESET}"
echo -e "  ${BOLD}Mode: ${YELLOW}${MODE^^}${RESET}\n"

# ─── Step 1: Dependencies ─────────────────────────────────────────────────────
header "Step 1/7 — Installing system dependencies"

apt-get update -qq
apt-get install -yq \
  curl git build-essential pkg-config libssl-dev \
  iptables iproute2 ca-certificates podman jq 2>/dev/null
ok "Dependencies installed"

if ! command -v cargo &>/dev/null; then
  info "Installing Rust toolchain..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  source "$HOME/.cargo/env"
  ok "Rust installed: $(rustc --version)"
else
  ok "Rust already present: $(rustc --version)"
fi
export PATH="$HOME/.cargo/bin:$PATH"

# ─── Step 2: Reinstall cleanup ────────────────────────────────────────────────
if [ "$MODE" = "reinstall" ]; then
  header "Step 2/7 — Wiping previous installation"
  systemctl stop  "$SERVICE_NAME" 2>/dev/null || true
  systemctl stop  "neox-guard"    2>/dev/null || true
  systemctl disable "$SERVICE_NAME" 2>/dev/null || true
  systemctl disable "neox-guard"    2>/dev/null || true
  rm -f "$BIN_PATH" "$SERVICE_FILE" /etc/systemd/system/neox-guard.service
  rm -rf "$INSTALL_DIR"
  systemctl daemon-reload
  ok "Previous installation wiped"
fi

# ─── Step 3: Clone / Pull repo ────────────────────────────────────────────────
header "Step 3/7 — Fetching source code"

if [ -d "$INSTALL_DIR/.git" ]; then
  info "Repo exists, pulling latest..."
  git -C "$INSTALL_DIR" pull origin master
else
  info "Cloning repo..."
  git clone "$REPO_URL" "$INSTALL_DIR"
fi
ok "Source code ready at $INSTALL_DIR"

# ─── Step 4: Configuration ────────────────────────────────────────────────────
header "Step 4/7 — Configuration"

if [ "$MODE" = "update" ] && [ -f "$CONFIG_FILE" ]; then
  ok "Keeping existing config.toml (--update mode)"
else
  PODMAN_SOCK=$(podman info --format '{{.Host.RemoteSocket.Path}}' 2>/dev/null || echo "/run/podman/podman.sock")

  echo ""
  echo -e "  ${BOLD}Configure this node:${RESET}\n"

  read -rp "  Agent port        [8443]: "                    INPUT_PORT;  AGENT_PORT="${INPUT_PORT:-8443}"
  GENERATED_KEY=$(cat /proc/sys/kernel/random/uuid | tr -d '-')
  read -rp "  API key           [auto-generate]: "            INPUT_KEY;   API_KEY="${INPUT_KEY:-$GENERATED_KEY}"
  read -rp "  Podman socket     [$PODMAN_SOCK]: "             INPUT_SOCK;  PODMAN_SOCKET="${INPUT_SOCK:-$PODMAN_SOCK}"
  read -rp "  Data dir          [/var/lib/neoxagent]: "       INPUT_DATA;  DATA_DIR_CFG="${INPUT_DATA:-/var/lib/neoxagent}"
  read -rp "  Volumes dir       [/var/lib/neoxagent/servers]: " INPUT_VOLS; VOLUMES_DIR="${INPUT_VOLS:-/var/lib/neoxagent/servers}"

  TLS_ENABLED="false"; TLS_CERT=""; TLS_KEY=""
  read -rp "  Enable TLS?       [y/N]: " INPUT_TLS
  if [[ "${INPUT_TLS,,}" == "y" || "${INPUT_TLS,,}" == "yes" ]]; then
    TLS_ENABLED="true"
    read -rp "  TLS cert path: " TLS_CERT
    read -rp "  TLS key path:  " TLS_KEY
  fi

  # ── CORS origins ────────────────────────────────────────────────────────────
  echo ""
  echo -e "  ${BOLD}CORS allowed origins${RESET}"
  echo -e "  Dominios que pueden hacer requests a este agente (separados por espacios)."
  echo -e "  Ej: ${CYAN}panel.neox.com *.neox.com http://localhost:3000${RESET}"
  echo -e "  ${RED}Si dejas en blanco: no habrá lista blanca (todos los orígenes permitidos).${RESET}\n"

  read -rp "  CORS origins      [sin lista blanca]: " INPUT_CORS

  CORS_ENTRIES=()
  if [ -n "$INPUT_CORS" ]; then
    for origin in $INPUT_CORS; do
      if [[ "$origin" =~ ^https?:// || "$origin" =~ ^\*\. ]]; then
        CORS_ENTRIES+=("$origin")
      else
        warn "  Ignorado '$origin' — debe iniciar con http://, https:// o *.dominio"
      fi
    done
  fi

  # Build TOML array string
  if [ ${#CORS_ENTRIES[@]} -eq 0 ]; then
    warn "Sin orígenes — cors_origins vacío (TODOS los orígenes permitidos)."
    CORS_TOML="cors_origins = []"
  else
    CORS_TOML="cors_origins = ["
    for origin in "${CORS_ENTRIES[@]}"; do
      CORS_TOML+=$'\n  '
      CORS_TOML+="\"${origin}\","
    done
    CORS_TOML="${CORS_TOML%,}"
    CORS_TOML+=$'\n]'
  fi
  # ── end CORS ────────────────────────────────────────────────────────────────

  mkdir -p "$DATA_DIR_CFG" "$VOLUMES_DIR"

  cat > "$CONFIG_FILE" <<EOF
[agent]
host = "0.0.0.0"
port = ${AGENT_PORT}
api_key = "${API_KEY}"
data_dir = "${DATA_DIR_CFG}"
# Allowed CORS origins. Exact (https://domain.com) or wildcard subdomain (*.domain.com).
# Empty = allow all (not safe for production).
${CORS_TOML}

[podman]
socket = "${PODMAN_SOCKET}"
volumes_dir = "${VOLUMES_DIR}"

[tls]
enabled = ${TLS_ENABLED}
cert_path = "${TLS_CERT}"
key_path = "${TLS_KEY}"

[defaults]
restart_policy = "always"
dns = ["1.1.1.1", "8.8.8.8"]

[backups]
max_per_server = 5
max_size_gb = 10
retention_days = 30
compression_level = 6
stop_server_before_backup = true
EOF
  ok "config.toml written to $CONFIG_FILE"
fi

FINAL_API_KEY=$(grep 'api_key' "$CONFIG_FILE" | sed 's/.*= "//;s/"//')
FINAL_PORT=$(grep 'port' "$CONFIG_FILE" | head -1 | sed 's/[^0-9]*//g')

# ─── Step 5: Build ────────────────────────────────────────────────────────────
header "Step 5/7 — Compiling neoxagent"

# Stop service before replacing the binary to avoid "Text file busy"
if systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
  info "Stopping $SERVICE_NAME before binary replacement..."
  systemctl stop "$SERVICE_NAME"
fi

info "Running cargo build --release (this may take a few minutes)..."
cd "$INSTALL_DIR"
cargo build --release 2>&1 | tail -5

cp target/release/neoxagent "$BIN_PATH"
chmod +x "$BIN_PATH"
ok "Binary installed at $BIN_PATH"

# ─── Step 6: Systemd services ─────────────────────────────────────────────────
header "Step 6/7 — Installing systemd services"

cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=NeoxAgent — Podman Management Agent
Documentation=https://github.com/dani626/NeoxAgent
After=network-online.target podman.socket neox-guard.service
Wants=network-online.target
Requires=neox-guard.service

[Service]
Type=simple
ExecStart=${BIN_PATH}
WorkingDirectory=${INSTALL_DIR}
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=neoxagent

[Install]
WantedBy=multi-user.target
EOF

cat > /etc/systemd/system/neox-guard.service <<'EOF'
[Unit]
Description=Neox host-level container IP leak guard
Documentation=https://github.com/dani626/NeoxAgent
Before=podman.service podman-restart.service neoxagent.service network-online.target
After=network.target
DefaultDependencies=no
ConditionPathExists=/sbin/iptables

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/sbin/iptables -I FORWARD 1 -m comment --comment "neox-guard-forward-drop" -j DROP
ExecStop=/sbin/iptables -D FORWARD -m comment --comment "neox-guard-forward-drop" -j DROP
ExecStopPost=/sbin/iptables -D FORWARD -m comment --comment "neox-guard-forward-drop" -j DROP

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable neox-guard
systemctl start  neox-guard
ok "neox-guard.service enabled and started (FORWARD DROP active)"

systemctl enable "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"
ok "neoxagent.service enabled and started"

info "Waiting for agent to respond..."
for i in $(seq 1 15); do
  if curl -sf "http://127.0.0.1:${FINAL_PORT}/api/health" \
       -H "Authorization: Bearer ${FINAL_API_KEY}" &>/dev/null; then
    ok "Agent is up!"
    break
  fi
  sleep 1
done

# ─── Step 7: Activate guard via API ───────────────────────────────────────────
header "Step 7/7 — Activating IP leak guard"

GUARD_RESPONSE=$(curl -sf -X POST \
  "http://127.0.0.1:${FINAL_PORT}/api/guard/install" \
  -H "Authorization: Bearer ${FINAL_API_KEY}" \
  -H "Content-Type: application/json" || echo '{"success":false}')

if echo "$GUARD_RESPONSE" | grep -q '"success":true'; then
  ok "Guard installed via API"
else
  warn "Guard API call failed (service already active via systemd, this is fine)"
fi

if iptables -C FORWARD -m comment --comment "neox-guard-forward-drop" -j DROP 2>/dev/null; then
  ok "FORWARD DROP rule is ACTIVE — VPS IP is protected"
else
  warn "FORWARD DROP rule not found. Check: journalctl -u neox-guard.service"
fi

# ─── Done ─────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}${GREEN}┌─────────────────────────────────────────────────────┐${RESET}"
echo -e "${BOLD}${GREEN}│  ✔ NeoxAgent setup complete!                        │${RESET}"
echo -e "${BOLD}${GREEN}└─────────────────────────────────────────────────────┘${RESET}"
echo ""
echo -e "  ${BOLD}Agent URL:${RESET}  http://$(hostname -I | awk '{print $1}'):${FINAL_PORT}"
echo -e "  ${BOLD}API Key:${RESET}    ${YELLOW}${FINAL_API_KEY}${RESET}"
echo -e "  ${BOLD}Config:${RESET}     ${CONFIG_FILE}"
echo ""
echo -e "  ${BOLD}Security:${RESET}"
echo -e "    ✔ neox-guard.service  — host FORWARD DROP (pre-Podman)"
echo -e "    ✔ NEOX_GUARD          — pod-level DROP-all gap protection"
echo -e "    ✔ HEV_FAILSAFE        — permanent kill-switch inside pod netns"
echo -e "    ✔ Watchdog wrapper    — reinstalls NEOX_GUARD on hev crash"
if [ ${#CORS_ENTRIES[@]:-0} -gt 0 ] 2>/dev/null; then
  echo -e "    ✔ CORS               — restricted to ${#CORS_ENTRIES[@]} origin(s)"
else
  echo -e "    ${YELLOW}⚠ CORS               — ALL origins allowed (set cors_origins in config.toml)${RESET}"
fi
echo ""
echo -e "  ${BOLD}Commands:${RESET}"
echo -e "    systemctl status neoxagent      # agent status"
echo -e "    systemctl status neox-guard     # guard status"
echo -e "    journalctl -fu neoxagent        # live logs"
echo -e "    bash scripts/setup.sh --update  # update to latest"
echo ""
