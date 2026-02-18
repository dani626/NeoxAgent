# NeoxAgent — Diagramas UML

> Documentación UML completa de la arquitectura del NeoxAgent.
> Todos los diagramas están en formato [Mermaid](https://mermaid.js.org/) para renderizado directo en GitHub/GitLab/VS Code.
>
> Generado: 2026-02-18

---

## Tabla de Contenidos

1. [Diagrama de Componentes — Arquitectura General](#1-diagrama-de-componentes--arquitectura-general)
2. [Diagrama de Clases — Modelo de Datos](#2-diagrama-de-clases--modelo-de-datos)
3. [Diagrama de Clases — Configuración](#3-diagrama-de-clases--configuración)
4. [Diagrama de Clases — Sistema de Errores](#4-diagrama-de-clases--sistema-de-errores)
5. [Diagrama de Paquetes — Módulos del Sistema](#5-diagrama-de-paquetes--módulos-del-sistema)
6. [Diagrama de Secuencia — Autenticación API Key](#6-diagrama-de-secuencia--autenticación-api-key)
7. [Diagrama de Secuencia — Ciclo de Vida de un Contenedor](#7-diagrama-de-secuencia--ciclo-de-vida-de-un-contenedor)
8. [Diagrama de Secuencia — Creación de Pod con Tun2socks](#8-diagrama-de-secuencia--creación-de-pod-con-tun2socks)
9. [Diagrama de Secuencia — WebSocket Console](#9-diagrama-de-secuencia--websocket-console)
10. [Diagrama de Secuencia — Deploy Kubernetes YAML](#10-diagrama-de-secuencia--deploy-kubernetes-yaml)
11. [Diagrama de Secuencia — Gestión de Archivos (File Manager)](#11-diagrama-de-secuencia--gestión-de-archivos)
12. [Diagrama de Actividad — Flujo de Backup](#12-diagrama-de-actividad--flujo-de-backup)
13. [Diagrama de Actividad — Restauración de Backup](#13-diagrama-de-actividad--restauración-de-backup)
14. [Diagrama de Secuencia — Pull de Imágenes (WebSocket)](#14-diagrama-de-secuencia--pull-de-imágenes-websocket)
15. [Diagrama de Secuencia — Systemd Integration](#15-diagrama-de-secuencia--systemd-integration)
16. [Diagrama de Actividad — Seguridad Path Traversal](#16-diagrama-de-actividad--seguridad-path-traversal)
17. [Diagrama de Estado — Ciclo de Vida del Pod](#17-diagrama-de-estado--ciclo-de-vida-del-pod)

---

## 1. Diagrama de Componentes — Arquitectura General

```mermaid
graph TB
    subgraph "Panel Next.js (Cliente)"
        PANEL[🖥️ Jexactyl Panel]
    end

    subgraph "NeoxAgent (Este Binario)"
        direction TB
        ROUTER[🔀 Axum Router]
        AUTH[🔒 Auth Middleware<br/>Bearer Token]
        
        subgraph "Fases Implementadas"
            P1[📦 Phase 1<br/>REST API Base]
            P2[📡 Phase 2<br/>WebSockets]
            P3[🐙 Phase 3<br/>Pods + Tun2socks]
            P4[☸️ Phase 4<br/>Kube YAML]
            P5[📁 Phase 5<br/>File Manager]
            P6[💾 Phase 6<br/>Backups]
            P7[🖼️ Phase 7<br/>Images + Systemd]
        end
        
        SVC[⚙️ Services Layer<br/>podman.rs]
        CFG[📋 Config<br/>config.toml]
        STATE[🧠 AppState<br/>Podman + Config]
    end

    subgraph "Sistema Operativo"
        PODMAN[🐙 Podman Engine]
        SOCK[podman.sock<br/>Unix Socket]
        FS[📂 Filesystem<br/>Volumes + Backups]
        SYSTEMD[🔧 systemd<br/>Service Manager]
    end

    PANEL -->|HTTPS + API Key| ROUTER
    ROUTER --> AUTH
    AUTH --> P1 & P2 & P3 & P4 & P5 & P6 & P7
    P1 & P3 --> SVC
    SVC -->|podman-api SDK| SOCK
    SOCK --> PODMAN
    P5 & P6 -->|tokio::fs| FS
    P7 -->|CLI: systemctl| SYSTEMD
    P4 -->|CLI: podman play kube| PODMAN
    STATE --> CFG
```

---

## 2. Diagrama de Clases — Modelo de Datos

```mermaid
classDiagram
    direction TB
    
    class AppState {
        +Podman podman
        +Config config
    }

    %% ═══ Container Models ═══
    class CreateContainerRequest {
        +String name
        +String image
        +HashMap~String,String~ env
        +Vec~PortMapping~ ports
        +Vec~VolumeMount~ volumes
        +ResourceLimits limits
        +HashMap~String,String~ labels
        +Vec~String~ dns
        +Vec~String~ command
        +String restart_policy
    }

    class ContainerResponse {
        +String id
        +String name
        +String image
        +String status
        +String created
        +HashMap~String,String~ labels
        +Vec~PortMapping~ ports
        +Vec~MountInfo~ mounts
        +ResourceUsage resources
    }

    class PortMapping {
        +u16 host_port
        +u16 container_port
        +String protocol
        +String host_ip
    }

    class VolumeMount {
        +String host_path
        +String container_path
        +bool read_only
    }

    class ResourceLimits {
        +i64 memory_mb
        +f64 cpu_cores
    }

    %% ═══ Pod Models ═══
    class CreatePodRequest {
        +String name
        +ProxyConfig proxy
        +Vec~PodContainerSpec~ containers
        +HashMap~String,String~ labels
        +Vec~String~ dns_servers
        +String hostname
        +String network
    }

    class ProxyConfig {
        +bool enabled
        +String proxy_type
        +String image
        +String socks5_url
        +String dns
        +HashMap~String,String~ env
        +String loglevel
    }

    class PodContainerSpec {
        +String name
        +String image
        +HashMap~String,String~ env
        +Vec~PortMapping~ ports
        +ResourceLimits limits
        +Vec~VolumeMount~ volumes
        +HashMap~String,String~ labels
        +String restart_policy
        +Vec~String~ command
    }

    class PodResponse {
        +String id
        +String name
        +String status
        +String created_at
        +String hostname
        +HashMap~String,String~ labels
        +Vec~PodContainerInfo~ containers
        +bool proxy_enabled
        +String infra_id
    }

    %% ═══ Network Models ═══
    class CreateNetworkRequest {
        +String name
        +String driver
        +bool dns_enabled
        +bool internal
        +bool ipv6_enabled
        +String subnet
        +String gateway
        +HashMap~String,String~ labels
    }

    class NetworkResponse {
        +String name
        +String id
        +String driver
        +bool dns_enabled
        +bool internal
        +Vec~SubnetInfo~ subnets
        +HashMap~String,String~ labels
    }

    %% ═══ File Manager Models ═══
    class FileEntry {
        +String name
        +String entry_type
        +u64 size
        +String modified
        +String permissions
        +u64 children
    }

    class RenameRequest {
        +String from
        +String to
    }

    %% ═══ Backup Models ═══
    class BackupInfo {
        +String id
        +String pod_id
        +u64 size_bytes
        +f64 size_mb
        +String created_at
        +String checksum_sha256
        +bool server_was_stopped
        +String filename
        +String description
    }

    class CreateBackupRequest {
        +bool stop_server
        +String description
    }

    %% ═══ Image Models ═══
    class ImageInfo {
        +String id
        +Vec~String~ repo_tags
        +Vec~String~ repo_digests
        +i64 size
        +i64 virtual_size
        +i64 created
    }

    class PullImageRequest {
        +String image
    }

    class ImageSearchQuery {
        +String q
        +u32 limit
    }

    %% ═══ Systemd Models ═══
    class SystemdGenerateResponse {
        +String pod_id
        +String service_name
        +String service_file_path
        +String unit_content
    }

    class SystemdStatusResponse {
        +String pod_id
        +String service_name
        +String active_state
        +String sub_state
        +bool enabled
    }

    %% ═══ Relationships ═══
    CreateContainerRequest --> PortMapping
    CreateContainerRequest --> VolumeMount
    CreateContainerRequest --> ResourceLimits
    CreatePodRequest --> ProxyConfig
    CreatePodRequest --> PodContainerSpec
    PodContainerSpec --> PortMapping
    PodContainerSpec --> VolumeMount
    PodContainerSpec --> ResourceLimits
    ContainerResponse --> PortMapping
```

---

## 3. Diagrama de Clases — Configuración

```mermaid
classDiagram
    direction LR

    class Config {
        +AgentConfig agent
        +PodmanConfig podman
        +TlsConfig tls
        +DefaultsConfig defaults
        +BackupsConfig backups
        +load(path: str) Config
        +load_default() Config
    }

    class AgentConfig {
        +String host
        +u16 port
        +String api_key
        +PathBuf data_dir
    }

    class PodmanConfig {
        +String socket
        +PathBuf volumes_dir
    }

    class TlsConfig {
        +bool enabled
        +String cert_path
        +String key_path
    }

    class DefaultsConfig {
        +String restart_policy
        +Vec~String~ dns
    }

    class BackupsConfig {
        +u32 max_per_server
        +u32 max_size_gb
        +u32 retention_days
        +u32 compression_level
        +bool stop_server_before_backup
    }

    Config *-- AgentConfig
    Config *-- PodmanConfig
    Config *-- TlsConfig
    Config *-- DefaultsConfig
    Config *-- BackupsConfig
```

---

## 4. Diagrama de Clases — Sistema de Errores

```mermaid
classDiagram
    class AppError {
        <<enum>>
        Podman(String)
        Config(String)
        Unauthorized
        NotFound(String)
        BadRequest(String)
        Internal(String)
        Io(io::Error)
        +fmt() String
        +into_response() Response
    }

    class StatusCode {
        <<HTTP>>
        BAD_GATEWAY ~502~
        INTERNAL_SERVER_ERROR ~500~
        UNAUTHORIZED ~401~
        NOT_FOUND ~404~
        BAD_REQUEST ~400~
    }

    AppError ..> StatusCode : maps to
    AppError ..|> IntoResponse : implements
    AppError ..|> Display : implements

    note for AppError "Cada variante se mapea a un\ncódigo HTTP específico y\ndevuelve JSON con error+message"
```

---

## 5. Diagrama de Paquetes — Módulos del Sistema

```mermaid
graph TB
    subgraph "main.rs"
        MAIN[main<br/>Axum Router Setup]
        APPSTATE[AppState]
    end

    subgraph "auth.rs"
        AUTHMW[auth_middleware]
        APIKEY[ApiKey]
    end

    subgraph "config.rs"
        CONFIG[Config]
    end

    subgraph "error.rs"
        APPERROR[AppError]
    end

    subgraph "models/"
        M_CONT[container.rs]
        M_POD[pod.rs]
        M_NET[network.rs]
        M_KUBE[kube.rs]
        M_FILE[files.rs]
        M_BACK[backups.rs]
        M_IMG[images.rs]
    end

    subgraph "routes/"
        R_SYS[system.rs<br/>GET /api/health]
        R_CONT[containers.rs<br/>CRUD + Lifecycle]
        R_WS[ws.rs<br/>Logs + Console + Stats]
        R_POD[pods.rs<br/>CRUD + Lifecycle]
        R_NET[networks.rs<br/>CRUD]
        R_KUBE[kube.rs<br/>Deploy + Stacks]
        R_FILE[files.rs<br/>File Manager]
        R_BACK[backups.rs<br/>Backup CRUD]
        R_IMG[images.rs<br/>Image Management]
        R_SYSD[systemd.rs<br/>Service Control]
    end

    subgraph "services/"
        S_POD[podman.rs<br/>Container SDK Wrapper]
    end

    MAIN --> AUTHMW
    MAIN --> APPSTATE
    APPSTATE --> CONFIG
    R_CONT --> S_POD
    R_CONT --> M_CONT
    R_POD --> M_POD
    R_NET --> M_NET
    R_KUBE --> M_KUBE
    R_FILE --> M_FILE
    R_BACK --> M_BACK
    R_IMG --> M_IMG
    R_SYSD --> M_IMG
    S_POD --> APPERROR
```

---

## 6. Diagrama de Secuencia — Autenticación API Key

```mermaid
sequenceDiagram
    actor Client as 🖥️ Panel Next.js
    participant Router as 🔀 Axum Router
    participant Auth as 🔒 auth_middleware
    participant Handler as 📦 Route Handler

    Client->>Router: HTTP Request<br/>Authorization: Bearer <API_KEY>
    Router->>Auth: Request + Extensions(ApiKey)
    
    alt Path = /api/health
        Auth->>Handler: Skip auth ✅
        Handler-->>Client: 200 OK
    else Token válido
        Auth->>Auth: Extraer Bearer token
        Auth->>Auth: Comparar con ApiKey esperada
        Auth->>Handler: Request continúa ✅
        Handler-->>Client: 200 OK + JSON
    else Token inválido o ausente
        Auth-->>Client: 401 Unauthorized ❌<br/>{"error": true, "message": "Invalid or missing API key"}
    end
```

---

## 7. Diagrama de Secuencia — Ciclo de Vida de un Contenedor

```mermaid
sequenceDiagram
    actor Client as 🖥️ Panel
    participant Route as routes::containers
    participant Svc as services::podman
    participant Pod as 🐙 Podman SDK
    participant Engine as Podman Engine

    Note over Client,Engine: Crear Contenedor
    Client->>Route: POST /api/containers<br/>{name, image, env, ports...}
    Route->>Route: Validar campos requeridos
    Route->>Svc: create_container(state, req)
    Svc->>Svc: Construir ContainerCreateOpts<br/>(env, ports, volumes, limits, labels)
    Svc->>Pod: containers().create(&opts)
    Pod->>Engine: API call via podman.sock
    Engine-->>Pod: Container ID
    Pod-->>Svc: CreateInfo
    Svc->>Pod: containers().get(id).inspect()
    Pod->>Engine: Inspect
    Engine-->>Pod: InspectContainerData
    Svc-->>Route: ContainerResponse
    Route-->>Client: 200 OK + JSON

    Note over Client,Engine: Lifecycle (Start/Stop/Restart/Kill)
    Client->>Route: POST /api/containers/:id/start
    Route->>Svc: start_container(state, id)
    Svc->>Pod: containers().get(id).start()
    Pod->>Engine: Start via socket
    Engine-->>Client: 200 OK

    Client->>Route: POST /api/containers/:id/stop?timeout=10
    Route->>Svc: stop_container(state, id, timeout)
    Svc->>Pod: containers().get(id).stop(&opts)
    Pod->>Engine: SIGTERM → wait → SIGKILL
    Engine-->>Client: 200 OK

    Note over Client,Engine: Eliminar Contenedor
    Client->>Route: DELETE /api/containers/:id?force=true
    Route->>Svc: delete_container(state, id, true, false)
    Svc->>Pod: containers().get(id).delete(&opts)
    Pod->>Engine: Force remove
    Engine-->>Client: 200 OK
```

---

## 8. Diagrama de Secuencia — Creación de Pod con Tun2socks

```mermaid
sequenceDiagram
    actor Client as 🖥️ Panel
    participant Route as routes::pods
    participant Pod as 🐙 Podman SDK
    participant Engine as Podman Engine

    Client->>Route: POST /api/pods<br/>{name, proxy: {enabled, socks5_url}, containers: [...]}
    
    Route->>Route: Validar request
    
    Note over Route,Engine: 1. Crear Pod
    Route->>Pod: PodCreateOpts::builder()<br/>.name().dns().hostname().labels()
    Pod->>Engine: Crear Pod infraestructura
    Engine-->>Route: Pod ID

    alt proxy.enabled = true
        Note over Route,Engine: 2. Crear Proxy Sidecar (tun2socks)
        Route->>Route: Configurar env vars:<br/>PROXY_URL, DNS, LOGLEVEL
        Route->>Pod: ContainerCreateOpts<br/>image: tun2socks<br/>cap_add: NET_ADMIN<br/>devices: /dev/net/tun
        Pod->>Engine: Crear contenedor proxy en Pod
        Engine-->>Route: Proxy Container ID
    end

    Note over Route,Engine: 3. Crear Game Server Container(s)
    loop Para cada container en request
        Route->>Pod: ContainerCreateOpts<br/>pod: pod_id, image, env, ports, volumes
        Pod->>Engine: Crear contenedor en Pod
        Engine-->>Route: Container ID
    end

    Note over Route,Engine: 4. Iniciar Pod
    Route->>Pod: pods().get(id).start()
    Pod->>Engine: Start Pod + all containers
    Engine-->>Route: OK

    Note over Route,Engine: 5. Inspeccionar resultado
    Route->>Pod: pods().get(id).inspect()
    Pod->>Engine: Inspect
    Engine-->>Route: InspectPodData
    Route-->>Client: 200 OK + PodResponse
```

---

## 9. Diagrama de Secuencia — WebSocket Console

```mermaid
sequenceDiagram
    actor Client as 🖥️ Browser
    participant WS as ws.rs Handler
    participant Pod as 🐙 Podman SDK
    participant Container as 📦 Container

    Client->>WS: WS Upgrade<br/>GET /api/containers/:id/console

    WS->>Pod: containers().get(id).inspect()
    Pod-->>WS: Container labels

    alt labels["neox.type"] = "gameserver"
        Note over WS,Container: Modo Attach (stdin → PID 1)
        WS->>Pod: containers().get(id).attach(&opts)
        Pod-->>WS: Multiplexer (reader, writer)
        
        par Lectura (Container → Cliente)
            loop Stream de salida
                Container-->>Pod: stdout/stderr chunks
                Pod-->>WS: TtyChunk
                WS-->>Client: {"stream": "stdout", "data": "..."}
            end
        and Escritura (Cliente → Container)
            loop Comandos del usuario
                Client->>WS: "say Hello World\n"
                WS->>Pod: writer.write_all(input)
                Pod->>Container: stdin
            end
        end
    else Contenedor genérico
        Note over WS,Container: Modo Exec (shell interactivo)
        WS->>Pod: containers().get(id).create_exec(&opts)<br/>cmd: ["/bin/sh"]
        Pod-->>WS: Exec ID
        WS->>Pod: exec_start(exec_id)
        Pod-->>WS: Multiplexer
        
        par Lectura
            Container-->>Client: Shell output via WS
        and Escritura
            Client->>Container: Commands via WS
        end
    end

    Client->>WS: WS Close
    WS->>WS: Cleanup resources
```

---

## 10. Diagrama de Secuencia — Deploy Kubernetes YAML

```mermaid
sequenceDiagram
    actor Client as 🖥️ Panel
    participant Route as routes::kube
    participant FS as 📂 Filesystem
    participant CLI as podman CLI

    Client->>Route: POST /api/kube/deploy<br/>{name, yaml_content, start: true}
    
    Route->>Route: Validar nombre y YAML
    Route->>Route: Parsear YAML (serde_yaml)
    
    Note over Route,FS: Persistir Stack
    Route->>FS: Crear {data_dir}/stacks/{name}/
    Route->>FS: Escribir stack.yaml
    Route->>FS: Escribir metadata.json<br/>{name, deployed_at, yaml_hash}

    Note over Route,CLI: Deploy con Podman
    Route->>CLI: podman play kube stack.yaml
    CLI-->>Route: stdout (pods/containers creados)
    
    alt Éxito
        Route->>FS: Actualizar metadata.json<br/>status: "deployed"
        Route-->>Client: 200 OK<br/>{name, status, pods_created}
    else Error
        Route-->>Client: 502 Bad Gateway<br/>{error: "podman play kube failed"}
    end

    Note over Client,CLI: Lifecycle de Stacks
    Client->>Route: POST /api/kube/stacks/:name/down
    Route->>CLI: podman play kube --down stack.yaml
    CLI-->>Client: Stack teardown complete

    Client->>Route: POST /api/kube/stacks/:name/up
    Route->>CLI: podman play kube stack.yaml
    CLI-->>Client: Stack recreated
```

---

## 11. Diagrama de Secuencia — Gestión de Archivos

```mermaid
sequenceDiagram
    actor Client as 🖥️ Panel
    participant Route as routes::files
    participant Security as 🔒 safe_resolve()
    participant FS as 📂 tokio::fs

    Note over Client,FS: Listar archivos
    Client->>Route: GET /api/pods/:id/files?path=/
    Route->>Route: resolve_volume_root(state, pod_id)
    Route->>Security: safe_resolve(root, "/")
    Security->>Security: Rechazar ".." traversal
    Security->>Security: Canonicalizar path
    Security->>Security: Verificar starts_with(root)
    Security-->>Route: PathBuf seguro ✅
    Route->>FS: read_dir(target)
    FS-->>Route: Vec<DirEntry>
    Route->>Route: Sort: dirs first, then alphabetical
    Route-->>Client: 200 OK + {path, entries: [...]}

    Note over Client,FS: Leer archivo
    Client->>Route: GET /api/pods/:id/files/content?path=/server.properties
    Route->>Security: safe_resolve(root, path)
    Route->>FS: metadata(target) → check size < 10MB
    Route->>FS: read_to_string(target)
    FS-->>Route: String content
    Route-->>Client: 200 OK + {content, size, modified}

    Note over Client,FS: Subir archivo
    Client->>Route: POST /api/pods/:id/files/upload?path=/plugins<br/>multipart/form-data
    Route->>Security: safe_resolve(root, path)
    loop Para cada archivo en multipart
        Route->>Route: Sanitizar filename<br/>No /, \, ..
        Route->>Route: Check size < 100MB
        Route->>FS: write(target/filename, bytes)
    end
    Route-->>Client: 200 OK + {uploaded: [...]}

    Note over Client,FS: Descargar directorio
    Client->>Route: GET /api/pods/:id/files/download?path=/world
    Route->>Security: safe_resolve(root, path)
    alt Es archivo
        Route->>FS: read(target)
        Route-->>Client: Raw bytes + Content-Disposition
    else Es directorio
        Route->>Route: spawn_blocking → tar.gz
        Route-->>Client: application/gzip stream
    end
```

---

## 12. Diagrama de Actividad — Flujo de Backup

```mermaid
flowchart TD
    A[📥 POST /api/pods/:id/backups] --> B{Volume dir<br/>existe?}
    B -->|No| ERR1[❌ 404 Not Found]
    B -->|Sí| C[Crear backups/ dir]
    C --> D{stop_server?}
    
    D -->|Sí| E[⏹️ podman pod stop]
    E --> F[Crear tar.gz]
    D -->|No| F
    
    F --> G[📦 spawn_blocking<br/>tar::Builder + flate2::GzEncoder]
    G --> H[Comprimir volumes_dir/{pod_id}/<br/>→ YYYYMMDD_HHMMSS.tar.gz]
    
    H --> I[🔐 spawn_blocking<br/>SHA256 checksum]
    I --> J[📝 Crear BackupInfo<br/>id, size, checksum, timestamp]
    
    J --> K[Actualizar index.json]
    K --> L{Supera<br/>max_per_server?}
    
    L -->|Sí| M[🗑️ Eliminar backups más antiguos]
    M --> N{Se detuvo<br/>el server?}
    L -->|No| N
    
    N -->|Sí| O[▶️ podman pod start]
    O --> P[✅ 200 OK + BackupInfo]
    N -->|No| P

    style A fill:#4CAF50,color:#fff
    style ERR1 fill:#f44336,color:#fff
    style P fill:#2196F3,color:#fff
    style G fill:#FF9800,color:#fff
    style I fill:#FF9800,color:#fff
```

---

## 13. Diagrama de Actividad — Restauración de Backup

```mermaid
flowchart TD
    A[📥 POST /api/pods/:id/backups/:backup_id/restore] --> B{Backup existe<br/>en index.json?}
    B -->|No| ERR1[❌ 404 Not Found]
    B -->|Sí| C{Archivo .tar.gz<br/>existe en disco?}
    C -->|No| ERR2[❌ 404 File Not Found]
    C -->|Sí| D[⏹️ podman pod stop]
    
    D --> E[🗑️ Limpiar volume dir<br/>remove_dir_all + remove_file<br/>Mantener dir raíz]
    
    E --> F[📦 spawn_blocking<br/>Extraer tar.gz]
    F --> G[flate2::GzDecoder<br/>tar::Archive::entries]
    G --> H[Strip "data/" prefix<br/>Recrear estructura de archivos]
    
    H --> I[▶️ podman pod start]
    I --> J{Pod inició<br/>correctamente?}
    
    J -->|Sí| K[✅ 200 OK<br/>pod_restarted: true]
    J -->|No| L[⚠️ 200 OK<br/>pod_restarted: false]

    style A fill:#4CAF50,color:#fff
    style ERR1 fill:#f44336,color:#fff
    style ERR2 fill:#f44336,color:#fff
    style K fill:#2196F3,color:#fff
    style L fill:#FF9800,color:#fff
```

---

## 14. Diagrama de Secuencia — Pull de Imágenes (WebSocket)

```mermaid
sequenceDiagram
    actor Client as 🖥️ Panel
    participant WS as routes::images<br/>pull_image_stream
    participant SDK as 🐙 Podman SDK
    participant Registry as 🌐 Container Registry

    Client->>WS: WS Upgrade<br/>GET /api/images/pull/stream
    Client->>WS: {"image": "docker.io/itzg/minecraft-server"}
    
    WS-->>Client: {"type": "start", "status": "Starting pull"}
    
    WS->>SDK: PullOpts::builder().reference(image)
    SDK->>Registry: Pull request

    loop Stream de progreso
        Registry-->>SDK: Layer data + progress
        SDK-->>WS: LibpodImagesPullReport
        
        alt report.error is Some
            WS-->>Client: {"type": "error", "error": "..."}
        else report.stream is Some
            WS-->>Client: {"type": "progress",<br/>"stream": "Pulling layer abc...",<br/>"id": "sha256:..."}
        end
    end

    WS-->>Client: {"type": "complete",<br/>"status": "Pull complete"}
    
    Note over Client,WS: WS connection closes
```

---

## 15. Diagrama de Secuencia — Systemd Integration

```mermaid
sequenceDiagram
    actor Client as 🖥️ Panel
    participant Route as routes::systemd
    participant CLI as systemctl / podman CLI
    participant Systemd as 🔧 systemd
    participant FS as 📂 Filesystem

    Note over Client,Systemd: 1. Generar servicio  
    Client->>Route: POST /api/pods/:id/systemd/generate
    Route->>CLI: podman generate systemd<br/>--name --new --restart-policy=on-failure<br/>pod_id
    CLI-->>Route: Unit file content
    
    Route->>Route: is_rootless()?
    alt Rootless
        Route->>FS: Write to ~/.config/systemd/user/<br/>pod-{id}.service
    else Root
        Route->>FS: Write to /etc/systemd/system/<br/>pod-{id}.service
    end
    
    Route->>CLI: systemctl [--user] daemon-reload
    Route-->>Client: 200 OK + {service_name, unit_content}

    Note over Client,Systemd: 2. Habilitar auto-start
    Client->>Route: POST /api/pods/:id/systemd/enable
    Route->>CLI: systemctl [--user] enable pod-{id}.service
    CLI->>Systemd: Enable service
    Systemd-->>Client: 200 OK + {enabled: true}

    Note over Client,Systemd: 3. Verificar estado
    Client->>Route: GET /api/pods/:id/systemd/status
    Route->>CLI: systemctl [--user] show pod-{id}.service<br/>--property=ActiveState,SubState,UnitFileState
    CLI-->>Route: ActiveState=active<br/>SubState=running<br/>UnitFileState=enabled
    Route-->>Client: 200 OK + {active_state, sub_state, enabled}

    Note over Client,Systemd: Flujo de Auto-Recovery
    Note right of Systemd: VPS se reinicia →<br/>systemd inicia pod-{id}.service →<br/>Pod y containers se levantan solos ✅
```

---

## 16. Diagrama de Actividad — Seguridad Path Traversal

```mermaid
flowchart TD
    A[🔍 safe_resolve<br/>volume_root, user_path] --> B{Contiene<br/>"../" ?}
    B -->|Sí| DENY1[❌ 403 Access Denied<br/>Path traversal detected]
    B -->|No| C[Construir path:<br/>volume_root.join user_path]
    
    C --> D{volume_root<br/>existe?}
    D -->|No| E[Crear volume_root<br/>create_dir_all]
    D -->|Sí| F[Canonicalizar<br/>volume_root]
    E --> F
    
    F --> G{Target existe?}
    G -->|Sí| H[Canonicalizar target]
    G -->|No| I[Canonicalizar parent dir]
    
    H --> J{canonical_target<br/>starts_with<br/>canonical_root?}
    I --> J
    
    J -->|No| DENY2[❌ 403 Access Denied<br/>Path outside volume]
    J -->|Sí| K[✅ Return PathBuf seguro]

    style DENY1 fill:#f44336,color:#fff
    style DENY2 fill:#f44336,color:#fff
    style K fill:#4CAF50,color:#fff
```

---

## 17. Diagrama de Estado — Ciclo de Vida del Pod

```mermaid
stateDiagram-v2
    [*] --> Created : POST /api/pods
    
    Created --> Running : POST /start
    Created --> Deleted : DELETE /api/pods/:id

    Running --> Stopped : POST /stop
    Running --> Running : POST /restart
    Running --> Deleted : DELETE (force)
    Running --> BackingUp : POST /backups
    
    BackingUp --> Stopped : stop_server=true
    Stopped --> BackingUp : backup continues
    BackingUp --> Running : backup complete + restart
    
    Stopped --> Running : POST /start
    Stopped --> Deleted : DELETE /api/pods/:id
    Stopped --> Restoring : POST /restore
    
    Restoring --> Running : restore complete + start
    
    Running --> SystemdManaged : POST /systemd/enable
    SystemdManaged --> Running : POST /systemd/disable
    SystemdManaged --> SystemdManaged : VPS rebota →<br/>systemd auto-start

    Deleted --> [*]

    note right of SystemdManaged
        systemd monitorea el Pod
        Si crashea → restart automático
        Si VPS rebota → auto-start
    end note

    note right of BackingUp
        Volume dir se comprime
        a tar.gz con SHA256 checksum
    end note
```

---

## Resumen de Endpoints por Fase

| Fase | Módulo | Endpoints | Descripción |
|:-----|:-------|:---------:|:------------|
| **1** | `containers.rs` | 7 | CRUD + Lifecycle (start/stop/restart/kill) |
| **1** | `system.rs` | 1 | Health check |
| **2** | `ws.rs` | 3 | WebSocket: logs, console, stats |
| **3** | `pods.rs` | 9 | Pod CRUD + Lifecycle + Tun2socks |
| **3** | `networks.rs` | 4 | Network CRUD |
| **4** | `kube.rs` | 7 | Deploy YAML + Stack management |
| **5** | `files.rs` | 8 | File Manager (list, read, write, upload, download...) |
| **6** | `backups.rs` | 6 | Backup CRUD + Restore |
| **7** | `images.rs` | 5 | Image management + WS Pull Stream |
| **7** | `systemd.rs` | 4 | Systemd generate/enable/disable/status |
| | | **54** | **Total endpoints** |
