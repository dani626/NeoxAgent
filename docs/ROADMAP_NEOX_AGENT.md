# 🦀 neoxagent — Roadmap de Desarrollo

> **Agente de nodo escrito en Rust con Podman Nativo**
> Reemplazo ligero y seguro de Pterodactyl Wings para el panel Jexactyl Next.js
> Soporte para: Servidores de juegos, apps containerizadas, redes tun2socks (via Pods), Kubernetes YAML
> **Daemonless, Rootless, Seguro por diseño**

---

## 📐 Arquitectura General

```
┌─────────────────────────────────────────────────────┐
│            Panel Jexactyl (Next.js)                 │
│  ├── Frontend (React/Dashboard)                     │
│  ├── Backend (API Routes /api/nodes/*, /api/pods)   │
│  └── Base de Datos (MySQL/PostgreSQL)               │
│            │                                        │
│     HTTPS + API Key + JSON                          │
│            │                                        │
└────────────┼────────────────────────────────────────┘
             │
     ┌───────┴───────┐
     ▼               ▼
┌──────────┐   ┌──────────┐
│  Nodo 1  │   │  Nodo 2  │   ...N nodos
│──────────│   │──────────│
│ neoxagent│   │ neoxagent│   (Rust Binary ~3MB, ~3-5MB RAM)
│  (Rust)  │   │  (Rust)  │
│    │     │   │    │     │
│podman-api│   │podman-api│   (SDK Nativo de Podman)
│    │     │   │    │     │
│ Podman   │   │ Podman   │   (Rootless, Daemonless)
│ Engine   │   │ Engine   │
│  ┌─────┐ │   │  ┌─────┐ │
│  │Pod 1│ │   │  │Pod 2│ │
│  │ 🔒🎮│ │   │  │ 🌐🤖│ │
│  └─────┘ │   │  └─────┘ │
│  ┌─────┐ │   │  ┌─────┐ │
│  │Pod 2│ │   │  │Pod 3│ │
│  │ 🔒🎮│ │   │  │ 🎮  │ │
│  └─────┘ │   │  └─────┘ │
└──────────┘   └──────────┘
```

### ¿Por qué Podman en lugar de Docker?

| Aspecto           | Docker                         | Podman (Nuestra elección)     |
| :---------------- | :----------------------------- | :---------------------------- |
| Daemon            | Sí (`dockerd` siempre activo)  | **No** (daemonless)           |
| Seguridad         | Root por defecto               | **Rootless** por defecto      |
| Si crashea...     | Todos los contenedores mueren  | **Solo el afectado**          |
| Ram idle          | ~50-100MB (daemon)             | **~0MB** (sin daemon)         |
| Pods nativos      | No                             | **Sí** (como Kubernetes)      |
| Networking        | iptables (legacy)              | **Netavark** (Rust, moderno)  |
| Systemd           | Parcial                        | **Nativo** (auto-generate)    |
| Kubernetes compat | No nativo                      | **`podman play kube`**        |

### ¿Por qué Pods son clave para tun2socks?

**Problema con Docker tradicional:**
```
Contenedor Tun2socks (red propia)
     ↕ network_mode: container:xxx (frágil, manual, crashea)
Contenedor Minecraft
```

**Solución con Pods de Podman:**
```
┌─── Pod "minecraft-proxy" ────────────────┐
│  Comparten: localhost, IP, red, puertos  │
│                                          │
│  🔒 tun2socks (sidecar)                 │
│  🎮 minecraft-server (main)             │
│                                          │
│  Todo el tráfico de Minecraft            │
│  sale por tun2socks automáticamente      │
└──────────────────────────────────────────┘
```
- Si reinicias el Pod → ambos se reinician ordenadamente
- Comparten `localhost` → Minecraft ve a tun2socks en `127.0.0.1`
- Un solo bloque lógico → Fácil de gestionar desde el panel

---

## 🗂️ Stack Tecnológico del Agente

| Componente        | Librería Rust              | Propósito                          |
| :---------------- | :------------------------- | :--------------------------------- |
| **Podman SDK**    | `podman-api`               | Comunicación nativa con Podman     |
| HTTP Server       | `axum`                     | API REST del agente                |
| WebSocket         | `axum` + `tokio-tungstenite` | Console y Logs en tiempo real    |
| Async Runtime     | `tokio`                    | Concurrencia asíncrona             |
| Serialización     | `serde` + `serde_json`     | Parse de JSON (requests/responses) |
| Autenticación     | Custom (API Key/JWT)       | Seguridad de las peticiones        |
| Logging           | `tracing` + `tracing-subscriber` | Logs estructurados          |
| Config            | `toml` + `serde`           | Archivo de configuración           |
| File I/O          | `tokio::fs`                | File manager de volúmenes          |
| Compression       | `tar` + `flate2`           | Backups de volúmenes               |
| TLS (opcional)    | `rustls` + `axum-server`   | HTTPS nativo                       |
| Kube YAML         | `serde_yaml`               | Parsing de Kubernetes YAML (Pods)  |

