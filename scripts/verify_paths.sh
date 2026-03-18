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
check /usr/local/bin/hev-socks5-tproxy
check /sbin/iptables

echo ""
systemctl is-active neoxagent   && ok "neoxagent running"    || miss "neoxagent not running"

