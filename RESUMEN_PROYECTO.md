# Resumen del Proyecto: neoxagent

**neoxagent** es un agente de nodo ligero y seguro escrito en **Rust** para la administración nativa de contenedores y pods a través de **Podman**, diseñado como reemplazo para *Pterodactyl Wings* para integrarse con el panel *Jexactyl* (Next.js).

Toda la documentación detallada del proyecto, incluyendo su stack tecnológico, la descripción minuciosa de cada característica de la API, las características desactivadas y la comparativa de características faltantes respecto a Pterodactyl Wings se encuentra disponible en el [README.md](file:///c:/proyectos/NeoxAgent/README.md).

---

## 📋 Resumen Rápido de Estado

### Características Activas:
*   ✅ **Endpoints de Estado y Host:** Información del host y consumo de recursos (CPU/RAM/Almacenamiento).
*   ✅ **CRUD y Ciclo de Vida de Contenedores:** Listado, inspección, creación, eliminación y comandos de energía.
*   ✅ **WebSockets en Tiempo Real:** Visualización de logs en vivo, consolas interactivas (exec/attach) y telemetría de consumo.
*   ✅ **Administración de Archivos:** API de ficheros con protección contra Path Traversal, subidas y descargas en `.tar.gz`.
*   ✅ **Sistema de Respaldos:** Creación de respaldos locales `.tar.gz`, cálculo de hashes SHA256 y rotación de copias obsoletas.
*   ✅ **Gestión de Imágenes:** Descarga (`pull`) de imágenes con reporte de progreso y búsqueda en registros públicos.
*   ✅ **Integración de Systemd:** Generación y habilitación de unidades service persistentes a nivel rootless y root.
*   ✅ **Cuotas de Disco Activas:** Soporte integrado para límites de espacio a nivel de kernel (cuotas de proyecto de ext4 y XFS) para cada volumen asignado.

### 🚫 Desactivado Temporalmente:
*   **Proxy de Red (Tun2socks):** Lógica sidecar para ruteo de tráfico a través de SOCKS5 en Pods.
*   **Pilas Multicontenedor (Kubernetes YAML):** Despliegue directo mediante `podman play kube`.

### ⚠️ Características Faltantes respecto a Pterodactyl Wings:
1.  **Servidor SFTP Incorporado:** Falta de un daemon SFTP nativo en el puerto 2022 para clientes como FileZilla.
2.  **Watchdog Local:** No hay un hilo local que monitoree caídas y controle bucles de reinicio del servidor.
3.  **Ejecución Autónoma de Tareas (Schedules):** La ejecución de tareas cron programadas depende del panel externo.
4.  **Motor Egg/Nest Parser:** Sin capacidad de edición e inyección dinámica avanzada en archivos de configuración de juegos.
5.  **Historial de Estadísticas de Consumo:** Las métricas de consumo de CPU/RAM son efímeras y no se guardan en el nodo.
6.  **SSL Auto-Configuration (ACME):** La generación y renovación de TLS requiere configuración externa.
7.  **Autenticación en Registros Privados:** No soporta pull de imágenes privadas que requieran autenticación por servidor.
8.  **Estado de Suspensión:** No hay mecanismo integrado para suspender y bloquear físicamente servidores desactivados.
