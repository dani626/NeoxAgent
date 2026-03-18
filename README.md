# NeoxAgent

> Lightweight Podman management agent for the Neoxhost panel. Written in Rust.

NeoxAgent runs on each VPS node and exposes a REST + WebSocket API to manage containers, pods, volumes, networks, images, backups and file system operations — all through Podman, no Docker daemon required.

## Features

- **Podman-native** — uses the `podman-api` Rust SDK, no Docker
- **Pod + proxy support** — creates pods with a `hev-socks5-tproxy` sidecar for transparent SOCKS5 proxying, VPS IP is never exposed
- **Real-time WebSockets** — container logs, interactive console, stats stream
- **Kubernetes YAML** — deploy stacks via `podman play kube`
- **File Manager** — list, read, write, upload, download, rename inside pods
- **Backups** — create, restore, download compressed pod backups
- **Image management** — pull, search, inspect, delete images
- **Systemd integration** — generate and enable `.service` units for pod auto-start
- **TLS support** — optional HTTPS via `rustls`
- **~3-5 MB RAM** at idle

## Requirements

- Debian 11/12 or Ubuntu 20.04/22.04 (x86_64)
- Root access
- Internet access (for Rust install + `apt` packages)
- Podman installed (the setup script installs it if missing)

## Install

Clone the repo and run the setup script on the VPS:

```bash
git clone https://github.com/dani626/NeoxAgent.git
bash NeoxAgent/scripts/setup.sh
```

The script will:
1. Install system dependencies (`podman`, `iptables`, `build-essential`, etc.)
2. Install Rust if not present
3. Ask for port, API key (auto-generated if left blank), Podman socket, data dirs and TLS config
4. Compile the binary (`cargo build --release`)
5. Install and enable `neoxagent.service`

At the end it prints your **API key** and **node URL**.

## Update

```bash
bash /opt/neoxagent/scripts/setup.sh --update
```

Pulls latest code, recompiles, restarts the service. Config is preserved.

## Reinstall

```bash
bash /opt/neoxagent/scripts/setup.sh --reinstall
```

Wipes the previous installation and starts fresh.

## Configuration

Config file: `/opt/neoxagent/config.toml`

```toml
[agent]
host = "0.0.0.0"
port = 8443
api_key = "your-secret-key"
data_dir = "/var/lib/neoxagent"

[podman]
socket = "/run/podman/podman.sock"
volumes_dir = "/var/lib/neoxagent/servers"

[tls]
enabled = false
cert_path = ""
key_path = ""

[defaults]
restart_policy = "always"
dns = ["1.1.1.1", "8.8.8.8"]

[backups]
max_per_server = 5
max_size_gb = 10
retention_days = 30
compression_level = 6
stop_server_before_backup = true
```

## API Authentication

All endpoints require a `Bearer` token:

```bash
curl http://NODE_IP:8443/api/health \
  -H "Authorization: Bearer your-secret-key"
```

## API Reference

### System
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/health` | Health check |
| GET | `/api/system/info` | Podman + host info |
| GET | `/api/system/resources` | CPU / RAM usage |

### Containers
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/containers` | List containers |
| POST | `/api/containers` | Create container |
| GET | `/api/containers/{id}` | Inspect container |
| DELETE | `/api/containers/{id}` | Delete container |
| GET | `/api/containers/{id}/logs` | Get logs |
| POST | `/api/containers/{id}/start` | Start |
| POST | `/api/containers/{id}/stop` | Stop |
| POST | `/api/containers/{id}/restart` | Restart |
| POST | `/api/containers/{id}/kill` | Kill |
| WS | `/api/containers/{id}/logs/stream` | Live log stream |
| WS | `/api/containers/{id}/console` | Interactive console |
| WS | `/api/containers/{id}/stats` | CPU/RAM live stats |

