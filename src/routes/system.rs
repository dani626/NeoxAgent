use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

/// GET /api/health
/// Public endpoint (no auth required) for health checks.
pub async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    let podman_version = match state.podman.info().await {
        Ok(info) => info
            .version
            .and_then(|v| v.version)
            .unwrap_or_else(|| "unknown".to_string()),
        Err(_) => "unreachable".to_string(),
    };

    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "podman_version": podman_version,
    }))
}

/// GET /api/system/info
/// Returns detailed system and Podman information.
pub async fn system_info(State(state): State<Arc<AppState>>) -> Json<Value> {
    match state.podman.info().await {
        Ok(info) => {
            let host = info.host.as_ref();
            let version = info.version.as_ref();
            let store = info.store.as_ref();

            // Extract CPU cores from host info
            let cpu_cores = host
                .and_then(|h| h.cpus.as_ref())
                .copied()
                .unwrap_or(0);

            // Extract memory total (bytes → MB)
            let memory_total_mb = host
                .and_then(|h| h.mem_total.as_ref())
                .map(|m| m / 1024 / 1024)
                .unwrap_or(0);

            // Extract cgroup version
            let cgroup_version = host
                .and_then(|h| h.cgroup_version.as_deref())
                .unwrap_or("unknown");

            Json(json!({
                "os": host.and_then(|h| h.os.as_deref()).unwrap_or("unknown"),
                "arch": host.and_then(|h| h.arch.as_deref()).unwrap_or("unknown"),
                "hostname": host.and_then(|h| h.hostname.as_deref()).unwrap_or("unknown"),
                "kernel": host.and_then(|h| h.kernel.as_deref()).unwrap_or("unknown"),
                "podman_version": version.and_then(|v| v.version.as_deref()).unwrap_or("unknown"),
                "api_version": version.and_then(|v| v.api_version.as_deref()).unwrap_or("unknown"),
                "containers": store
                    .and_then(|s| s.container_store.as_ref())
                    .and_then(|cs| cs.number)
                    .unwrap_or(0),
                "images": store
                    .and_then(|s| s.image_store.as_ref())
                    .and_then(|is| is.number)
                    .unwrap_or(0),
                "rootless": host.and_then(|h| h.security.as_ref())
                    .and_then(|s| s.rootless)
                    .unwrap_or(false),
                "cpu_cores": cpu_cores,
                "memory_total_mb": memory_total_mb,
                "cgroup_version": cgroup_version,
            }))
        }
        Err(e) => Json(json!({
            "error": true,
            "message": format!("Failed to get Podman info: {}", e),
        })),
    }
}

/// GET /api/system/resources
/// Returns current resource usage (CPU, memory, disk).
pub async fn system_resources(State(state): State<Arc<AppState>>) -> Json<Value> {
    match state.podman.info().await {
        Ok(info) => {
            let host = info.host.as_ref();

            let mem_total = host
                .and_then(|h| h.mem_total.as_ref())
                .copied()
                .unwrap_or(0);
            let mem_free = host
                .and_then(|h| h.mem_free.as_ref())
                .copied()
                .unwrap_or(0);

            let mem_total_mb = mem_total / 1024 / 1024;
            let mem_free_mb = mem_free / 1024 / 1024;
            let mem_used_mb = if mem_total_mb > mem_free_mb {
                mem_total_mb - mem_free_mb
            } else {
                0
            };

            // Swap info
            let swap_total = host
                .and_then(|h| h.swap_total.as_ref())
                .copied()
                .unwrap_or(0);
            let swap_free = host
                .and_then(|h| h.swap_free.as_ref())
                .copied()
                .unwrap_or(0);

            Json(json!({
                "memory_total_mb": mem_total_mb,
                "memory_used_mb": mem_used_mb,
                "memory_free_mb": mem_free_mb,
                "swap_total_mb": swap_total / 1024 / 1024,
                "swap_free_mb": swap_free / 1024 / 1024,
            }))
        }
        Err(e) => Json(json!({
            "error": true,
            "message": format!("Failed to get system resources: {}", e),
        })),
    }
}