---

## 📦 Estructura del Proyecto Rust

```
neoxagent/
├── Cargo.toml                  # Dependencias
├── config.toml                 # Configuración del nodo
├── src/
│   ├── main.rs                 # Entry point, inicializa Axum + Podman
│   ├── config.rs               # Lectura de config.toml
│   ├── auth.rs                 # Middleware de autenticación (API Key)
│   ├── error.rs                # Tipos de error unificados
│   ├── routes/
│   │   ├── mod.rs              # Router principal
│   │   ├── system.rs           # GET /health, GET /system/info
│   │   ├── containers.rs       # CRUD de contenedores individuales
│   │   ├── pods.rs             # CRUD de Pods (tun2socks + game server)
│   │   ├── kube.rs             # Deploy Kubernetes YAML (podman play kube)
│   │   ├── networks.rs         # CRUD de redes Podman (Netavark)
│   │   ├── volumes.rs          # Gestión de volúmenes
│   │   ├── images.rs           # Pull/List/Remove imágenes
│   │   ├── files.rs            # File manager (listar, leer, escribir, subir)
│   │   ├── backups.rs          # Crear/restaurar/descargar backups
│   │   ├── systemd.rs          # Generar/gestionar systemd units
│   │   └── ws.rs               # WebSocket: logs stream + console interactiva
│   ├── services/
│   │   ├── mod.rs
│   │   ├── podman.rs           # Wrapper sobre podman-api (singleton)
│   │   ├── stats.rs            # Collector de CPU/RAM/Disk en tiempo real
│   │   ├── backup.rs           # Lógica de compresión/restauración
│   │   └── systemd_gen.rs      # Generador de .service files
│   └── models/
│       ├── mod.rs
│       ├── container.rs        # Request/Response types para containers
│       ├── pod.rs              # Types para Pods
│       ├── network.rs          # Types para redes
│       └── server.rs           # Modelo de "Server" con su estado
├── templates/
│   ├── pod-proxy.yaml          # Template de Pod con tun2socks sidecar
│   └── pod-standalone.yaml     # Template de Pod sin proxy
├── scripts/
│   └── install.sh              # Script de instalación automática en el nodo
└── Dockerfile                  # Para compilar el agente (multi-stage build)
```

---

## 🗺️ ROADMAP — Fases de Desarrollo

---

### ═══════════════════════════════════════════════
### FASE 0: Setup del Proyecto (Día 1)
### ═══════════════════════════════════════════════

**Objetivo:** Proyecto Rust compilando y conectado a Podman.

#### Paso 0.1 — Inicializar proyecto Rust
```bash
cargo init neoxagent
cd neoxagent
```

#### Paso 0.2 — Agregar dependencias en `Cargo.toml`
```toml
[package]
name = "neoxagent"
version = "0.1.0"
edition = "2021"

[dependencies]
# Podman (SDK Nativo)
podman-api = "0.10"

# Web Server
axum = { version = "0.7", features = ["ws", "multipart"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# Async
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = "0.23"
futures-util = "0.3"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
toml = "0.8"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Utils
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }

# Compression (backups)
tar = "0.4"
flate2 = "1"

# TLS
rustls = "0.23"

# Process (para podman CLI fallback y compose)
tokio-process = "0.2"
```

#### Paso 0.3 — Prerequisitos en el Nodo Linux
```bash
# Instalar Podman
sudo apt install podman
# o en RHEL/Fedora:
sudo dnf install podman

# Habilitar el socket de Podman (rootless, para tu usuario)
systemctl --user enable --now podman.socket

# Verificar que el socket existe
ls /run/user/$(id -u)/podman/podman.sock

# Permitir puertos bajos (para game servers si necesitas < 1024)
sudo sysctl -w net.ipv4.ip_unprivileged_port_start=80
echo "net.ipv4.ip_unprivileged_port_start=80" | sudo tee -a /etc/sysctl.conf

# Habilitar linger (para que los contenedores sobrevivan al logout del usuario)
sudo loginctl enable-linger $(whoami)
```

#### Paso 0.4 — Verificar conexión con Podman
```rust
use podman_api::Podman;

#[tokio::main]
async fn main() {
    // Conectar al socket rootless
    let podman = Podman::unix("/run/user/1000/podman/podman.sock");
    
    let info = podman.info().await.unwrap();
    println!("Podman v{}", info.version.unwrap().version.unwrap());
    println!("Contenedores: {}", info.store.unwrap().container_store.unwrap().number.unwrap());
    println!("Host: {}", info.host.unwrap().hostname.unwrap());
}
```

**Criterio de éxito:** `cargo run` muestra la versión de Podman y el hostname del nodo.

---

### ═══════════════════════════════════════════════
### FASE 1: API REST Base (Días 2-4)
### ═══════════════════════════════════════════════

**Objetivo:** API funcional con autenticación para crear/listar/eliminar contenedores.

