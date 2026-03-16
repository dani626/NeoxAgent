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
    ContainerCreateOpts, ContainerDeleteOpts, PodCreateOpts, PodListOpts,
};

use crate::error::AppError;
use crate::models::pod::{
    AddContainerToPodRequest, CreatePodRequest, DeletePodQuery, GenerateKubeQuery,
    PodContainerInfo, PodResponse, PodSummary, RenamePodRequest, UpdateProxyRequest,
};
use crate::models::container::LogsQuery;
use crate::AppState;

// ─── List Pods ───────────────────────────────────────────────────────────────

/// GET /api/pods
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

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Builds the bash startup script for the hev-socks5-tproxy sidecar.
/// 
/// Security properties:
/// - Installs iptables DROP rules on mangle PREROUTING + OUTPUT **before** any
///   hev configuration, so zero packets can escape during the swap window.
/// - DROP rules are removed only right before `exec hev-socks5-tproxy`, so
///   the window with no interception is at most a few milliseconds.
/// - Skips DROP on loopback (127/8) so the container can still communicate
///   internally during startup.
fn build_tproxy_script(
    proxy_host: &str,
    proxy_port: &str,
    auth_yaml: &str,
    dns_server: &str,
    log_level: &str,
) -> String {
    format!(r#"
set -e

# ── OPTION C: Drop-all gap protection ────────────────────────────────────────
# Block ALL non-loopback traffic immediately so no packets escape while we
# tear down the old sidecar rules and set up the new ones.
iptables -t mangle -N NEOX_GUARD 2>/dev/null || true
iptables -t mangle -F NEOX_GUARD
iptables -t mangle -A NEOX_GUARD -i lo -j RETURN
iptables -t mangle -A NEOX_GUARD -j DROP
# Insert at position 1 to take priority over any stale rules
iptables -t mangle -I PREROUTING 1 -j NEOX_GUARD
iptables -t mangle -I OUTPUT 1 -j NEOX_GUARD

# ── Pre-requisites ────────────────────────────────────────────────────────────
if ! command -v iptables >/dev/null 2>&1; then
  apt-get update -qq && apt-get install -yq iptables iproute2 ca-certificates >/dev/null 2>&1
fi

# ── hev-socks5-tproxy config ──────────────────────────────────────────────────
mkdir -p /etc/hev
cat > /etc/hev/tproxy.yml << 'HEVEOF'
main:
  workers: 1

socks5:
  port: {proxy_port}
  address: '{proxy_host}'
  udp: 'udp'
  mark: 438
{auth_yaml}

tcp:
  port: 1088
  address: '0.0.0.0'

udp:
  port: 1088
  address: '0.0.0.0'

misc:
  task-stack-size: 131072
  connect-timeout: 5000
  tcp-read-write-timeout: 300000
  udp-read-write-timeout: 60000
  log-file: stderr
  log-level: {log_level}
HEVEOF

# ── DNS redirect ──────────────────────────────────────────────────────────────
iptables -t nat -N HEV_DNS 2>/dev/null || true
iptables -t nat -F HEV_DNS
iptables -t nat -A HEV_DNS -p udp --dport 53 -j DNAT --to-destination {dns_server}:53
iptables -t nat -A HEV_DNS -p tcp --dport 53 -j DNAT --to-destination {dns_server}:53
iptables -t nat -A PREROUTING -j HEV_DNS
iptables -t nat -A OUTPUT -j HEV_DNS

# ── TPROXY rules ──────────────────────────────────────────────────────────────
iptables -t mangle -N HEV_TPROXY 2>/dev/null || true
iptables -t mangle -F HEV_TPROXY
iptables -t mangle -A HEV_TPROXY -m mark --mark 0x438 -j RETURN
iptables -t mangle -A HEV_TPROXY -d 0.0.0.0/8    -j RETURN
iptables -t mangle -A HEV_TPROXY -d 10.0.0.0/8   -j RETURN
iptables -t mangle -A HEV_TPROXY -d 127.0.0.0/8  -j RETURN
iptables -t mangle -A HEV_TPROXY -d 169.254.0.0/16 -j RETURN
iptables -t mangle -A HEV_TPROXY -d 172.16.0.0/12 -j RETURN
iptables -t mangle -A HEV_TPROXY -d 192.168.0.0/16 -j RETURN
iptables -t mangle -A HEV_TPROXY -d 224.0.0.0/4  -j RETURN
iptables -t mangle -A HEV_TPROXY -d 240.0.0.0/4  -j RETURN
iptables -t mangle -A HEV_TPROXY -d {proxy_host}  -j RETURN
iptables -t mangle -A HEV_TPROXY -p tcp -j TPROXY --on-port 1088 --tproxy-mark 0x440
iptables -t mangle -A HEV_TPROXY -p udp -j TPROXY --on-port 1088 --tproxy-mark 0x440
iptables -t mangle -A PREROUTING -j HEV_TPROXY

iptables -t mangle -N HEV_OUTPUT 2>/dev/null || true
iptables -t mangle -F HEV_OUTPUT
iptables -t mangle -A HEV_OUTPUT -m mark --mark 0x438 -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 0.0.0.0/8    -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 10.0.0.0/8   -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 127.0.0.0/8  -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 169.254.0.0/16 -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 172.16.0.0/12 -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 192.168.0.0/16 -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 224.0.0.0/4  -j RETURN
iptables -t mangle -A HEV_OUTPUT -d 240.0.0.0/4  -j RETURN
iptables -t mangle -A HEV_OUTPUT -d {proxy_host}  -j RETURN
iptables -t mangle -A HEV_OUTPUT -p tcp -j MARK --set-mark 0x440
iptables -t mangle -A HEV_OUTPUT -p udp -j MARK --set-mark 0x440
iptables -t mangle -A OUTPUT -j HEV_OUTPUT

# ── Policy routing ────────────────────────────────────────────────────────────
ip rule add fwmark 0x440 table 100 2>/dev/null || true
ip route add local default dev lo table 100 2>/dev/null || true

# ── OPTION C: Lift drop-all guard — hev is about to take over ─────────────────
# All TPROXY rules are now in place. Remove the guard so hev can intercept.
iptables -t mangle -D PREROUTING -j NEOX_GUARD 2>/dev/null || true
iptables -t mangle -D OUTPUT     -j NEOX_GUARD 2>/dev/null || true
iptables -t mangle -F NEOX_GUARD 2>/dev/null || true
iptables -t mangle -X NEOX_GUARD 2>/dev/null || true

echo "Starting hev-socks5-tproxy -> {proxy_host}:{proxy_port}"
exec /usr/local/bin/hev-socks5-tproxy /etc/hev/tproxy.yml
"#,
        proxy_port = proxy_port,
        proxy_host = proxy_host,
        auth_yaml = auth_yaml,
        dns_server = dns_server,
        log_level = log_level,
    )
}

