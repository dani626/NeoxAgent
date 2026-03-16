use std::sync::Arc;

use podman_api::opts::{
    ContainerCreateOpts, ContainerDeleteOpts, ContainerListOpts, ContainerStopOpts,
    ContainerRestartPolicy,
    VolumeCreateOpts, VolumeListOpts,
};
use podman_api::models::PortMapping as PodmanPortMapping;

use crate::error::AppError;
use crate::models::container::{
    ContainerResponse, CreateContainerRequest, PortMapping, ResourceLimits,
};
use crate::models::volume::{VolumeResponse, CreateVolumeRequest};
use crate::AppState;

/// Creates a container from the API request using Podman's native SDK.
///
/// This translates our clean API model into Podman's ContainerCreateOpts,
/// applying environment variables, port mappings, volume mounts, resource limits,
/// labels, DNS and restart policy.
pub async fn create_container(
    state: &Arc<AppState>,
    req: CreateContainerRequest,
) -> Result<ContainerResponse, AppError> {
    // Start building the container create options
    let mut builder = ContainerCreateOpts::builder()
        .name(&req.name)
        .image(&req.image);

    // Environment variables
    if !req.env.is_empty() {
        builder = builder.env(req.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    // Port mappings
    if !req.ports.is_empty() {
        let port_mappings: Vec<PodmanPortMapping> = req
            .ports
            .iter()
            .map(|p| PodmanPortMapping {
                container_port: Some(p.container),
                host_port: Some(p.host),
                protocol: Some(p.protocol.clone()),
                host_ip: None,
                range: None,
            })
            .collect();
        builder = builder.portmappings(port_mappings);
    }

    // Volume mounts (as bind mounts via the "mounts" field)
    if !req.volumes.is_empty() {
        let mounts: Vec<podman_api::models::ContainerMount> = req
            .volumes
            .iter()
            .map(|v| podman_api::models::ContainerMount {
                destination: Some(v.container_path.clone()),
                source: Some(v.host_path.clone()),
                _type: Some("bind".to_string()),
                options: None,
                gid_mappings: None,
                uid_mappings: None,
            })
            .collect();
        builder = builder.mounts(mounts);
    }

    // Resource limits (memory + CPU)
    if let Some(ref limits) = req.limits {
        let memory = limits.memory_mb.map(|memory_mb| {
            let memory_bytes = (memory_mb as i64) * 1024 * 1024;
            podman_api::models::LinuxMemory {
                disable_oom_killer: None,
                kernel: None,
                kernel_tcp: None,
                limit: Some(memory_bytes),
                reservation: None,
                swap: None,
                swappiness: None,
                use_hierarchy: None,
            }
        });

        let cpu = limits.cpu_cores.map(|cpu_cores| {
            // CPU quota: cores * period (default period is 100000 microseconds)
            let period: u64 = 100_000;
            let quota = (cpu_cores * period as f64) as i64;
            podman_api::models::LinuxCpu {
                cpus: None,
                mems: None,
                period: Some(period),
                quota: Some(quota),
                realtime_period: None,
                realtime_runtime: None,
                shares: None,
            }
        });

        let linux_resources = podman_api::models::LinuxResources {
            block_io: None,
            cpu,
            devices: None,
            hugepage_limits: None,
            memory,
            network: None,
            pids: None,
            rdma: None,
            unified: None,
        };

        builder = builder.resource_limits(linux_resources);
    }

    // Labels — always add neox.managed = true
    let mut labels = req.labels.clone();
    labels.entry("neox.managed".to_string()).or_insert_with(|| "true".to_string());
    
    // Store network speed and disk limits in labels so we can retrieve them later
    if let Some(ref limits) = req.limits {
        if let Some(speed) = limits.network_speed_mbps {
            labels.insert("neox.network.speed_mbps".to_string(), speed.to_string());
        }
        if let Some(disk) = limits.disk_mb {
            labels.insert("neox.disk_mb".to_string(), disk.to_string());
        }
    }
    
    builder = builder.labels(labels.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    // DNS servers from config defaults
    let dns_servers = state.config.defaults.dns.clone();
    if !dns_servers.is_empty() {
        builder = builder.dns_server(dns_servers.iter().map(|s| s.as_str()));
    }

    // Restart policy
    let restart_policy_str = req
        .restart_policy
        .as_deref()
        .unwrap_or(&state.config.defaults.restart_policy);

    let restart_policy = match restart_policy_str {
        "always" => ContainerRestartPolicy::Always,
        "on-failure" => ContainerRestartPolicy::OnFailure,
        "unless-stopped" => ContainerRestartPolicy::UnlessStopped,
        "no" | "none" => ContainerRestartPolicy::No,
        _ => ContainerRestartPolicy::Always,
    };
    builder = builder.restart_policy(restart_policy);

    // Entrypoint override
    if let Some(ref entrypoint) = req.entrypoint {
        if !entrypoint.is_empty() {
            tracing::info!("🛠️ Overriding entrypoint to: {:?}", entrypoint);
            builder = builder.entrypoint(entrypoint.iter().map(|s| s.as_str()));
        }
    }

    // Command override
    if !req.command.is_empty() {
        tracing::info!("🛠️ Overriding command to: {:?}", req.command);
        builder = builder.command(req.command.iter().map(|s| s.as_str()));
    }

    // Build and create with a temporary name
    let temp_name = format!("{}-tmp-{}", req.name, uuid::Uuid::new_v4().to_string()[..8].to_string());
    let opts = builder.name(&temp_name).build();
    
    let container = state
        .podman
        .containers()
        .create(&opts)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to create container: {}", e)))?;

    // Rename to final format: name-short_id
    let short_id = &container.id[..12];
    let final_name = format!("{}-{}", req.name, short_id);
    
    tracing::info!("🏷️ Renaming container {} to {}", container.id, final_name);
    
    state.podman.containers().get(&container.id).rename(&final_name).await
        .map_err(|e| AppError::Podman(format!("Failed to rename container: {}", e)))?;

    let container_id = container.id.clone();

    // Build port mapping response
    let ports: Vec<PortMapping> = req.ports;

    // Build limits response
    let limits = req.limits;

    Ok(ContainerResponse {
        id: container_id,
        name: req.name,
        image: req.image,
        status: "created".to_string(),
        created_at: Some(crate::time_utils::now_rfc3339()),
        ports,
        limits,
        labels,
    })
}

/// Gets detailed information about a single container by ID or name.
pub async fn inspect_container(
    state: &Arc<AppState>,
    id: &str,
) -> Result<ContainerResponse, AppError> {
    let container = state.podman.containers().get(id);

    let inspect = container
        .inspect()
        .await
        .map_err(|e| AppError::NotFound(format!("Container '{}' not found: {}", id, e)))?;

    let name = inspect
        .name
        .unwrap_or_default()
        .trim_start_matches('/')
        .to_string();

    let image = inspect.image_name.unwrap_or_default();

    let status = inspect
        .state
        .as_ref()
        .and_then(|s| s.status.as_deref())
        .unwrap_or("unknown")
        .to_string();

    let created_at = inspect.created.map(|t| t.to_rfc3339());

    // Extract port mappings from host config
    let ports: Vec<PortMapping> = inspect
        .host_config
        .as_ref()
        .and_then(|hc| hc.port_bindings.as_ref())
        .map(|bindings| {
            bindings
                .iter()
                .flat_map(|(container_port_proto, host_bindings)| {
                    // container_port_proto is like "25565/tcp"
                    let parts: Vec<&str> = container_port_proto.split('/').collect();
                    let container_port = parts
                        .first()
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(0);
                    let protocol = parts.get(1).unwrap_or(&"tcp").to_string();

                    host_bindings
                        .as_ref()
                        .map(|hbs| {
                            hbs.iter()
                                .map(|hb| {
                                    let host_port = hb
                                        .host_port
                                        .as_deref()
                                        .and_then(|p| p.parse::<u16>().ok())
                                        .unwrap_or(0);
                                    PortMapping {
                                        host: host_port,
                                        container: container_port,
                                        protocol: protocol.clone(),
                                    }
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract memory limit
    let memory_limit = inspect
        .host_config
        .as_ref()
        .and_then(|hc| hc.memory)
        .filter(|&m| m > 0)
        .map(|m| (m / 1024 / 1024) as u64);

    // Extract CPU from NanoCpus
    let cpu_cores = inspect
        .host_config
        .as_ref()
        .and_then(|hc| hc.nano_cpus)
        .filter(|&c| c > 0)
        .map(|c| c as f64 / 1_000_000_000.0);

    let labels = inspect
        .config
        .as_ref()
        .and_then(|c| c.labels.clone())
        .unwrap_or_default();

    // Extract network and disk limits from labels
    let network_speed_mbps = labels
        .get("neox.network.speed_mbps")
        .and_then(|v| v.parse::<u64>().ok());
    
    let disk_mb = labels
        .get("neox.disk_mb")
        .and_then(|v| v.parse::<u64>().ok());

    let limits = if memory_limit.is_some() || cpu_cores.is_some() || network_speed_mbps.is_some() || disk_mb.is_some() {
        Some(ResourceLimits {
            memory_mb: memory_limit,
            cpu_cores,
            disk_mb,
            network_speed_mbps,
        })
    } else {
        None
    };

    Ok(ContainerResponse {
        id: inspect.id.unwrap_or_default(),
        name,
        image,
        status,
        created_at,
        ports,
        limits,
        labels,
    })
}

/// Lists all containers (running and stopped).
pub async fn list_containers(
    state: &Arc<AppState>,
) -> Result<Vec<serde_json::Value>, AppError> {
    let opts = ContainerListOpts::builder().all(true).build();
    let containers = state
        .podman
        .containers()
        .list(&opts)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to list containers: {}", e)))?;

    let list: Vec<serde_json::Value> = containers
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "names": c.names,
                "image": c.image,
                "state": c.state,
                "status": c.status,
                "created": c.created,
                "ports": c.ports,
                "labels": c.labels,
            })
        })
        .collect();

    Ok(list)
}

/// Deletes a container by ID or name.
pub async fn delete_container(
    state: &Arc<AppState>,
    id: &str,
    force: bool,
    remove_volumes: bool,
) -> Result<(), AppError> {
    let container = state.podman.containers().get(id);

    let opts = ContainerDeleteOpts::builder()
        .force(force)
        .volumes(remove_volumes)
        .build();

    container
        .delete(&opts)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to delete container '{}': {}", id, e)))?;

    Ok(())
}

/// Starts a container.
pub async fn start_container(
    state: &Arc<AppState>,
    id: &str,
) -> Result<(), AppError> {
    let container = state.podman.containers().get(id);
    container
        .start(None)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to start container '{}': {}", id, e)))?;
        
    // Apply network limit if present
    if let Ok(inspect) = inspect_container(state, id).await {
        if let Some(limits) = inspect.limits {
            if let Some(speed) = limits.network_speed_mbps {
                if speed > 0 {
                    if let Ok(raw_inspect) = container.inspect().await {
                        if let Some(c_state) = raw_inspect.state {
                            if let Some(pid) = c_state.pid {
                                if pid > 0 {
                                    let _ = std::process::Command::new("nsenter")
                                        .args(&[
                                            "-t", &pid.to_string(),
                                            "-n",
                                            "tc", "qdisc", "replace", "dev", "eth0", "root", "tbf",
                                            "rate", &format!("{}mbit", speed),
                                            "burst", "32kbit",
                                            "latency", "400ms"
                                        ])
                                        .output();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
        
    Ok(())
}

/// Stops a container with optional timeout.
pub async fn stop_container(
    state: &Arc<AppState>,
    id: &str,
    timeout: Option<u64>,
) -> Result<(), AppError> {
    let container = state.podman.containers().get(id);

    let mut opts_builder = ContainerStopOpts::builder();
    if let Some(t) = timeout {
        opts_builder = opts_builder.timeout(t as usize);
    }
    let opts = opts_builder.build();

    container
        .stop(&opts)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to stop container '{}': {}", id, e)))?;
    Ok(())
}

/// Restarts a container.
pub async fn restart_container(
    state: &Arc<AppState>,
    id: &str,
) -> Result<(), AppError> {
    let container = state.podman.containers().get(id);
    container
        .restart()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to restart container '{}': {}", id, e)))?;
    Ok(())
}

/// Kills a container with SIGKILL.
pub async fn kill_container(
    state: &Arc<AppState>,
    id: &str,
) -> Result<(), AppError> {
    let container = state.podman.containers().get(id);
    container
        .kill()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to kill container '{}': {}", id, e)))?;
    Ok(())
}

/// Fetches logs from a container.
pub async fn get_container_logs(
    state: &Arc<AppState>,
    id: &str,
    tail: Option<usize>,
) -> Result<String, AppError> {
    use futures_util::StreamExt;
    use podman_api::opts::ContainerLogsOpts;

    let container = state.podman.containers().get(id);
    let mut opts_builder = ContainerLogsOpts::builder()
        .stdout(true)
        .stderr(true);
        
    if let Some(t) = tail {
        opts_builder = opts_builder.tail(t.to_string());
    }
    
    let opts = opts_builder.build();

    let mut logs = container.logs(&opts);
    let mut output = String::new();

    while let Some(chunk) = logs.next().await {
        match chunk {
            Ok(chunk) => {
                let data = match chunk {
                    podman_api::conn::TtyChunk::StdOut(d) => d,
                    podman_api::conn::TtyChunk::StdErr(d) => d,
                    podman_api::conn::TtyChunk::StdIn(d) => d,
                };
                let text = String::from_utf8_lossy(&data);
                output.push_str(&text);
            }
            Err(e) => return Err(AppError::Podman(format!("Error reading logs: {}", e))),
        }
    }

    Ok(output)
}

/// Fetches logs from a pod (all containers).
pub async fn get_pod_logs(
    _state: &Arc<AppState>,
    _id: &str,
    _tail: Option<usize>,
) -> Result<String, AppError> {
    Err(AppError::Podman("Pod logs stream not natively supported by Podman API".to_string()))
}

// ─── Volumes ───────────────────────────────────────────

pub async fn list_volumes(state: &Arc<AppState>) -> Result<Vec<VolumeResponse>, AppError> {
    let opts = VolumeListOpts::builder().build();
    let volumes = state.podman.volumes().list(&opts).await
        .map_err(|e| AppError::Podman(format!("Failed to list volumes: {}", e)))?;

    let list = volumes.iter().map(|v| VolumeResponse {
        name:        v.name.clone().unwrap_or_default(),
        driver:      v.driver.clone().unwrap_or_default(),
        mountpoint:  v.mountpoint.clone().unwrap_or_default(),
        created_at:  v.created_at.clone().map(|dt| dt.to_rfc3339()),
        labels:      v.labels.clone().unwrap_or_default(),
        options:     v.options.clone().unwrap_or_default(),
    }).collect();

    Ok(list)
}

pub async fn create_volume(state: &Arc<AppState>, req: CreateVolumeRequest) -> Result<VolumeResponse, AppError> {
    let mut builder = VolumeCreateOpts::builder().name(&req.name);
    
    if let Some(ref driver) = req.driver {
        builder = builder.driver(driver);
    }
    
    if let Some(ref labels) = req.labels {
        builder = builder.labels(labels.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }
    
    if let Some(ref options) = req.options {
        builder = builder.options(options.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    let opts = builder.build();
    let volume = state.podman.volumes().create(&opts).await
        .map_err(|e| AppError::Podman(format!("Failed to create volume: {}", e)))?;

    Ok(VolumeResponse {
        name:        volume.name.clone().unwrap_or_default(),
        driver:      volume.driver.clone().unwrap_or_default(),
        mountpoint:  volume.mountpoint.clone().unwrap_or_default(),
        created_at:  volume.created_at.clone().map(|dt| dt.to_rfc3339()),
        labels:      volume.labels.clone().unwrap_or_default(),
        options:     volume.options.clone().unwrap_or_default(),
    })
}

pub async fn delete_volume(state: &Arc<AppState>, name: &str, _force: bool) -> Result<(), AppError> {
    state.podman.volumes().get(name).remove().await
        .map_err(|e| AppError::Podman(format!("Failed to delete volume '{}': {}", name, e)))?;
    Ok(())
}

pub async fn inspect_volume(state: &Arc<AppState>, name: &str) -> Result<VolumeResponse, AppError> {
    let volume = state.podman.volumes().get(name).inspect().await
        .map_err(|e| AppError::Podman(format!("Failed to inspect volume '{}': {}", name, e)))?;

    Ok(VolumeResponse {
        name:        volume.name.clone().unwrap_or_default(),
        driver:      volume.driver.clone().unwrap_or_default(),
        mountpoint:  volume.mountpoint.clone().unwrap_or_default(),
        created_at:  volume.created_at.clone().map(|dt| dt.to_rfc3339()),
        labels:      volume.labels.clone().unwrap_or_default(),
        options:     volume.options.clone().unwrap_or_default(),
    })
}