#### Paso 1.1 — Configuración (`config.toml`)
```toml
[agent]
host = "0.0.0.0"
port = 8443
api_key = "tu-clave-secreta-super-larga-aqui"
data_dir = "/var/lib/neoxagent"

[podman]
# Socket rootless (ajustar UID si es necesario)
socket = "/run/user/1000/podman/podman.sock"
# Directorio base para volúmenes de servidores
volumes_dir = "/home/neox/servers"

[tls]
enabled = false
cert_path = ""
key_path = ""

[defaults]
restart_policy = "always"
dns = ["1.1.1.1", "8.8.8.8"]
```

#### Paso 1.2 — Middleware de Autenticación (`auth.rs`)
- Leer header `Authorization: Bearer <API_KEY>`
- Comparar con la key del `config.toml`
- Rechazar con `401 Unauthorized` si no coincide
- Excluir `/api/health` de la autenticación (para health checks)

#### Paso 1.3 — Endpoints de Sistema
```
GET  /api/health              → { "status": "ok", "version": "0.1.0", "podman_version": "5.x" }
GET  /api/system/info         → { "os": "linux", "arch": "x86_64", "podman_version": "...",
                                  "cpu_cores": 8, "memory_total_mb": 16384, 
                                  "disk_total_gb": 500, "rootless": true,
                                  "cgroup_version": "v2" }
GET  /api/system/resources    → { "cpu_used_percent": 23.5, "memory_used_mb": 8192,
                                  "memory_free_mb": 8192, "disk_used_gb": 120 }
```

#### Paso 1.4 — Endpoints CRUD de Contenedores
```
GET    /api/containers                → Lista todos los contenedores
POST   /api/containers               → Crear contenedor individual
GET    /api/containers/:id            → Detalle de un contenedor
DELETE /api/containers/:id            → Eliminar contenedor (+ opción: eliminar volumen)
POST   /api/containers/:id/start     → Iniciar
POST   /api/containers/:id/stop      → Detener (graceful, con timeout)
POST   /api/containers/:id/restart   → Reiniciar
POST   /api/containers/:id/kill      → Forzar detención (SIGKILL)
```

#### Paso 1.5 — Request Body para Crear Contenedor
```json
POST /api/containers
{
    "name": "minecraft-survival",
    "image": "docker.io/itzg/minecraft-server:latest",
    "env": {
        "EULA": "TRUE",
        "TYPE": "PAPER",
        "VERSION": "1.21.4",
        "MEMORY": "2G"
    },
    "ports": [
        { "host": 25565, "container": 25565, "protocol": "tcp" }
    ],
    "limits": {
        "memory_mb": 2048,
        "cpu_cores": 2.0,
        "disk_mb": 10240
    },
    "volumes": [
        { "host_path": "/home/neox/servers/minecraft-survival", "container_path": "/data" }
    ],
    "network": "default",
    "restart_policy": "always",
    "labels": {
        "neox.managed": "true",
        "neox.server.type": "minecraft",
        "neox.owner": "user-uuid-here"
    }
}
```

#### Paso 1.6 — Response estándar
```json
{
    "id": "a1b2c3d4e5f6",
    "name": "minecraft-survival",
    "image": "docker.io/itzg/minecraft-server:latest",
    "status": "running",
    "created_at": "2026-02-17T18:00:00Z",
    "ports": [{ "host": 25565, "container": 25565, "protocol": "tcp" }],
    "limits": { "memory_mb": 2048, "cpu_cores": 2.0 },
    "labels": { "neox.managed": "true" }
}
```

**Criterio de éxito:** Crear un servidor Minecraft con `curl` y verlo corriendo con `podman ps`.

---

### ═══════════════════════════════════════════════
### FASE 2: Logs, Console y Stats en Tiempo Real (Días 5-7)
### ═══════════════════════════════════════════════

**Objetivo:** El panel puede ver logs en vivo, enviar comandos y ver stats de CPU/RAM.

#### Paso 2.1 — Endpoint de Logs (HTTP estático)
```
GET /api/containers/:id/logs?tail=100&timestamps=true
→ Últimas 100 líneas de logs (texto plano con timestamps)
```

#### Paso 2.2 — WebSocket de Logs (Streaming en vivo)
```
WS /api/containers/:id/logs/stream
```
- Al conectarse, llama a `container.logs()` con `follow: true`
- Cada línea nueva se envía por el WebSocket al panel
- El panel las renderiza en una terminal (`xterm.js`)
- Soportar filtros: `stdout`, `stderr`, timestamps

#### Paso 2.3 — WebSocket de Console Interactiva
```
WS /api/containers/:id/console
```
- Flujo bidireccional:
  - **← Server → Panel:** Output del proceso principal (stdout/stderr)
  - **→ Panel → Server:** Comandos del usuario (ej: `/op player`, `/stop`)
- Para **Minecraft:** usar `container.attach()` directamente al PID 1
  (la consola de Minecraft no es un shell, es stdin del proceso Java)
- Para **shells genéricos:** usar `container.exec()` con `/bin/sh`
- Detectar tipo por label `neox.server.type`

