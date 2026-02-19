# Resumen del Proyecto: neoxagent

## ¿Qué es neoxagent?
**neoxagent** es un agente de nodo escrito en **Rust** diseñado para interactuar de forma nativa con **Podman**. Su objetivo principal es actuar como un reemplazo ligero, eficiente y seguro para *Pterodactyl Wings*, diseñado específicamente para integrarse con el panel *Jexactyl* (basado en Next.js).

El agente está diseñado para ser **Daemonless** (sin procesos en segundo plano consumiendo memoria pasivamente) y **Rootless** (más seguro por diseño), administrando así servidores de juegos, aplicaciones en contenedores, redes con *tun2socks* (a través de Pods) y despliegues mediante *Kubernetes YAML*.

## Características Principales
*   **Podman Nativo:** Utiliza la SDK nativa (`podman-api`) para comunicarse con Podman, eliminando la necesidad de Docker y su daemon (`dockerd`).
*   **Gestión de Pods y tun2socks:** Permite la creación de Pods que encapsulan contenedores (ej. un servidor de Minecraft junto a un sidecar de tun2socks) para compartir la misma red e IP, aislando y protegiendo el tráfico.
*   **Seguro y Ligero:** Al usar Rust y Podman rootless, el consumo de memoria en reposo es mínimo (~3-5MB RAM), proporcionando alta seguridad perimetral para los contenedores.
*   **Soporte Kubernetes YAML:** Utiliza la compatibilidad nativa de Podman con `play kube` para levantar arquitecturas multicontenedor con archivos YAML, en lugar de depender de Docker Compose.

## Stack Tecnológico
*   **Lenguaje:** Rust.
*   **API y WebSockets:** `axum` y `tokio-tungstenite`.
*   **Runtime Asíncrono:** `tokio`.
*   **Comunicación con Contenedores:** `podman-api`.
*   **Serialización y Configuración:** `serde`, `serde_json`, `serde_yaml`, y `toml`.

## Estado Actual (Según Documentación)
El proyecto ha completado formalmente la **Fase 1** y tiene un Roadmap definido de desarrollo:

*   ✅ **Fase 1 completada (API REST Base):**
    *   Autenticación mediante *Bearer Token* (API Key configurada en `config.toml`).
    *   Endpoints de estado de salud del proyecto y métricas de memoria/host (`/api/system/info`, `/api/system/resources`).
    *   Operaciones CRUD de contenedores (listar, crear con mapeo de puertos/volúmenes/límites, inspeccionar, eliminar).
    *   Ciclo de vida de contenedores (start, stop, restart, kill).

*   **Roadmap Futuro (Fases 2 a 7):**
    *   *Fase 2:* Consola interactiva, visualización de logs y métricas (CPU/RAM) en tiempo real mediante WebSockets.
    *   *Fase 3:* Soporte para creación y gestión de Pods completos junto con proxy *tun2socks*.
    *   *Fase 4:* Despliegue de stacks complejos desde archivos Kubernetes YAML.
    *   *Fase 5:* File Manager completo (listar, leer, descargar, subir, permisos).
    *   *Fase 6:* Sistema de Backups (compresión, guardado, checksum, restauración).
    *   *Fase 7:* Gestión de imágenes de contenedores e integración con *Systemd* para generar servicios automáticamente y proveer auto-arranque en Linux.
