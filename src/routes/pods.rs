use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use podman_api::models::{
    ContainerMount, InspectPodContainerInfo, InspectPodData,
    LinuxMemory, LinuxResources,
    PortMapping as PodmanPortMapping,
};
use podman_api::opts::{
    ContainerCreateOpts, PodCreateOpts, PodListOpts,
};

use crate::error::AppError;
use crate::models::pod::{
    AddContainerToPodRequest, CreatePodRequest, DeletePodQuery, GenerateKubeQuery,
    PodContainerInfo, PodResponse, PodSummary,
};
use crate::AppState;

// ─── List Pods ───────────────────────────────────────────────────────────────

/// GET /api/pods
/// Lists all pods managed by Podman on this node.
pub async fn list_pods(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, AppError> {
    let pods = state.podman.pods()
        .list(&PodListOpts::builder().build())
        .await
        .map_err(|e| AppError::Podman(format!("Failed to list pods: {}", e)))?;

    let summaries: Vec<PodSummary> = pods.iter().map(|p| {
        PodSummary {
            id: p.id.clone().unwrap_or_default(),
            name: p.name.clone(),
            status: p.status.clone(),
            created: p.created.map(|t| t.to_rfc3339()),
            num_containers: p.containers.as_ref().map(|c| c.len() as i64),
            infra_id: p.infra_id.clone(),
            labels: p.labels.clone(),
        }
    }).collect();

    let total = summaries.len();

    Ok(Json(json!({
        "pods": summaries,
        "total": total,
    })))
}

// ─── Create Pod ──────────────────────────────────────────────────────────────

/// POST /api/pods
/// Creates a new pod, optionally with a tun2socks proxy sidecar and game server containers.
pub async fn create_pod(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePodRequest>,
) -> Result<Json<Value>, AppError> {
    // Validate
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Pod name is required".into()));
    }

    tracing::info!("🏗️ Creating pod '{}'", req.name);

    // Collect all port mappings from all containers for the pod-level config
    let mut all_ports: Vec<PodmanPortMapping> = Vec::new();
    for ctr in &req.containers {
        for p in &ctr.ports {
            all_ports.push(PodmanPortMapping {
                container_port: Some(p.container as u16),
                host_port: Some(p.host as u16),
                protocol: Some(p.protocol.clone()),
                host_ip: None,
                range: None,
            });
        }
    }

    // Build PodCreateOpts
    let mut pod_builder = PodCreateOpts::builder()
        .name(&req.name)
        .labels(req.labels.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    if !all_ports.is_empty() {
        pod_builder = pod_builder.portmappings(all_ports);
    }

    if let Some(hostname) = &req.hostname {
        pod_builder = pod_builder.hostname(hostname.as_str());
    }

    if !req.dns_servers.is_empty() {
        pod_builder = pod_builder.dns_server(req.dns_servers.iter().map(|s| s.as_str()));
    }

    let pod_opts = pod_builder.build();

    let pod = state.podman.pods()
        .create(&pod_opts)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to create pod '{}': {}", req.name, e)))?;

    let pod_id = pod.id().to_string();
    tracing::info!("✅ Pod '{}' created: {}", req.name, pod_id);

    // Step 2: If proxy is enabled, create tun2socks sidecar container
    let proxy_enabled = req.proxy.as_ref().map_or(false, |p| p.enabled);

    if proxy_enabled {
        if let Some(proxy) = &req.proxy {
            let socks5_url = proxy.socks5_url.as_deref()
                .ok_or_else(|| AppError::BadRequest("socks5_url is required when proxy is enabled".into()))?;

            let proxy_image = proxy.image.as_deref()
                .unwrap_or("docker.io/xjasonlyu/tun2socks:latest");
            let proxy_dns = proxy.dns.as_deref().unwrap_or("1.1.1.1");
            let loglevel = proxy.loglevel.as_deref().unwrap_or("info");

            let sidecar_name = format!("{}-tun2socks", req.name);

            tracing::info!("🔌 Creating tun2socks sidecar '{}'", sidecar_name);

            // Build environment for tun2socks
            let mut tun_env: HashMap<&str, &str> = HashMap::new();
            tun_env.insert("PROXY", socks5_url);
            tun_env.insert("LOGLEVEL", loglevel);
            tun_env.insert("TUN_DNS", proxy_dns);

            // Also include any extra env from the proxy config
            for (k, v) in &proxy.env {
                tun_env.insert(k.as_str(), v.as_str());
            }

            let sidecar_opts = ContainerCreateOpts::builder()
                .name(&sidecar_name)
                .image(proxy_image)
                .env(tun_env)
                .pod(req.name.as_str())
                .privileged(true) // tun2socks needs NET_ADMIN + TUN device
                .labels([
                    ("neox.role", "proxy-sidecar"),
                    ("neox.proxy.type", "tun2socks"),
                    ("neox.pod", req.name.as_str()),
                ])
                .build();

            state.podman.containers()
                .create(&sidecar_opts)
                .await
                .map_err(|e| AppError::Podman(format!(
                    "Failed to create tun2socks sidecar '{}': {}", sidecar_name, e
                )))?;

            tracing::info!("✅ Tun2socks sidecar '{}' created", sidecar_name);
        }
    }

    // Step 3: Create user-specified containers inside the pod
    for ctr_spec in &req.containers {
        if ctr_spec.name.is_empty() {
            return Err(AppError::BadRequest("Container name is required".into()));
        }
        if ctr_spec.image.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Image is required for container '{}'", ctr_spec.name
            )));
        }

        let ctr_name = format!("{}-{}", req.name, ctr_spec.name);
        tracing::info!("📦 Creating container '{}' in pod '{}'", ctr_name, req.name);

        let env_refs: HashMap<&str, &str> = ctr_spec.env.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let mut label_map: HashMap<&str, &str> = ctr_spec.labels.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        label_map.insert("neox.role", "main");
        label_map.insert("neox.pod", req.name.as_str());

        let mut ctr_builder = ContainerCreateOpts::builder()
            .name(&ctr_name)
            .image(&ctr_spec.image)
            .env(env_refs)
            .pod(req.name.as_str())
            .labels(label_map);

        // Apply resource limits (memory)
        if let Some(limits) = &ctr_spec.limits {
            if let Some(mem) = limits.memory_mb {
                let mem_bytes = (mem * 1024 * 1024) as i64;
                ctr_builder = ctr_builder.resource_limits(
                    LinuxResources {
                        memory: Some(LinuxMemory {
                            limit: Some(mem_bytes),
                            reservation: None,
                            swap: None,
                            kernel: None,
                            kernel_tcp: None,
                            swappiness: None,
                            disable_oom_killer: None,
                            use_hierarchy: None,
                        }),
                        cpu: None,
                        pids: None,
                        block_io: None,
                        hugepage_limits: None,
                        network: None,
                        devices: None,
                        rdma: None,
                        unified: None,
                    }
                );
            }
        }

        // Apply volume mounts using ContainerMount (the type expected by mounts())
        for vol in &ctr_spec.volumes {
            ctr_builder = ctr_builder.mounts([
                ContainerMount {
                    destination: Some(vol.container_path.clone()),
                    source: Some(vol.host_path.clone()),
                    _type: Some("bind".to_string()),
                    options: Some(vec!["rbind".to_string()]),
                    uid_mappings: None,
                    gid_mappings: None,
                }
            ]);
        }

        // Apply command override
        if !ctr_spec.command.is_empty() {
            ctr_builder = ctr_builder.command(ctr_spec.command.iter().map(|s| s.as_str()));
        }

        let ctr_opts = ctr_builder.build();

        state.podman.containers()
            .create(&ctr_opts)
            .await
            .map_err(|e| AppError::Podman(format!(
                "Failed to create container '{}' in pod '{}': {}", ctr_name, req.name, e
            )))?;

        tracing::info!("✅ Container '{}' created in pod '{}'", ctr_name, req.name);
    }

    // Step 4: Start the pod (all containers start together)
    tracing::info!("🚀 Starting pod '{}'", req.name);
    pod.start()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to start pod '{}': {}", req.name, e)))?;
    tracing::info!("✅ Pod '{}' started successfully", req.name);

    // Return pod info
    let inspect = pod.inspect()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to inspect pod: {}", e)))?;

    let response = build_pod_response(&inspect, proxy_enabled);

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Inspect Pod ─────────────────────────────────────────────────────────────