#### Paso 2.4 — WebSocket de Stats en Tiempo Real
```
WS /api/containers/:id/stats
```
- Usa `container.stats()` con stream
- Envía cada ~1 segundo:
```json
{
    "timestamp": "2026-02-17T18:30:00Z",
    "cpu_percent": 45.2,
    "memory_used_mb": 1823,
    "memory_limit_mb": 2048,
    "memory_percent": 89.0,
    "network": {
        "rx_bytes": 1048576,
        "tx_bytes": 524288
    },
    "disk": {
        "read_bytes": 0,
        "write_bytes": 4096
    },
    "pids": 42
}
```

**Criterio de éxito:** Ver logs de Minecraft en tiempo real via `wscat` o un cliente WebSocket.

---

### ═══════════════════════════════════════════════
### FASE 3: PODS + Tun2socks (Días 8-11)
### ═══════════════════════════════════════════════

**Objetivo:** Crear Pods que encapsulen tun2socks + servidor de juego como unidad atómica.

#### Paso 3.1 — ¿Qué es un Pod en Podman?
Un Pod es un grupo de contenedores que comparten:
- La misma interfaz de red (misma IP, mismo `localhost`)
- El mismo namespace de procesos (opcional)
- El mismo ciclo de vida (start/stop/restart juntos)

Es idéntico al concepto de Pod en Kubernetes.

```
┌─── Pod "mc-proxy-1" ─────────────────────────┐
│                                               │
│  IP: 10.88.0.5                                │
│  Puertos expuestos: 25565                     │
│                                               │
│  ┌─────────────────┐  ┌────────────────────┐  │
│  │  tun2socks      │  │  minecraft-server  │  │
│  │  (sidecar)      │  │  (main)            │  │
│  │                 │  │                    │  │
│  │  Redirige todo  │  │  Cree que sale     │  │
│  │  el tráfico por │  │  a internet        │  │
│  │  SOCKS5 proxy   │  │  normalmente       │  │
│  └─────────────────┘  └────────────────────┘  │
│                                               │
│  El tráfico de Minecraft sale por el proxy    │
│  SOCKS5 de forma transparente                 │
└───────────────────────────────────────────────┘
```

#### Paso 3.2 — Endpoints de Pods
```
GET    /api/pods                       → Listar todos los Pods
POST   /api/pods                       → Crear Pod (con o sin proxy sidecar)
GET    /api/pods/:id                   → Detalle del Pod (contenedores, estado, IP)
DELETE /api/pods/:id                   → Eliminar Pod + todos sus contenedores
POST   /api/pods/:id/start            → Iniciar Pod (todos los contenedores)
POST   /api/pods/:id/stop             → Detener Pod
POST   /api/pods/:id/restart          → Reiniciar Pod
GET    /api/pods/:id/containers        → Listar contenedores dentro del Pod
POST   /api/pods/:id/containers        → Agregar contenedor al Pod existente
```

#### Paso 3.3 — Request: Crear Pod con Proxy Tun2socks
```json
POST /api/pods
{
    "name": "mc-proxy-1",
    "proxy": {
        "enabled": true,
        "type": "tun2socks",
        "image": "docker.io/xjasonlyu/tun2socks:latest",
        "socks5_url": "socks5://usuario:password@proxy-server.com:1080",
        "dns": "1.1.1.1"
    },
    "containers": [
        {
            "name": "minecraft",
            "image": "docker.io/itzg/minecraft-server:latest",
            "env": {
                "EULA": "TRUE",
                "TYPE": "PAPER",
                "VERSION": "1.21.4"
            },
            "ports": [
                { "host": 25565, "container": 25565, "protocol": "tcp" }
            ],
            "limits": {
                "memory_mb": 2048,
                "cpu_cores": 2.0
            },
            "volumes": [
                { "host_path": "/home/neox/servers/mc-proxy-1/data", "container_path": "/data" }
            ]
        }
    ],
    "labels": {
        "neox.managed": "true",
        "neox.owner": "user-uuid"
    }
}
```

#### Paso 3.4 — Lógica interna del Agente al crear Pod con Proxy
```
1. Crear Pod con nombre "mc-proxy-1" y puertos mapeados
   → podman.pods().create(pod_config)

2. Agregar sidecar tun2socks al Pod:
   → Crear contenedor con:
     - pod: "mc-proxy-1"
     - image: "xjasonlyu/tun2socks"
     - cap_add: ["NET_ADMIN"]
     - devices: ["/dev/net/tun"]
     - env: { PROXY: "socks5://...", LOGLEVEL: "info" }

3. Agregar contenedor principal (minecraft) al Pod:
   → Crear contenedor con:
     - pod: "mc-proxy-1"
     - image: "itzg/minecraft-server"
     - env, limits, volumes...

4. Iniciar el Pod:
   → pod.start()
   → Ambos contenedores arrancan, comparten red
   → Todo el tráfico de minecraft pasa por tun2socks
```

