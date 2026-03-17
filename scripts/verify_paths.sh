#!/bin/bash
# Moved from root — verifies required paths and binaries exist on the VPS.
# Usage: bash scripts/verify_paths.sh

set -euo pipefail

ok()   { echo -e "\e[32m[OK]\e[0m   $*"; }
miss() { echo -e "\e[31m[MISS]\e[0m $*"; }

check() {
  if [ -e "$1" ]; then ok "$1"; else miss "$1"; fi
}

check /usr/local/bin/neoxagent
check /opt/neoxagent/config.toml
check /etc/systemd/system/neoxagent.service
check /etc/systemd/system/neox-guard.service
check /usr/local/bin/hev-socks5-tproxy
check /sbin/iptables

echo ""
systemctl is-active neoxagent   && ok "neoxagent running"    || miss "neoxagent not running"
systemctl is-active neox-guard  && ok "neox-guard running"   || miss "neox-guard not running"
iptables -C FORWARD -m comment --comment "neox-guard-forward-drop" -j DROP 2>/dev/null \
  && ok "FORWARD DROP rule active" || miss "FORWARD DROP rule NOT active"
