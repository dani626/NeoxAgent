#!/bin/bash
# =============================================================================
# NeoxAgent — External VPS Integration and Lifecycle Tester
# Verifies that an external NeoxAgent instance is fully operational from outside.
#
# Usage:
#   bash scripts/test_vps.sh -k "your-api-key" -h "vps-ip" [-p 8443] [-s] [-i]
# =============================================================================
set -euo pipefail

# ─── Colors ───────────────────────────────────────────────────────────────────
RED='\e[0;31m'; GREEN='\e[0;32m'; YELLOW='\e[1;33m'
BLUE='\e[0;34m'; CYAN='\e[0;36m'; BOLD='\e[1m'; RESET='\e[0m'

info() { echo -e "${BLUE}[INFO]${RESET} $*"; }
ok()   { echo -e "${GREEN}[OK]${RESET}   $*"; }
warn() { echo -e "${YELLOW}[WARN]${RESET} $*"; }
err()  { echo -e "${RED}[ERR]${RESET}  $*" >&2; }

# ─── Prerequisites ────────────────────────────────────────────────────────────
if ! command -v curl &>/dev/null; then
  err "This script requires 'curl' to make HTTP requests."
  exit 1
fi

if ! command -v jq &>/dev/null; then
  err "This script requires 'jq' to parse API JSON responses."
  exit 1
fi

# ─── Defaults ─────────────────────────────────────────────────────────────────
API_KEY=""
HOST=""
PORT="8443"
PROTOCOL="http"
INSECURE="false"

# ─── Help ─────────────────────────────────────────────────────────────────────
usage() {
  echo -e "${BOLD}Uso:${RESET}"
  echo -e "  $0 -k <api_key> -h <host> [-p <port>] [-s] [-i]"
  echo -e ""
  echo -e "${BOLD}Opciones:${RESET}"
  echo -e "  -k    API Key de NeoxAgent (Requerido)"
  echo -e "  -h    Dirección IP o Host del VPS externo (Requerido)"
  echo -e "  -p    Puerto del agente (Por defecto: 8443)"
  echo -e "  -s    Usar HTTPS en lugar de HTTP"
  echo -e "  -i    Ignorar verificación de certificados TLS (Inseguro / Autofirmados)"
  exit 1
}

# ─── Parse Arguments ──────────────────────────────────────────────────────────
while getopts "k:h:p:si" opt; do
  case "$opt" in
    k) API_KEY="$OPTARG" ;;
    h) HOST="$OPTARG" ;;
    p) PORT="$OPTARG" ;;
    s) PROTOCOL="https" ;;
    i) INSECURE="true" ;;
    *) usage ;;
  esac
done

if [ -z "$API_KEY" ] || [ -z "$HOST" ]; then
  err "Error: Faltan argumentos requeridos (-k y -h)."
  usage
fi

BASE_URL="${PROTOCOL}://${HOST}:${PORT}"

# ─── Build Curl Command ───────────────────────────────────────────────────────
CURL_FLAGS=(-sf)
if [ "$INSECURE" = "true" ]; then
  CURL_FLAGS+=(-k)
fi

HEADERS=(-H "Authorization: Bearer ${API_KEY}" -H "Content-Type: application/json")

# ─── Start Verification ───────────────────────────────────────────────────────
echo -e ""
echo -e "${BOLD}${CYAN}=== Probando NeoxAgent en ${BASE_URL} ===${RESET}"
echo -e ""

# --- Test 1: Health Check (Public Endpoint) ---
info "1/6 — Comprobando endpoint de salud (/api/health)..."
HEALTH_RES=$(curl "${CURL_FLAGS[@]}" "${BASE_URL}/api/health")
STATUS=$(echo "$HEALTH_RES" | jq -r '.status')

if [ "$STATUS" = "ok" ]; then
  VERSION=$(echo "$HEALTH_RES" | jq -r '.version')
  PODMAN_VER=$(echo "$HEALTH_RES" | jq -r '.podman_version')
  ok "Salud del Agente: OK (Versión: $VERSION, Podman: $PODMAN_VER)"
else
  err "Salud del Agente: Error en la respuesta."
  exit 1
fi

# --- Test 2: System Info (Authenticated) ---
info "2/6 — Obteniendo información detallada del sistema (/api/system/info)..."
SYS_INFO=$(curl "${CURL_FLAGS[@]}" "${BASE_URL}/api/system/info" "${HEADERS[@]}")
HOSTNAME=$(echo "$SYS_INFO" | jq -r '.hostname')
OS=$(echo "$SYS_INFO" | jq -r '.os')
ARCH=$(echo "$SYS_INFO" | jq -r '.arch')
CORES=$(echo "$SYS_INFO" | jq -r '.cpu_cores')
RAM=$(echo "$SYS_INFO" | jq -r '.memory_total_mb')
ROOTLESS=$(echo "$SYS_INFO" | jq -r '.rootless')

ok "Sistema: $OS ($ARCH) | Host: $HOSTNAME"
ok "Recursos: CPU Cores: $CORES | Memoria Total: ${RAM} MB"
ok "Modo Podman Rootless: $ROOTLESS"