#### Paso 3.5 — Request: Crear Pod SIN proxy (servidor normal)
```json
POST /api/pods
{
    "name": "mc-vanilla",
    "proxy": {
        "enabled": false
    },
    "containers": [
        {
            "name": "minecraft",
            "image": "docker.io/itzg/minecraft-server:latest",
            "env": { "EULA": "TRUE" },
            "ports": [{ "host": 25566, "container": 25565 }],
            "limits": { "memory_mb": 4096, "cpu_cores": 4.0 },
            "volumes": [
                { "host_path": "/home/neox/servers/mc-vanilla/data", "container_path": "/data" }
            ]
        }
    ]
}
```

#### Paso 3.6 — Endpoints de Redes (complementario)
```
GET    /api/networks                   → Listar redes Podman (Netavark)
POST   /api/networks                   → Crear red custom
DELETE /api/networks/:id               → Eliminar red
GET    /api/networks/:id               → Detalle (subnet, gateway, containers conectados)
```

**Criterio de éxito:** Crear un Pod con tun2socks + Minecraft, verificar que la IP pública del servidor sea la del proxy SOCKS5.

---

### ═══════════════════════════════════════════════
### FASE 4: Kubernetes YAML Support (Días 12-14)
### ═══════════════════════════════════════════════

**Objetivo:** Desplegar stacks completos desde archivos Kubernetes YAML (reemplazo de Docker Compose).

#### Paso 4.1 — ¿Por qué Kubernetes YAML en vez de Docker Compose?
- Podman tiene soporte **nativo** para Kubernetes YAML (`podman play kube`)
- Es el estándar de la industria (portable a K8s real si escalas)
- Puedes exportar Pods existentes con `podman generate kube`
- Docker Compose NO es nativo de Podman (requiere `podman-compose`, incompleto)

#### Paso 4.2 — Endpoints
```
POST   /api/kube/deploy               → Subir YAML y desplegar Pod/Deployment
GET    /api/kube/stacks                → Listar stacks desplegados
POST   /api/kube/stacks/:name/up      → Levantar stack
POST   /api/kube/stacks/:name/down    → Detener y eliminar stack
DELETE /api/kube/stacks/:name          → Eliminar stack + volúmenes
GET    /api/kube/stacks/:name/status   → Estado de todos los containers del stack
POST   /api/kube/generate/:pod_id     → Exportar Pod existente a Kubernetes YAML
```

#### Paso 4.3 — Ejemplo: Kubernetes YAML con Tun2socks + Minecraft
```yaml
# Enviado desde el panel o subido como archivo
apiVersion: v1
kind: Pod
metadata:
  name: mc-proxy-1
  labels:
    neox.managed: "true"
    neox.owner: "user-uuid"
    neox.type: "minecraft"
spec:
  containers:
    # Sidecar: Proxy Tun2socks
    - name: tun2socks
      image: docker.io/xjasonlyu/tun2socks:latest
      env:
        - name: PROXY
          value: "socks5://user:pass@proxy.example.com:1080"
        - name: LOGLEVEL
          value: "info"
      securityContext:
        capabilities:
          add: ["NET_ADMIN"]
      resources:
        limits:
          memory: "128Mi"
          cpu: "0.5"
    
    # Principal: Minecraft Server
    - name: minecraft
      image: docker.io/itzg/minecraft-server:latest
      env:
        - name: EULA
          value: "TRUE"
        - name: TYPE
          value: "PAPER"
        - name: VERSION
          value: "1.21.4"
        - name: MEMORY
          value: "2G"
      ports:
        - containerPort: 25565
          hostPort: 25565
          protocol: TCP
      volumeMounts:
        - name: mc-data
          mountPath: /data
      resources:
        limits:
          memory: "2Gi"
          cpu: "2"

  volumes:
    - name: mc-data
      hostPath:
        path: /home/neox/servers/mc-proxy-1/data
        type: DirectoryOrCreate
```

#### Paso 4.4 — Flujo Interno
1. El panel envía el YAML al agente
2. El agente guarda en `/var/lib/neoxagent/stacks/{nombre}/pod.yaml`
3. Ejecuta `podman play kube pod.yaml` (via API o CLI fallback)
4. Retorna la lista de contenedores creados dentro del Pod
5. Los labels `neox.*` permiten al agente identificar qué Pods gestiona

#### Paso 4.5 — Exportar configuración existente
```
POST /api/kube/generate/mc-proxy-1
→ Genera el YAML del Pod "mc-proxy-1" tal como está corriendo
→ El usuario puede descargarlo, editarlo, y re-desplegarlo
```

**Criterio de éxito:** Desplegar un Pod desde YAML y verlo corriendo en el panel.

---

### ═══════════════════════════════════════════════
### FASE 5: File Manager (Días 15-18)
### ═══════════════════════════════════════════════

**Objetivo:** Gestionar archivos del servidor desde el panel web.