/// GET /api/pods/:id
/// Gets detailed information about a pod.
pub async fn get_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let pod = state.podman.pods().get(&id);

    let inspect = pod.inspect()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to inspect pod '{}': {}", id, e)))?;

    // Detect if proxy is enabled by checking container names for tun2socks pattern
    let proxy_enabled = inspect.containers.as_ref()
        .map(|ctrs| {
            ctrs.iter().any(|c| {
                c.name.as_deref().unwrap_or("").contains("tun2socks")
            })
        })
        .unwrap_or(false);

    let response = build_pod_response(&inspect, proxy_enabled);

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Delete Pod ──────────────────────────────────────────────────────────────

/// DELETE /api/pods/:id
/// Deletes a pod and all its containers.
pub async fn delete_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<DeletePodQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🗑️ Deleting pod '{}' (force: {})", id, query.force);

    let pod = state.podman.pods().get(&id);

    if query.force {
        pod.remove()
            .await
            .map_err(|e| AppError::Podman(format!("Failed to force-remove pod '{}': {}", id, e)))?;
    } else {
        pod.delete()
            .await
            .map_err(|e| AppError::Podman(format!("Failed to delete pod '{}': {}", id, e)))?;
    }

    tracing::info!("✅ Pod '{}' deleted", id);

    Ok(Json(json!({
        "success": true,
        "message": format!("Pod '{}' deleted successfully", id),
        "pod_id": id,
    })))
}

