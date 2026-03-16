#!/bin/bash

# ============================================================
#  Script de actualización: NeoxAgent — rama gpt-5.4
#  Correcciones aplicadas por GPT-5.4 vía Perplexity AI
# ============================================================

set -e

BRANCH="gpt-5.4"

echo ""
echo "╔══════════════════════════════════════════════════╗"
echo "║       NeoxAgent — Updater GPT-5.4               ║"
echo "║  Branch: $BRANCH                                ║"
echo "║  Fixes:  Volume types + builder.options()       ║"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# 1. Cambiar a la rama correcta
echo "🔀 Cambiando a rama '$BRANCH'..."
git fetch origin
git checkout $BRANCH
git pull origin $BRANCH

# 2. Compilar
echo "🛠️  Compilando (rama: $BRANCH)..."
cargo build --release

# 3. Detener servicio
echo "⏹️  Deteniendo servicio neoxagent..."
systemctl stop neoxagent || true

# 4. Desplegar binario
echo "🚀 Copiando binario a /usr/local/bin/..."
cp target/release/neoxagent /usr/local/bin/neoxagent

# 5. Reiniciar servicio
echo "▶️  Iniciando servicio neoxagent..."
systemctl start neoxagent

# 6. Estado final
echo ""
echo "✅ ¡Actualización completada — rama $BRANCH activa!"
echo "   Cambios aplicados por GPT-5.4 via Perplexity AI"
echo ""
systemctl status neoxagent --no-pager | grep "Active:"