#### Paso 5.1 — Endpoints
```
GET    /api/pods/:id/files?path=/                     → Listar directorio
GET    /api/pods/:id/files/content?path=/server.properties → Leer archivo (texto)
PUT    /api/pods/:id/files/content?path=/server.properties → Escribir archivo
POST   /api/pods/:id/files/create-dir?path=/plugins/new   → Crear carpeta
POST   /api/pods/:id/files/rename                     → Renombrar/Mover
DELETE /api/pods/:id/files?path=/logs/old.log          → Eliminar archivo/carpeta
POST   /api/pods/:id/files/upload?path=/plugins        → Subir archivo (multipart)
GET    /api/pods/:id/files/download?path=/world        → Descargar como tar.gz
```

#### Paso 5.2 — Implementación
- El agente conoce el `volume_path` de cada Pod/container
  (ej: `/home/neox/servers/mc-proxy-1/data/`)
- Las operaciones son directas sobre el filesystem del host (el volumen montado)
- **Seguridad crítica:**
  - Validar que el path no escape del directorio del servidor (path traversal)
  - Sanitizar: rechazar `..`, links simbólicos fuera del volumen
  - Limitar tamaño de uploads (configurable)
  - Opciones de permisos (read-only si el servidor está corriendo)

#### Paso 5.3 — Response de Listar Directorio
```json
GET /api/pods/mc-proxy-1/files?path=/
{
    "path": "/",
    "items": [
        { "name": "server.properties", "type": "file", "size": 1234, "modified": "2026-02-17T18:00:00Z", "permissions": "rw-r--r--" },
        { "name": "plugins",           "type": "dir",  "size": 0,    "modified": "2026-02-17T17:00:00Z", "children": 12 },
        { "name": "world",             "type": "dir",  "size": 0,    "modified": "2026-02-17T18:30:00Z", "children": 45 },
        { "name": "logs",              "type": "dir",  "size": 0,    "modified": "2026-02-17T18:31:00Z", "children": 3 }
    ]
}
```

**Criterio de éxito:** Editar `server.properties` desde una petición HTTP y reiniciar el servidor.

---

### ═══════════════════════════════════════════════
### FASE 6: Backups (Días 19-21)
### ═══════════════════════════════════════════════

**Objetivo:** Crear y restaurar backups de los datos del servidor.

#### Paso 6.1 — Endpoints
```
GET    /api/pods/:id/backups                    → Listar backups
POST   /api/pods/:id/backups                    → Crear backup
GET    /api/pods/:id/backups/:backup_id         → Info del backup (tamaño, fecha, checksum)
GET    /api/pods/:id/backups/:backup_id/download → Descargar backup (tar.gz stream)
POST   /api/pods/:id/backups/:backup_id/restore  → Restaurar backup
DELETE /api/pods/:id/backups/:backup_id          → Eliminar backup
```

#### Paso 6.2 — Flujo de Backup
1. (Opcional) Detener el Pod para consistencia de datos
2. Comprimir el directorio del volumen → `{timestamp}.tar.gz`
3. Calcular checksum SHA256
4. Guardar en `/var/lib/neoxagent/backups/{pod_id}/{timestamp}.tar.gz`
5. (Opcional) Reiniciar el Pod
6. Responder con metadata:
```json
{
    "id": "backup-uuid",
    "pod_id": "mc-proxy-1",
    "size_mb": 245,
    "created_at": "2026-02-17T18:00:00Z",
    "checksum_sha256": "abc123...",
    "includes_world": true,
    "server_was_stopped": true
}
```

#### Paso 6.3 — Configuración de Backups
```toml
[backups]
max_per_server = 5
max_size_gb = 10
retention_days = 30
compression_level = 6          # 1-9, balance velocidad/tamaño
stop_server_before_backup = true
```

---

### ═══════════════════════════════════════════════
### FASE 7: Imágenes + Systemd (Días 22-24)
### ═══════════════════════════════════════════════

**Objetivo:** Gestionar imágenes y auto-iniciar servidores con systemd.

#### Paso 7.1 — Endpoints de Imágenes
```
GET    /api/images                         → Listar imágenes en el nodo
POST   /api/images/pull                    → Descargar imagen
DELETE /api/images/:id                     → Eliminar imagen
GET    /api/images/search?q=minecraft      → Buscar en registries
```

#### Paso 7.2 — Pull con Progreso (WebSocket)
```
WS /api/images/pull/stream
→ { "image": "itzg/minecraft-server", "layer": "abc", "status": "Downloading", "progress": "45/120 MB" }
→ { "status": "Pull complete" }
```

#### Paso 7.3 — Systemd Integration (Exclusivo de Podman)
```
POST /api/pods/:id/systemd/generate    → Generar archivo .service para el Pod
POST /api/pods/:id/systemd/enable      → Habilitar auto-start al boot
POST /api/pods/:id/systemd/disable     → Deshabilitar auto-start
GET  /api/pods/:id/systemd/status      → Estado del servicio systemd
```

**Flujo:**
1. Crear Pod → funciona
2. `POST /api/pods/mc-proxy-1/systemd/generate`
   → Podman genera `pod-mc-proxy-1.service` automáticamente