# --- Test 3: System Resources Telemetry ---
info "3/6 — Obteniendo telemetría en tiempo real (/api/system/resources)..."
SYS_RES=$(curl "${CURL_FLAGS[@]}" "${BASE_URL}/api/system/resources" "${HEADERS[@]}")
RAM_USED=$(echo "$SYS_RES" | jq -r '.memory_used_mb')
DISK_USED=$(echo "$SYS_RES" | jq -r '.disk_used_gb | tonumber | round')
DISK_TOTAL=$(echo "$SYS_RES" | jq -r '.disk_total_gb | tonumber | round')

ok "Uso actual de RAM: ${RAM_USED} MB / ${RAM} MB"
ok "Uso de Disco: ${DISK_USED} GB / ${DISK_TOTAL} GB"

# --- Test 4: Volumes API CRUD ---
info "4/6 — Probando API de Volúmenes (Crear -> Inspeccionar -> Eliminar)..."
VOL_NAME="test-vps-vol-$(date +%s)"

# Create
CREATE_VOL_RES=$(curl "${CURL_FLAGS[@]}" -X POST "${BASE_URL}/api/volumes" "${HEADERS[@]}" \
  -d "{\"name\": \"$VOL_NAME\"}")
ok "Volumen creado: $(echo "$CREATE_VOL_RES" | jq -r '.name')"

# Inspect
INSPECT_VOL_RES=$(curl "${CURL_FLAGS[@]}" "${BASE_URL}/api/volumes/$VOL_NAME" "${HEADERS[@]}")
ok "Volumen inspeccionado: $(echo "$INSPECT_VOL_RES" | jq -r '.name') (Driver: $(echo "$INSPECT_VOL_RES" | jq -r '.driver'))"

# Delete
DELETE_VOL_RES=$(curl "${CURL_FLAGS[@]}" -X DELETE "${BASE_URL}/api/volumes/$VOL_NAME" "${HEADERS[@]}")
ok "Volumen eliminado con éxito: $(echo "$DELETE_VOL_RES" | jq -r '.message')"

# --- Test 5: Networks API CRUD ---
info "5/6 — Probando API de Redes Netavark (Crear -> Inspeccionar -> Eliminar)..."
NET_NAME="test-vps-net-$(date +%s)"

# Create
CREATE_NET_RES=$(curl "${CURL_FLAGS[@]}" -X POST "${BASE_URL}/api/networks" "${HEADERS[@]}" \
  -d "{\"name\": \"$NET_NAME\", \"dns_enabled\": true, \"ipv6_enabled\": false, \"internal\": false}")
NET_ID=$(echo "$CREATE_NET_RES" | jq -r '.id')
ok "Red creada: $NET_NAME (ID: $NET_ID)"

# Inspect
INSPECT_NET_RES=$(curl "${CURL_FLAGS[@]}" "${BASE_URL}/api/networks/$NET_ID" "${HEADERS[@]}")
ok "Red inspeccionada: $(echo "$INSPECT_NET_RES" | jq -r '.name') (DNS: $(echo "$INSPECT_NET_RES" | jq -r '.dns_enabled'))"

# Delete
DELETE_NET_RES=$(curl "${CURL_FLAGS[@]}" -X DELETE "${BASE_URL}/api/networks/$NET_ID" "${HEADERS[@]}")
ok "Red eliminada con éxito: $(echo "$DELETE_NET_RES" | jq -r '.message')"

# --- Test 6: Pod Lifecycle & Containment ---
info "6/6 — Probando el ciclo de vida de un Pod con contenedores (Crear -> Detener -> Eliminar)..."
POD_NAME="test-vps-pod-$(date +%s)"

# Create Pod
CREATE_POD_RES=$(curl "${CURL_FLAGS[@]}" -X POST "${BASE_URL}/api/pods" "${HEADERS[@]}" -d "{
  \"name\": \"$POD_NAME\",
  \"containers\": [{
    \"name\": \"alpine-test-ctr\",
    \"image\": \"alpine:latest\",
    \"command\": [\"sleep\", \"3600\"]
  }]
}")
POD_ID=$(echo "$CREATE_POD_RES" | jq -r '.id')
ok "Pod creado con éxito. ID asignado: $POD_ID"

# Inspect Pod status
INSPECT_POD_RES=$(curl "${CURL_FLAGS[@]}" "${BASE_URL}/api/pods/$POD_ID" "${HEADERS[@]}")
ok "Estado del Pod: $(echo "$INSPECT_POD_RES" | jq -r '.status')"

# Stop Pod
info "Deteniendo el Pod..."
curl "${CURL_FLAGS[@]}" -X POST "${BASE_URL}/api/pods/$POD_ID/stop" "${HEADERS[@]}" > /dev/null
ok "Pod detenido"

# Delete Pod (force)
info "Eliminando el Pod..."
curl "${CURL_FLAGS[@]}" -X DELETE "${BASE_URL}/api/pods/$POD_ID?force=true" "${HEADERS[@]}" > /dev/null
ok "Pod eliminado"

echo -e ""
echo -e "${BOLD}${GREEN}┌─────────────────────────────────────────────────────┐${RESET}"
echo -e "${BOLD}${GREEN}│  ✔ Todas las pruebas de integración en VPS pasaron! │${RESET}"
echo -e "${BOLD}${GREEN}└─────────────────────────────────────────────────────┘${RESET}"
echo -e ""