// ─── Start Pod ───────────────────────────────────────────────────────────────

/// POST /api/pods/:id/start
/// Starts a pod and all its containers.
pub async fn start_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("▶️ Starting pod '{}'", id);

    state.podman.pods().get(&id)
        .start()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to start pod '{}': {}", id, e)))?;

    tracing::info!("✅ Pod '{}' started", id);

    Ok(Json(json!({
        "success": true,
        "message": format!("Pod '{}' started successfully", id),
        "pod_id": id,
    })))
}

// ─── Stop Pod ────────────────────────────────────────────────────────────────

/// POST /api/pods/:id/stop
/// Stops a pod and all its containers.
pub async fn stop_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("⏹️ Stopping pod '{}'", id);

    state.podman.pods().get(&id)
        .stop()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to stop pod '{}': {}", id, e)))?;

    tracing::info!("✅ Pod '{}' stopped", id);

    Ok(Json(json!({
        "success": true,
        "message": format!("Pod '{}' stopped successfully", id),
        "pod_id": id,
    })))
}

// ─── Restart Pod ─────────────────────────────────────────────────────────────

/// POST /api/pods/:id/restart
/// Restarts a pod and all its containers.
pub async fn restart_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🔄 Restarting pod '{}'", id);

    state.podman.pods().get(&id)
        .restart()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to restart pod '{}': {}", id, e)))?;

    tracing::info!("✅ Pod '{}' restarted", id);

    Ok(Json(json!({
        "success": true,
        "message": format!("Pod '{}' restarted successfully", id),
        "pod_id": id,
    })))
}

// ─── List Pod Containers ─────────────────────────────────────────────────────

/// GET /api/pods/:id/containers
/// Lists all containers inside a pod.
pub async fn list_pod_containers(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let pod = state.podman.pods().get(&id);

    let inspect = pod.inspect()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to inspect pod '{}': {}", id, e)))?;

    let containers = extract_containers(&inspect.containers);
    let total = containers.len();

    Ok(Json(json!({
        "pod_id": id,
        "containers": containers,
        "total": total,
    })))
}

// ─── Add Container to Pod ────────────────────────────────────────────────────