3. `POST /api/pods/mc-proxy-1/systemd/enable`
   → El servicio se habilita en systemd
4. Si el VPS se reinicia → systemd levanta el Pod automáticamente
5. Si el Pod crashea → systemd lo reinicia (configurable)

**Criterio de éxito:** Reiniciar el VPS y que los servidores de juego vuelvan solos.

---

### ═══════════════════════════════════════════════
### FASE 8: Integración con el Panel Next.js (Días 25-32)
### ═══════════════════════════════════════════════

**Objetivo:** Conectar todo con tu panel Jexactyl.

#### Paso 8.1 — Modelo de Datos en el Panel (Prisma/DB)
```prisma
model Node {
    id            String         @id @default(uuid())
    name          String                              // "Nodo Miami 1"
    fqdn          String                              // "nodo1.neoxhost.com"
    port          Int            @default(8443)
    apiKey        String                              // Token del agente
    location      String?                             // "Miami, FL"
    isOnline      Boolean        @default(false)
    memoryTotalMb Int            @default(0)
    diskTotalGb   Int            @default(0)
    cpuCores      Int            @default(0)
    podmanVersion String?
    servers       PodServer[]
    createdAt     DateTime       @default(now())
    updatedAt     DateTime       @updatedAt
}

model PodServer {
    id            String         @id @default(uuid())
    userId        String                              // Dueño del servidor
    nodeId        String                              // En qué nodo está
    node          Node           @relation(fields: [nodeId], references: [id])
    
    // Identificadores Podman
    podId         String?                             // Podman Pod ID
    podName       String                              // "mc-proxy-1"
    
    // Configuración
    name          String                              // "Mi Minecraft"
    image         String                              // "itzg/minecraft-server:latest"
    status        String         @default("created")  // created, running, stopped, error
    
    // Proxy
    proxyEnabled  Boolean        @default(false)
    proxyType     String?                             // "tun2socks"
    proxySocks5   String?                             // URL del SOCKS5 (encrypted)
    
    // Resources
    ports         Json                                // [{ host: 25565, container: 25565 }]
    envVars       Json                                // { EULA: "TRUE", ... }
    limits        Json                                // { memory_mb: 2048, cpu: 2.0 }
    volumePath    String?                             // Path en el nodo
    
    // Kube YAML (si fue desplegado desde YAML)
    kubeYaml      String?        @db.Text
    
    // Systemd
    systemdEnabled Boolean       @default(false)
    
    // Backups
    backups       Backup[]
    
    createdAt     DateTime       @default(now())
    updatedAt     DateTime       @updatedAt
}

model Backup {
    id            String         @id @default(uuid())
    serverId      String
    server        PodServer      @relation(fields: [serverId], references: [id])
    sizeMb        Int
    checksumSha256 String?
    createdAt     DateTime       @default(now())
}
```

#### Paso 8.2 — API Routes en Next.js (Proxy al Agente)
```
// Nodos
GET    /api/nodes                    → Lista nodos registrados
POST   /api/nodes                    → Registrar nuevo nodo
GET    /api/nodes/:id                → Detalle del nodo (ping al agente)
DELETE /api/nodes/:id                → Eliminar nodo

// Servidores (proxy al agente del nodo correspondiente)
GET    /api/pod-servers              → Lista servidores del usuario
POST   /api/pod-servers              → Crear servidor (elige nodo, crea Pod en agente)
GET    /api/pod-servers/:id          → Detalle (pide stats al agente)
DELETE /api/pod-servers/:id          → Eliminar (elimina Pod en agente + registro en DB)

// Power (proxy)
POST   /api/pod-servers/:id/power    → { action: "start"|"stop"|"restart"|"kill" }

// WebSocket (proxy)
WS     /api/pod-servers/:id/console  → Proxy WS hacia agente → contenedor
WS     /api/pod-servers/:id/logs     → Proxy WS hacia agente → logs
WS     /api/pod-servers/:id/stats    → Proxy WS hacia agente → stats

// Files (proxy)
GET    /api/pod-servers/:id/files    → Proxy al file manager del agente
PUT    /api/pod-servers/:id/files    → Proxy escritura
POST   /api/pod-servers/:id/files/upload → Proxy upload

// Backups (proxy)
GET    /api/pod-servers/:id/backups  → Lista backups
POST   /api/pod-servers/:id/backups  → Crear backup
```

#### Paso 8.3 — Páginas del Panel
```
/dashboard                           → Lista servidores (ya existente, adaptar)
/pod-servers/create                  → Formulario: imagen, env, puertos, proxy, nodo
/pod-servers/[id]                    → Dashboard del servidor (stats, console, overview)
/pod-servers/[id]/console            → Terminal interactiva (xterm.js + WebSocket)
/pod-servers/[id]/files              → File manager visual
/pod-servers/[id]/files/[...path]    → Editor de archivos
/pod-servers/[id]/backups            → Lista y gestión de backups
/pod-servers/[id]/network            → Config de red / proxy tun2socks
/pod-servers/[id]/settings           → Nombre, imagen, env vars, limits
/pod-servers/[id]/startup            → Variables de arranque, restart policy
/admin/nodes                         → Gestión de nodos
/admin/nodes/[id]                    → Detalle: servidores, recursos, estado
/admin/nodes/new                     → Registrar nodo nuevo
```