/// Parses a socks5://[user:pass@]host:port URL into its components.
fn parse_socks5_url(socks5_url: &str) -> (Option<String>, Option<String>, String, String) {
    let url_str = socks5_url.trim_start_matches("socks5://");
    let (auth_part, host_part) = if let Some((auth, rest)) = url_str.split_once('@') {
        (Some(auth), rest)
    } else {
        (None, url_str)
    };
    let (proxy_host, proxy_port) = match host_part.split_once(':') {
        Some((h, p)) => (h.to_string(), p.to_string()),
        None => (host_part.to_string(), "1080".to_string()),
    };
    let (proxy_user, proxy_pass) = if let Some(auth) = auth_part {
        match auth.split_once(':') {
            Some((u, p)) => (Some(u.to_string()), Some(p.to_string())),
            None => (Some(auth.to_string()), None),
        }
    } else {
        (None, None)
    };
    (proxy_user, proxy_pass, proxy_host, proxy_port)
}

/// Writes neox.proxy.url label to the pod via `podman pod label`.
/// This is the Opción B persistence mechanism.
fn set_pod_proxy_label(pod_name: &str, socks5_url: &str) {
    let label = format!("neox.proxy.url={}", socks5_url);
    let result = std::process::Command::new("podman")
        .args(["pod", "label", pod_name, &label])
        .output();
    match result {
        Ok(o) if o.status.success() => {
            tracing::info!("✅ Set neox.proxy.url label on pod '{}'", pod_name);
        }
        Ok(o) => {
            tracing::warn!(
                "⚠️ podman pod label failed for '{}': {}",
                pod_name,
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(e) => {
            tracing::warn!("⚠️ Failed to execute podman pod label: {}", e);
        }
    }
}

/// Removes neox.proxy.url label from the pod.
fn remove_pod_proxy_label(pod_name: &str) {
    let result = std::process::Command::new("podman")
        .args(["pod", "label", "--delete", "neox.proxy.url", pod_name])
        .output();
    if let Err(e) = result {
        tracing::warn!("⚠️ Failed to remove neox.proxy.url label from pod '{}': {}", pod_name, e);
    }
}

// ─── List Pods ───────────────────────────────────────────────────────────────

/// GET /api/pods
/// Lists all pods managed by Podman on this node.
pub async fn list_pods_handler(
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
pub async fn create_pod(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePodRequest>,
) -> Result<Json<Value>, AppError> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Pod name is required".into()));
    }

    tracing::info!("🏗️ Creating pod '{}'", req.name);

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

    let proxy_enabled = req.proxy.as_ref().map_or(false, |p| p.enabled);

    if proxy_enabled {
        if let Some(proxy) = &req.proxy {
            let socks5_url = proxy.socks5_url.as_deref()
                .ok_or_else(|| AppError::BadRequest("socks5_url is required when proxy is enabled".into()))?;

            let (proxy_user, proxy_pass, proxy_host, proxy_port) = parse_socks5_url(socks5_url);

            let proxy_image = proxy.image.as_deref()
                .unwrap_or("localhost/neox-tproxy-sidecar:latest");
            let log_level = proxy.loglevel.as_deref().unwrap_or("warn");
            let dns_server = proxy.dns.as_deref().unwrap_or("8.8.8.8");

            let sidecar_name = format!("{}-hev-tproxy", req.name);

            tracing::info!(
                "🔌 Creating hev-socks5-tproxy sidecar '{}' → {}:{} (auth: {})",
                sidecar_name, proxy_host, proxy_port,
                if proxy_user.is_some() { "yes" } else { "no" }
            );

            let auth_yaml = match (&proxy_user, &proxy_pass) {
                (Some(user), Some(pass)) => format!("  username: '{}'\n  password: '{}'", user, pass),
                _ => String::new(),
            };

            let script = build_tproxy_script(&proxy_host, &proxy_port, &auth_yaml, dns_server, log_level);

            let sidecar_opts = ContainerCreateOpts::builder()
                .name(&sidecar_name)
                .image(proxy_image)
                .pod(req.name.as_str())
                .privileged(true)
                .mounts(vec![
                    ContainerMount {
                        destination: Some("/usr/local/bin/hev-socks5-tproxy".to_string()),
                        source: Some("/usr/local/bin/hev-socks5-tproxy".to_string()),
                        _type: Some("bind".to_string()),
                        options: Some(vec!["ro".to_string()]),
                        uid_mappings: None,
                        gid_mappings: None,
                    }
                ])
                .command(["sh", "-c", &script])
                .labels([
                    ("neox.role", "proxy-sidecar"),
                    ("neox.proxy.type", "hev-socks5-tproxy"),
                    ("neox.pod", req.name.as_str()),
                ])
                .build();

            state.podman.containers()
                .create(&sidecar_opts)
                .await
                .map_err(|e| AppError::Podman(format!(
                    "Failed to create hev-socks5-tproxy sidecar '{}': {}", sidecar_name, e
                )))?;

            tracing::info!("✅ hev-socks5-tproxy sidecar '{}' created", sidecar_name);

            // Opción B: persist proxy URL as pod label
            set_pod_proxy_label(&req.name, socks5_url);
        }
    }

    for ctr_spec in &req.containers {
        if ctr_spec.name.is_empty() {
            return Err(AppError::BadRequest("Container name is required".into()));
        }
        if ctr_spec.image.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Image is required for container '{}'", ctr_spec.name
            )));
        }

        let ctr_name = ctr_spec.name.clone();
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

        if let Some(ref entrypoint) = ctr_spec.entrypoint {
            if !entrypoint.is_empty() {
                ctr_builder = ctr_builder.entrypoint(entrypoint.iter().map(|s| s.as_str()));
            }
        }

        if !ctr_spec.command.is_empty() {
            ctr_builder = ctr_builder.command(ctr_spec.command.iter().map(|s| s.as_str()));
        }

        let temp_name = format!("{}-tmp-{}", ctr_name, uuid::Uuid::new_v4().to_string()[..8].to_string());
        let ctr_opts = ctr_builder.name(&temp_name).build();

        let created = state.podman.containers()
            .create(&ctr_opts)
            .await
            .map_err(|e| AppError::Podman(format!(
                "Failed to create container '{}' in pod '{}': {}", ctr_name, req.name, e
            )))?;

        let short_id = &created.id[..12];
        let final_name = format!("{}-{}", ctr_name, short_id);
        state.podman.containers().get(&created.id).rename(&final_name).await
            .map_err(|e| AppError::Internal(format!("Failed to rename container: {}", e)))?;

        tracing::info!("✅ Container '{}' created in pod '{}'", ctr_name, req.name);
    }

    tracing::info!("🚀 Starting pod '{}'", req.name);
    pod.start()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to start pod '{}': {}", req.name, e)))?;
    tracing::info!("✅ Pod '{}' started successfully", req.name);

    let inspect = pod.inspect()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to inspect pod: {}", e)))?;

    // Read back the label we just set so the response is accurate
    let proxy_url = if proxy_enabled {
        req.proxy.as_ref().and_then(|p| p.socks5_url.clone())
    } else {
        None
    };

    let response = build_pod_response(&inspect, proxy_enabled, proxy_url);
    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Inspect Pod ─────────────────────────────────────────────────────────────

