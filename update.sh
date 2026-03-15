#!/bin/bash

# Script de actualización automática para NeoxAgent
# Realiza pull, compilación, despliegue del binario y reinicio del servicio.

set -e # Detener el script si algo falla

echo "🔄 Iniciando actualización de NeoxAgent..."

# 1. Obtener últimos cambios de GitHub
echo "📡 Descargando cambios desde GitHub..."
git pull origin master

# 2. Compilar la versión de producción
echo "🛠️ Compilando el proyecto (esto puede tardar unos minutos)..."
cargo build --release

# 3. Detener el servicio para desbloquear el archivo
echo "⏹️ Deteniendo servicio neoxagent..."
systemctl stop neoxagent || true

# 4. Desplegar el nuevo binario
echo "🚀 Copiando binario a /usr/local/bin/..."
cp target/release/neoxagent /usr/local/bin/neoxagent

# 5. Volver a arrancar
echo "▶️ Iniciando servicio neoxagent..."
systemctl start neoxagent

# 6. Verificación final
echo "✅ ¡Actualización completada con éxito!"
systemctl status neoxagent --no-pager | grep "Active:"