### Pods
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/pods` | List pods |
| POST | `/api/pods` | Create pod (with optional proxy sidecar) |
| GET | `/api/pods/{id}` | Inspect pod |
| DELETE | `/api/pods/{id}` | Delete pod |
| POST | `/api/pods/{id}/start` | Start |
| POST | `/api/pods/{id}/stop` | Stop |
| POST | `/api/pods/{id}/restart` | Restart |
| GET | `/api/pods/{id}/logs` | Logs |
| POST | `/api/pods/{id}/proxy` | Update proxy config |
| GET | `/api/pods/{id}/containers` | List pod containers |
| POST | `/api/pods/{id}/containers` | Add container to pod |
| GET | `/api/pods/{id}/kube` | Generate Kubernetes YAML |

### Files
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/pods/{id}/files` | List files |
| GET | `/api/pods/{id}/files/content` | Read file |
| PUT | `/api/pods/{id}/files/content` | Write file |
| POST | `/api/pods/{id}/files/create-dir` | Create directory |
| POST | `/api/pods/{id}/files/rename` | Rename |
| DELETE | `/api/pods/{id}/files` | Delete |
| POST | `/api/pods/{id}/files/upload` | Upload |
| GET | `/api/pods/{id}/files/download` | Download |

### Backups
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/pods/{id}/backups` | List backups |
| POST | `/api/pods/{id}/backups` | Create backup |
| GET | `/api/pods/{id}/backups/{backup_id}` | Backup info |
| GET | `/api/pods/{id}/backups/{backup_id}/download` | Download backup |
| POST | `/api/pods/{id}/backups/{backup_id}/restore` | Restore |
| DELETE | `/api/pods/{id}/backups/{backup_id}` | Delete |

### Images
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/images` | List images |
| POST | `/api/images/pull` | Pull image |
| GET | `/api/images/search` | Search images |
| WS | `/api/images/pull/stream` | Pull with live progress |
| GET | `/api/images/{id}/inspect` | Inspect |
| DELETE | `/api/images/{id}` | Delete |

### Volumes & Networks
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/volumes` | List volumes |
| POST | `/api/volumes` | Create volume |
| DELETE | `/api/volumes/{name}` | Delete volume |
| GET | `/api/networks` | List networks |
| POST | `/api/networks` | Create network |
| GET | `/api/networks/{id}` | Inspect network |
| DELETE | `/api/networks/{id}` | Delete network |

### Kube YAML
| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/kube/deploy` | Deploy from YAML |
| GET | `/api/kube/stacks` | List stacks |
| POST | `/api/kube/stacks/{name}/up` | Stack up |
| POST | `/api/kube/stacks/{name}/down` | Stack down |
| DELETE | `/api/kube/stacks/{name}` | Delete stack |
| GET | `/api/kube/stacks/{name}/status` | Stack status |
| POST | `/api/kube/generate/{pod_id}` | Generate YAML from pod |

### Systemd
| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/pods/{id}/systemd/generate` | Generate `.service` file |
| POST | `/api/pods/{id}/systemd/enable` | Enable auto-start |
| POST | `/api/pods/{id}/systemd/disable` | Disable auto-start |
| GET | `/api/pods/{id}/systemd/status` | Service status |

## Useful Commands

```bash
# Check agent status
systemctl status neoxagent

# Live agent logs
journalctl -fu neoxagent

# Verify installation
bash /opt/neoxagent/scripts/verify_paths.sh

# Test pod lifecycle
API_KEY=your-key bash /opt/neoxagent/scripts/test_lifecycle.sh
```

## Project Structure

```
NeoxAgent/
├── src/
│   ├── main.rs          # Router + server entrypoint
│   ├── auth.rs          # Bearer token middleware
│   ├── config.rs        # config.toml loader
│   ├── error.rs         # AppError types
│   ├── models/          # Request/response structs
│   ├── routes/
│   │   ├── pods.rs        # Pod CRUD + hev-socks5-tproxy
│   │   ├── containers.rs  # Container CRUD + lifecycle
│   │   ├── ws.rs          # WebSocket handlers
│   │   ├── files.rs       # File manager
│   │   ├── backups.rs     # Backup system
│   │   ├── images.rs      # Image management
│   │   ├── kube.rs        # Kubernetes YAML
│   │   ├── systemd.rs     # Systemd unit management
│   │   ├── networks.rs    # Network CRUD
│   │   └── volumes.rs     # Volume CRUD
│   └── services/        # Podman service helpers
├── scripts/
│   ├── setup.sh         # Master install / update / reinstall
│   ├── verify_paths.sh  # Check binaries and services
│   ├── test_lifecycle.sh
│   └── test_runner_wsl.sh
├── config.toml          # Default config template
└── Cargo.toml
```