/// GET /api/pods/:id
pub async fn get_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let pod = state.podman.pods().get(&id);

    let inspect = pod.inspect()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to inspect pod '{}': {}", id, e)))?;

    let proxy_enabled = inspect.containers.as_ref()
        .map(|ctrs| {
            ctrs.iter().any(|c| {
                let name = c.name.as_deref().unwrap_or("");
                name.contains("hev-tproxy") || name.contains("ipt2socks") || name.contains("tun2socks")
            })
        })
        .unwrap_or(false);

    // Opción B: read proxy URL from pod label
    let proxy_url = inspect.labels
        .as_ref()
        .and_then(|l| l.get("neox.proxy.url"))
        .cloned();

    let response = build_pod_response(&inspect, proxy_enabled, proxy_url);
    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Delete Pod ──────────────────────────────────────────────────────────────

/// DELETE /api/pods/:id
pub async fn delete_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<DeletePodQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🗑️ Deleting pod '{}' (force: {})", id, query.force);
    let pod = state.podman.pods().get(&id);
    if query.force {
        pod.remove().await
            .map_err(|e| AppError::Podman(format!("Failed to force-remove pod '{}': {}", id, e)))?;
    } else {
        pod.delete().await
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
pub async fn start_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("▶️ Starting pod '{}'", id);
    state.podman.pods().get(&id).start().await
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
pub async fn stop_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("⏹️ Stopping pod '{}'", id);
    state.podman.pods().get(&id).stop().await
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
pub async fn restart_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🔄 Restarting pod '{}'", id);
    state.podman.pods().get(&id).restart().await
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
pub async fn list_pod_containers(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let pod = state.podman.pods().get(&id);
    let inspect = pod.inspect().await
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

    let ctr_name = ctr.name.clone();
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

    if let Some(ref entrypoint) = ctr.entrypoint {
        if !entrypoint.is_empty() {
            ctr_builder = ctr_builder.entrypoint(entrypoint.iter().map(|s| s.as_str()));
        }
    }
    if !ctr.command.is_empty() {
        ctr_builder = ctr_builder.command(ctr.command.iter().map(|s| s.as_str()));
    }

    let temp_name = format!("{}-tmp-{}", ctr_name, uuid::Uuid::new_v4().to_string()[..8].to_string());
    let ctr_opts = ctr_builder.name(&temp_name).build();

    let created = state.podman.containers()
        .create(&ctr_opts)
        .await
        .map_err(|e| AppError::Podman(format!(
            "Failed to create container '{}' in pod '{}': {}", ctr_name, id, e
        )))?;

    let short_id = &created.id[..12];
    let final_name = format!("{}-{}", ctr_name, short_id);
    state.podman.containers().get(&created.id).rename(&final_name).await
        .map_err(|e| AppError::Internal(format!("Failed to rename container: {}", e)))?;

    let container_id = created.id.clone();
    tracing::info!("✅ Container '{}' added to pod '{}' (id: {})", ctr_name, id, container_id);

    state.podman.containers().get(&container_id)
        .start(None)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to start container '{}': {}", ctr_name, e)))?;

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
pub async fn generate_kube_yaml(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<GenerateKubeQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("📄 Generating Kube YAML for pod '{}'", id);
    let pod = state.podman.pods().get(&id);
    let yaml = pod.generate_kube_yaml(query.service).await
        .map_err(|e| AppError::Podman(format!(
            "Failed to generate Kube YAML for pod '{}': {}", id, e
        )))?;
    Ok(Json(json!({
        "pod_id": id,
        "yaml": yaml,
        "service_included": query.service,
    })))
}

/// GET /api/pods/:id/logs
pub async fn get_pod_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> Result<String, AppError> {
    crate::services::podman::get_pod_logs(&state, &id, query.tail).await
}

// ─── Rename Pod ─────────────────────────────────────────────────────────────

/// POST /api/pods/:id/rename
pub async fn rename_pod(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<RenamePodRequest>,
) -> Result<Json<Value>, AppError> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("New pod name is required".into()));
    }
    tracing::info!("🏷️ Renaming pod '{}' to '{}'", id, req.name);
    let output = std::process::Command::new("podman")
        .args(["pod", "rename", &id, &req.name])
        .output()
        .map_err(|e| AppError::Internal(format!("Failed to execute podman pod rename: {}", e)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Podman(format!("podman pod rename failed: {}", stderr)));
    }
    Ok(Json(json!({
        "success": true,
        "message": format!("Pod '{}' renamed to '{}'", id, req.name),
        "pod_id": id,
        "new_name": req.name,
    })))
}