/// POST /api/pods/:id/containers
/// Adds a new container to an existing pod.
pub async fn add_container_to_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddContainerToPodRequest>,
) -> Result<Json<Value>, AppError> {
    let ctr = &req.container;

    if ctr.name.is_empty() {
        return Err(AppError::BadRequest("Container name is required".into()));
    }
    if ctr.image.is_empty() {
        return Err(AppError::BadRequest("Image is required".into()));
    }

    let ctr_name = format!("{}-{}", id, ctr.name);
    tracing::info!("📦 Adding container '{}' to pod '{}'", ctr_name, id);

    let env_refs: HashMap<&str, &str> = ctr.env.iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let mut label_map: HashMap<&str, &str> = ctr.labels.iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    label_map.insert("neox.role", "main");
    label_map.insert("neox.pod", &id);

    let mut ctr_builder = ContainerCreateOpts::builder()
        .name(&ctr_name)
        .image(&ctr.image)
        .env(env_refs)
        .pod(id.as_str())
        .labels(label_map);

    // Volume mounts
    for vol in &ctr.volumes {
        ctr_builder = ctr_builder.mounts([
            ContainerMount {
                destination: Some(vol.container_path.clone()),
                source: Some(vol.host_path.clone()),
                _type: Some("bind".to_string()),
                options: Some(vec!["rbind".to_string()]),
                uid_mappings: None,
                gid_mappings: None,
            }
        ]);
    }

    // Command override
    if !ctr.command.is_empty() {
        ctr_builder = ctr_builder.command(ctr.command.iter().map(|s| s.as_str()));
    }

    let ctr_opts = ctr_builder.build();

    let created = state.podman.containers()
        .create(&ctr_opts)
        .await
        .map_err(|e| AppError::Podman(format!(
            "Failed to create container '{}' in pod '{}': {}", ctr_name, id, e
        )))?;

    let container_id = created.id.clone();
    tracing::info!("✅ Container '{}' added to pod '{}' (id: {})", ctr_name, id, container_id);

    // Start the newly added container via the Podman container handle
    state.podman.containers().get(&container_id)
        .start(None)
        .await
        .map_err(|e| AppError::Podman(format!(
            "Failed to start container '{}': {}", ctr_name, e
        )))?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Container '{}' added to pod '{}'", ctr_name, id),
        "container_id": container_id,
        "container_name": ctr_name,
        "pod_id": id,
    })))
}

// ─── Generate Kube YAML ──────────────────────────────────────────────────────

/// GET /api/pods/:id/kube
/// Generates Kubernetes YAML for an existing pod.
pub async fn generate_kube_yaml(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<GenerateKubeQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("📄 Generating Kube YAML for pod '{}'", id);

    let pod = state.podman.pods().get(&id);

    let yaml = pod.generate_kube_yaml(query.service)
        .await
        .map_err(|e| AppError::Podman(format!(
            "Failed to generate Kube YAML for pod '{}': {}", id, e
        )))?;

    Ok(Json(json!({
        "pod_id": id,
        "yaml": yaml,
        "service_included": query.service,
    })))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract container info from an Option<Vec<InspectPodContainerInfo>>
fn extract_containers(
    ctrs: &Option<Vec<InspectPodContainerInfo>>,
) -> Vec<PodContainerInfo> {
    ctrs.as_ref()
        .map(|arr| {
            arr.iter().map(|c| PodContainerInfo {
                id: c.id.clone().unwrap_or_default(),
                name: c.name.clone().unwrap_or_default(),
                status: c.state.clone().unwrap_or_else(|| "unknown".to_string()),
            }).collect()
        })
        .unwrap_or_default()
}

/// Build a PodResponse from an InspectPodData struct.
fn build_pod_response(inspect: &InspectPodData, proxy_enabled: bool) -> PodResponse {
    let id = inspect.id.clone().unwrap_or_default();
    let name = inspect.name.clone().unwrap_or_default();
    let status = inspect.state.clone().unwrap_or_else(|| "unknown".to_string());
    let created_at = inspect.created.map(|t| t.to_rfc3339());
    let hostname = inspect.hostname.clone();
    let labels = inspect.labels.clone().unwrap_or_default();
    let infra_id = inspect.infra_container_id.clone();
    let containers = extract_containers(&inspect.containers);

    PodResponse {
        id,
        name,
        status,
        created_at,
        hostname,
        labels,
        containers,
        proxy_enabled,
        infra_id,
    }
}