---

### ═══════════════════════════════════════════════
### FASE 9: Seguridad y Producción (Días 33-37)
### ═══════════════════════════════════════════════

#### Paso 9.1 — TLS (HTTPS) para el Agente
```toml
[tls]
enabled = true
cert_path = "/etc/neoxagent/cert.pem"
key_path = "/etc/neoxagent/key.pem"
```
- Usar Let's Encrypt con certbot
- O self-signed para redes internas

#### Paso 9.2 — Rate Limiting
```rust
// Usar tower::limit::RateLimitLayer en Axum
// Limitar a 100 req/min por API Key
```

#### Paso 9.3 — Validación de Inputs
- Sanitizar nombres de contenedores/pods (alfanuméricos + guiones)
- Validar paths del file manager (prevenir path traversal: `../../etc/passwd`)
- Limitar tamaño de uploads (configurable, default 100MB)
- Validar imágenes contra lista de registries permitidos
- Limitar capacidades (`NET_ADMIN` solo si proxy habilitado)

#### Paso 9.4 — Systemd Service para el Agente
```ini
# /etc/systemd/system/neoxagent.service
# NOTA: Corre como usuario normal, NO como root
[Unit]
Description=neoxagent - Podman Management Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=neox
Group=neox
ExecStart=/usr/local/bin/neoxagent
Restart=always
RestartSec=5
WorkingDirectory=/var/lib/neoxagent
Environment=RUST_LOG=info

# Seguridad adicional de systemd
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/lib/neoxagent /home/neox/servers

[Install]
WantedBy=multi-user.target
```

#### Paso 9.5 — Script de Instalación Automatizada
```bash
#!/bin/bash
# install.sh — Instalar neoxagent en un nodo nuevo
# Uso: curl -sSL https://tudominio.com/install.sh | bash -s -- --api-key MI_CLAVE
#
# El script:
# 1. Detecta el OS (Ubuntu/Debian/Fedora/RHEL)
# 2. Instala Podman si no está instalado
# 3. Configura Podman rootless
# 4. Habilita podman.socket
# 5. Configura sysctl para puertos bajos
# 6. Habilita linger para el usuario
# 7. Descarga el binario neoxagent desde releases
# 8. Genera config.toml con la API key proporcionada
# 9. Instala el servicio systemd
# 10. Inicia el agente
# 11. Verifica que responde en /api/health
```

---

## 📅 Resumen de Timeline

| Fase | Descripción                              | Duración   | Acumulado |
| :--: | :--------------------------------------- | :--------- | :-------- |
| 0    | Setup Rust + Podman + conexión           | 1 día      | Día 1     |
| 1    | API REST + CRUD Contenedores             | 3 días     | Día 4     |
| 2    | Logs + Console + Stats (WebSocket)       | 3 días     | Día 7     |
| 3    | **Pods + Tun2socks** (feature clave)     | 4 días     | Día 11    |
| 4    | Kubernetes YAML deploy                   | 3 días     | Día 14    |
| 5    | File Manager                             | 4 días     | Día 18    |
| 6    | Backups                                  | 3 días     | Día 21    |
| 7    | Imágenes + Systemd auto-start            | 3 días     | Día 24    |
| 8    | Integración Panel Next.js                | 8 días     | Día 32    |
| 9    | Seguridad + Producción                   | 5 días     | Día 37    |

**Total estimado: ~5-6 semanas de desarrollo**

---

## 🏁 MVP Mínimo (Fases 0-3 = 11 días)

Con las primeras 4 fases tienes un agente funcional:

- ✅ Crear servidores desde cualquier imagen
- ✅ Start/Stop/Restart con consola en vivo
- ✅ CPU/RAM stats en tiempo real
- ✅ **Pods con tun2socks** (tu objetivo principal)
- ✅ Rootless y seguro (Podman)
- ❌ Sin file manager (usar SSH/SFTP externo)
- ❌ Sin backups (manuales)
- ❌ Sin Kubernetes YAML

---

## 🔗 Referencias

- [Podman API (Rust SDK)](https://crates.io/crates/podman-api)
- [Podman Docs - Pods](https://docs.podman.io/en/latest/markdown/podman-pod.1.html)
- [Podman Play Kube](https://docs.podman.io/en/latest/markdown/podman-kube-play.1.html)
- [Podman Generate Systemd](https://docs.podman.io/en/latest/markdown/podman-generate-systemd.1.html)
- [Axum (Web Framework)](https://github.com/tokio-rs/axum)
- [Netavark (Podman Networking)](https://github.com/containers/netavark)
- [itzg/minecraft-server](https://github.com/itzg/docker-minecraft-server)
- [xjasonlyu/tun2socks](https://github.com/xjasonlyu/tun2socks)