// ─── Update Proxy ───────────────────────────────────────────────────────────

/// POST /api/pods/:id/proxy
pub async fn update_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProxyRequest>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🔌 Updating proxy for pod '{}'. enabled: {}", id, req.proxy.enabled);

    let pod = state.podman.pods().get(&id);
    let inspect = pod.inspect().await
        .map_err(|e| AppError::Podman(format!("Failed to inspect pod '{}': {}", id, e)))?;

    let pod_name = inspect.name.clone().unwrap_or_else(|| id.clone());

    // 1. Find and force-delete existing proxy sidecar(s)
    if let Some(ctrs) = &inspect.containers {
        for c in ctrs {
            let name = c.name.as_deref().unwrap_or("");
            if name.contains("hev-tproxy") || name.contains("ipt2socks") || name.contains("tun2socks") {
                tracing::info!("🗑️ Removing existing proxy sidecar: {}", name);
                let ctr_id = c.id.clone().unwrap_or_default();
                let _ = state.podman.containers().get(&ctr_id)
                    .delete(&ContainerDeleteOpts::builder().force(true).build())
                    .await;
            }
        }
    }

    // 2. If proxy disabled → remove label and return
    if !req.proxy.enabled {
        remove_pod_proxy_label(&pod_name);
        return Ok(Json(json!({
            "success": true,
            "message": format!("Proxy removed for pod '{}'", id),
        })));
    }

    // 3. Create new sidecar with Opción C drop-all guard in script
    let socks5_url = req.proxy.socks5_url.as_deref()
        .ok_or_else(|| AppError::BadRequest("socks5_url is required when proxy is enabled".into()))?;

    let (proxy_user, proxy_pass, proxy_host, proxy_port) = parse_socks5_url(socks5_url);

    let proxy_image = req.proxy.image.as_deref()
        .unwrap_or("localhost/neox-tproxy-sidecar:latest");
    let log_level = req.proxy.loglevel.as_deref().unwrap_or("warn");
    let dns_server = req.proxy.dns.as_deref().unwrap_or("8.8.8.8");

    let short_uuid = &uuid::Uuid::new_v4().to_string()[..8];
    let sidecar_name = format!("{}-hev-tproxy-{}", pod_name, short_uuid);

    let auth_yaml = match (&proxy_user, &proxy_pass) {
        (Some(user), Some(pass)) => format!("  username: '{}'\n  password: '{}'", user, pass),
        _ => String::new(),
    };

    let script = build_tproxy_script(&proxy_host, &proxy_port, &auth_yaml, dns_server, log_level);

    let sidecar_opts = ContainerCreateOpts::builder()
        .name(&sidecar_name)
        .image(proxy_image)
        .pod(pod_name.as_str())
        .privileged(true)
        .mounts(vec![
            ContainerMount {
                destination: Some("/usr/local/bin/hev-socks5-tproxy".to_string()),
                source: Some("/usr/local/bin/hev-socks5-tproxy".to_string()),
                _type: Some("bind".to_string()),
                options: Some(vec!["ro".to_string()]),
                uid_mappings: None,
                gid_mappings: None,
            }
        ])
        .command(["sh", "-c", &script])
        .labels([
            ("neox.role", "proxy-sidecar"),
            ("neox.proxy.type", "hev-socks5-tproxy"),
            ("neox.pod", pod_name.as_str()),
        ])
        .build();

    let created = state.podman.containers()
        .create(&sidecar_opts)
        .await
        .map_err(|e| AppError::Podman(format!(
            "Failed to create hev-socks5-tproxy sidecar '{}': {}", sidecar_name, e
        )))?;

    state.podman.containers().get(&created.id)
        .start(None)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to start sidecar '{}': {}", sidecar_name, e)))?;

    // Opción B: update proxy URL label on the pod
    set_pod_proxy_label(&pod_name, socks5_url);

    tracing::info!("✅ Proxy updated for pod '{}' → sidecar '{}'", id, sidecar_name);

    Ok(Json(json!({
        "success": true,
        "message": format!("Proxy updated for pod '{}'", id),
        "sidecar": sidecar_name,
    })))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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

fn build_pod_response(inspect: &InspectPodData, proxy_enabled: bool, proxy_url: Option<String>) -> PodResponse {
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
        proxy_url,
        infra_id,
    }
}
