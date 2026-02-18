use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::AppError;
use crate::models::images::{SystemdGenerateResponse, SystemdStatusResponse};
use crate::AppState;

// ─── POST /api/pods/:id/systemd/generate — Generate .service file ────────────

/// POST /api/pods/:id/systemd/generate
/// Generates a systemd .service file for the pod using `podman generate systemd`.
pub async fn generate_systemd(
    State(_state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("⚙️ Generating systemd service for pod '{}'", pod_id);

    // Generate systemd unit using podman CLI
    let output = tokio::process::Command::new("podman")
        .arg("generate")
        .arg("systemd")
        .arg("--name")
        .arg("--new")
        .arg("--restart-policy=on-failure")
        .arg(&pod_id)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'podman generate systemd': {}", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Podman(format!(
            "Failed to generate systemd service for '{}': {}", pod_id, stderr.trim()
        )));
    }

    let unit_content = String::from_utf8_lossy(&output.stdout).to_string();

    // Determine service name and file path
    let service_name = format!("pod-{}.service", pod_id);

    // Write the service file to the user's systemd directory
    // For rootless: ~/.config/systemd/user/
    // For root: /etc/systemd/system/
    let systemd_dir = if is_rootless().await {
        dirs_systemd_user().await?
    } else {
        std::path::PathBuf::from("/etc/systemd/system")
    };

    tokio::fs::create_dir_all(&systemd_dir)
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to create systemd directory: {}", e
        )))?;

    let service_path = systemd_dir.join(&service_name);

    tokio::fs::write(&service_path, &unit_content)
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to write service file: {}", e
        )))?;

    // Reload systemd daemon
    reload_systemd_daemon().await?;

    tracing::info!(
        "✅ Systemd service '{}' generated at '{}'",
        service_name,
        service_path.display()
    );

    let response = SystemdGenerateResponse {
        pod_id: pod_id.clone(),
        service_name: service_name.clone(),
        service_file_path: service_path.display().to_string(),
        unit_content,
        message: format!(
            "Service '{}' generated. Use 'enable' endpoint to auto-start on boot.",
            service_name
        ),
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── POST /api/pods/:id/systemd/enable — Enable auto-start ───────────────────

/// POST /api/pods/:id/systemd/enable
/// Enables the systemd service to start the pod on boot.
pub async fn enable_systemd(
    State(_state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let service_name = format!("pod-{}.service", pod_id);

    tracing::info!("🔛 Enabling systemd service '{}'", service_name);

    let args = if is_rootless().await {
        vec!["--user", "enable", &service_name]
    } else {
        vec!["enable", &service_name]
    };

    let output = tokio::process::Command::new("systemctl")
        .args(&args)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'systemctl enable': {}", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!(
            "Failed to enable service '{}': {}", service_name, stderr.trim()
        )));
    }

    tracing::info!("✅ Service '{}' enabled for boot", service_name);

    Ok(Json(json!({
        "success": true,
        "pod_id": pod_id,
        "service_name": service_name,
        "enabled": true,
        "message": format!("Service '{}' enabled. Pod will auto-start on boot.", service_name),
    })))
}

// ─── POST /api/pods/:id/systemd/disable — Disable auto-start ─────────────────

/// POST /api/pods/:id/systemd/disable
/// Disables the systemd service (no auto-start on boot).
pub async fn disable_systemd(
    State(_state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let service_name = format!("pod-{}.service", pod_id);

    tracing::info!("🔕 Disabling systemd service '{}'", service_name);

    let args = if is_rootless().await {
        vec!["--user", "disable", &service_name]
    } else {
        vec!["disable", &service_name]
    };

    let output = tokio::process::Command::new("systemctl")
        .args(&args)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'systemctl disable': {}", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!(
            "Failed to disable service '{}': {}", service_name, stderr.trim()
        )));
    }

    tracing::info!("✅ Service '{}' disabled", service_name);

    Ok(Json(json!({
        "success": true,
        "pod_id": pod_id,
        "service_name": service_name,
        "enabled": false,
        "message": format!("Service '{}' disabled. Pod will not auto-start on boot.", service_name),
    })))
}

// ─── GET /api/pods/:id/systemd/status — Service status ───────────────────────

/// GET /api/pods/:id/systemd/status
/// Gets the current systemd service status for the pod.
pub async fn systemd_status(
    State(_state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let service_name = format!("pod-{}.service", pod_id);

    tracing::info!("📊 Getting systemd status for '{}'", service_name);

    let user_flag: &[&str] = if is_rootless().await {
        &["--user"]
    } else {
        &[]
    };

    // Get active state
    let active_output = tokio::process::Command::new("systemctl")
        .args(user_flag)
        .arg("show")
        .arg(&service_name)
        .arg("--property=ActiveState,SubState,Description,UnitFileState")
        .arg("--no-pager")
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to query systemd status: {}", e
        )))?;

    let stdout = String::from_utf8_lossy(&active_output.stdout);

    // Parse key=value output
    let mut active_state = "unknown".to_string();
    let mut sub_state = "unknown".to_string();
    let mut description = String::new();
    let mut unit_file_state = "unknown".to_string();

    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "ActiveState" => active_state = value.trim().to_string(),
                "SubState" => sub_state = value.trim().to_string(),
                "Description" => description = value.trim().to_string(),
                "UnitFileState" => unit_file_state = value.trim().to_string(),
                _ => {}
            }
        }
    }

    let enabled = unit_file_state == "enabled";

    let response = SystemdStatusResponse {
        pod_id: pod_id.clone(),
        service_name: service_name.clone(),
        active_state,
        sub_state,
        enabled,
        description,
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Check if Podman is running rootless (non-root user).
async fn is_rootless() -> bool {
    let output = tokio::process::Command::new("podman")
        .arg("info")
        .arg("--format")
        .arg("{{.Host.Security.Rootless}}")
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.trim().eq_ignore_ascii_case("true")
        }
        _ => {
            // Default to rootless check via UID
            libc_uid() != 0
        }
    }
}

/// Get UID without libc dependency — check if running as root.
fn libc_uid() -> u32 {
    // On Linux, we can check /proc/self/status
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("Uid:"))
                .and_then(|line| {
                    line.split_whitespace()
                        .nth(1)
                        .and_then(|uid| uid.parse().ok())
                })
        })
        .unwrap_or(1000) // Default to non-root
}

/// Get the user's systemd directory for rootless mode.
async fn dirs_systemd_user() -> Result<std::path::PathBuf, AppError> {
    // Try XDG_RUNTIME_DIR first, then HOME
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return Ok(std::path::PathBuf::from(runtime_dir)
            .join("systemd")
            .join("user"));
    }

    if let Ok(home) = std::env::var("HOME") {
        return Ok(std::path::PathBuf::from(home)
            .join(".config")
            .join("systemd")
            .join("user"));
    }

    Err(AppError::Internal(
        "Could not determine systemd user directory. Set HOME or XDG_RUNTIME_DIR.".into(),
    ))
}

/// Reload systemd daemon to pick up new/changed service files.
async fn reload_systemd_daemon() -> Result<(), AppError> {
    let args: Vec<&str> = if is_rootless().await {
        vec!["--user", "daemon-reload"]
    } else {
        vec!["daemon-reload"]
    };

    let output = tokio::process::Command::new("systemctl")
        .args(&args)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to reload systemd daemon: {}", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("Warning: daemon-reload had issues: {}", stderr.trim());
    }

    Ok(())
}
