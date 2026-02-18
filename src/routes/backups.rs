use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::backups::{BackupInfo, CreateBackupRequest};
use crate::AppState;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Returns the directory where backups are stored for a given pod.
/// Path: {data_dir}/backups/{pod_id}/
fn backups_dir(state: &Arc<AppState>, pod_id: &str) -> PathBuf {
    state.config.agent.data_dir.join("backups").join(pod_id)
}

/// Returns the volume directory for a given pod (the data being backed up).
/// Path: {volumes_dir}/{pod_id}/
fn volume_dir(state: &Arc<AppState>, pod_id: &str) -> PathBuf {
    state.config.podman.volumes_dir.join(pod_id)
}

/// Read the backup metadata index for a pod.
/// The index is a JSON file: {data_dir}/backups/{pod_id}/index.json
async fn read_backup_index(backups_path: &PathBuf) -> Vec<BackupInfo> {
    let index_path = backups_path.join("index.json");
    if !index_path.exists() {
        return Vec::new();
    }
    match tokio::fs::read_to_string(&index_path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Write the backup metadata index.
async fn write_backup_index(backups_path: &PathBuf, index: &[BackupInfo]) -> Result<(), AppError> {
    let index_path = backups_path.join("index.json");
    let content = serde_json::to_string_pretty(index)
        .map_err(|e| AppError::Internal(format!("Failed to serialize backup index: {}", e)))?;
    tokio::fs::write(&index_path, content)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write backup index: {}", e)))?;
    Ok(())
}

/// Calculate SHA256 checksum of a file (runs on blocking thread).
fn calculate_sha256(path: &std::path::Path) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Create a tar.gz archive of a directory (runs on blocking thread).
fn create_backup_archive(
    source_dir: &std::path::Path,
    output_path: &std::path::Path,
    compression_level: u32,
) -> Result<(), std::io::Error> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let file = std::fs::File::create(output_path)?;
    let encoder = GzEncoder::new(file, Compression::new(compression_level));
    let mut tar = tar::Builder::new(encoder);

    // Archive the contents under "data/" prefix
    tar.append_dir_all("data", source_dir)?;

    let encoder = tar.into_inner()?;
    encoder.finish()?;

    Ok(())
}

/// Enforce backup limits: delete oldest backups beyond `max_per_server`.
async fn enforce_backup_limits(
    backups_path: &PathBuf,
    index: &mut Vec<BackupInfo>,
    max_per_server: u32,
) -> Result<(), AppError> {
    // Sort by created_at descending (newest first)
    index.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    while index.len() > max_per_server as usize {
        if let Some(old) = index.pop() {
            let file_path = backups_path.join(&old.filename);
            if file_path.exists() {
                let _ = tokio::fs::remove_file(&file_path).await;
                tracing::info!("   Pruned old backup: {} ({})", old.id, old.filename);
            }
        }
    }

    Ok(())
}

// ─── GET /api/pods/:id/backups — List Backups ────────────────────────────────

/// GET /api/pods/:id/backups
/// Lists all backups for a given pod.
pub async fn list_backups(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let backups_path = backups_dir(&state, &pod_id);

    let mut index = read_backup_index(&backups_path).await;

    // Sort newest first
    index.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let total = index.len();

    Ok(Json(json!({
        "pod_id": pod_id,
        "backups": index,
        "total": total,
    })))
}

// ─── POST /api/pods/:id/backups — Create Backup ─────────────────────────────

/// POST /api/pods/:id/backups
/// Creates a backup of the pod's volume directory.
pub async fn create_backup(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    body: Option<Json<CreateBackupRequest>>,
) -> Result<Json<Value>, AppError> {
    let req = body.map(|b| b.0).unwrap_or(CreateBackupRequest {
        stop_server: None,
        description: None,
    });

    let vol_dir = volume_dir(&state, &pod_id);
    if !vol_dir.exists() {
        return Err(AppError::NotFound(format!(
            "Volume directory for pod '{}' does not exist at '{}'",
            pod_id,
            vol_dir.display()
        )));
    }

    let backups_path = backups_dir(&state, &pod_id);
    tokio::fs::create_dir_all(&backups_path)
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to create backups directory: {}", e
        )))?;

    // Determine whether to stop the server
    let should_stop = req
        .stop_server
        .unwrap_or(state.config.backups.stop_server_before_backup);

    tracing::info!(
        "💾 Creating backup for pod '{}' (stop_server={})",
        pod_id,
        should_stop
    );

    // Optionally stop the pod for data consistency
    if should_stop {
        tracing::info!("   Stopping pod '{}' for consistent backup...", pod_id);
        let stop_result = tokio::process::Command::new("podman")
            .arg("pod")
            .arg("stop")
            .arg(&pod_id)
            .output()
            .await;

        match stop_result {
            Ok(o) if o.status.success() => {
                tracing::info!("   Pod '{}' stopped", pod_id);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!("   Warning: failed to stop pod: {}", stderr.trim());
            }
            Err(e) => {
                tracing::warn!("   Warning: could not stop pod: {}", e);
            }
        }
    }

    // Generate backup metadata
    let backup_id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now();
    let filename = format!(
        "{}.tar.gz",
        timestamp.format("%Y%m%d_%H%M%S")
    );
    let archive_path = backups_path.join(&filename);

    // Create the archive on a blocking thread
    let vol_dir_clone = vol_dir.clone();
    let archive_path_clone = archive_path.clone();
    let compression_level = state.config.backups.compression_level;

    let archive_result = tokio::task::spawn_blocking(move || {
        create_backup_archive(&vol_dir_clone, &archive_path_clone, compression_level)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Backup task panicked: {}", e)))?;

    archive_result.map_err(|e| AppError::Internal(format!(
        "Failed to create backup archive: {}", e
    )))?;

    // Calculate SHA256 checksum
    let archive_path_for_hash = archive_path.clone();
    let checksum = tokio::task::spawn_blocking(move || {
        calculate_sha256(&archive_path_for_hash)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Checksum task panicked: {}", e)))?
    .map_err(|e| AppError::Internal(format!("Failed to calculate checksum: {}", e)))?;

    // Get file size
    let archive_meta = tokio::fs::metadata(&archive_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read archive metadata: {}", e)))?;

    let size_bytes = archive_meta.len();
    let size_mb = (size_bytes as f64) / (1024.0 * 1024.0);

    let backup_info = BackupInfo {
        id: backup_id.clone(),
        pod_id: pod_id.clone(),
        size_bytes,
        size_mb: (size_mb * 100.0).round() / 100.0, // 2 decimal places
        created_at: timestamp.to_rfc3339(),
        checksum_sha256: checksum,
        server_was_stopped: should_stop,
        filename: filename.clone(),
        description: req.description,
    };

    // Update index
    let mut index = read_backup_index(&backups_path).await;
    index.push(backup_info.clone());

    // Enforce limits (delete oldest if over max_per_server)
    enforce_backup_limits(
        &backups_path,
        &mut index,
        state.config.backups.max_per_server,
    )
    .await?;

    write_backup_index(&backups_path, &index).await?;

    // Optionally restart the pod
    if should_stop {
        tracing::info!("   Restarting pod '{}'...", pod_id);
        let start_result = tokio::process::Command::new("podman")
            .arg("pod")
            .arg("start")
            .arg(&pod_id)
            .output()
            .await;

        match start_result {
            Ok(o) if o.status.success() => {
                tracing::info!("   Pod '{}' restarted", pod_id);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!("   Warning: failed to restart pod: {}", stderr.trim());
            }
            Err(e) => {
                tracing::warn!("   Warning: could not restart pod: {}", e);
            }
        }
    }

    tracing::info!(
        "✅ Backup created: {} ({:.2} MB, {})",
        backup_id,
        size_mb,
        filename
    );

    Ok(Json(serde_json::to_value(backup_info).unwrap()))
}

// ─── GET /api/pods/:id/backups/:backup_id — Backup Info ──────────────────────

/// GET /api/pods/:id/backups/:backup_id
/// Returns metadata for a specific backup.
pub async fn get_backup_info(
    State(state): State<Arc<AppState>>,
    Path((pod_id, backup_id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let backups_path = backups_dir(&state, &pod_id);
    let index = read_backup_index(&backups_path).await;

    let backup = index
        .iter()
        .find(|b| b.id == backup_id)
        .ok_or_else(|| AppError::NotFound(format!(
            "Backup '{}' not found for pod '{}'", backup_id, pod_id
        )))?;

    Ok(Json(serde_json::to_value(backup).unwrap()))
}

// ─── GET /api/pods/:id/backups/:backup_id/download — Download Backup ─────────

/// GET /api/pods/:id/backups/:backup_id/download
/// Downloads a backup file as a tar.gz stream.
pub async fn download_backup(
    State(state): State<Arc<AppState>>,
    Path((pod_id, backup_id)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let backups_path = backups_dir(&state, &pod_id);
    let index = read_backup_index(&backups_path).await;

    let backup = index
        .iter()
        .find(|b| b.id == backup_id)
        .ok_or_else(|| AppError::NotFound(format!(
            "Backup '{}' not found for pod '{}'", backup_id, pod_id
        )))?;

    let file_path = backups_path.join(&backup.filename);

    if !file_path.exists() {
        return Err(AppError::NotFound(format!(
            "Backup file '{}' not found on disk", backup.filename
        )));
    }

    tracing::info!(
        "📥 Downloading backup: pod={}, backup_id={}, file={}",
        pod_id,
        backup_id,
        backup.filename
    );

    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read backup file: {}", e)))?;

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", backup.filename),
        )
        .header(header::CONTENT_LENGTH, content.len())
        .body(Body::from(content))
        .map_err(|e| AppError::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

// ─── POST /api/pods/:id/backups/:backup_id/restore — Restore Backup ──────────

/// POST /api/pods/:id/backups/:backup_id/restore
/// Restores a backup by extracting the archive into the pod's volume directory.
pub async fn restore_backup(
    State(state): State<Arc<AppState>>,
    Path((pod_id, backup_id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let backups_path = backups_dir(&state, &pod_id);
    let index = read_backup_index(&backups_path).await;

    let backup = index
        .iter()
        .find(|b| b.id == backup_id)
        .ok_or_else(|| AppError::NotFound(format!(
            "Backup '{}' not found for pod '{}'", backup_id, pod_id
        )))?;

    let archive_path = backups_path.join(&backup.filename);
    if !archive_path.exists() {
        return Err(AppError::NotFound(format!(
            "Backup file '{}' not found on disk", backup.filename
        )));
    }

    let vol_dir = volume_dir(&state, &pod_id);

    tracing::info!(
        "🔄 Restoring backup '{}' for pod '{}' (stop → extract → start)",
        backup_id,
        pod_id
    );

    // Step 1: Stop the pod
    tracing::info!("   Stopping pod '{}'...", pod_id);
    let _ = tokio::process::Command::new("podman")
        .arg("pod")
        .arg("stop")
        .arg(&pod_id)
        .output()
        .await;

    // Step 2: Clear the volume directory (keep the dir itself)
    if vol_dir.exists() {
        let mut entries = tokio::fs::read_dir(&vol_dir)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read volume dir: {}", e)))?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AppError::Internal(format!("Failed to read entry: {}", e))
        })? {
            let path = entry.path();
            if path.is_dir() {
                let _ = tokio::fs::remove_dir_all(&path).await;
            } else {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    } else {
        tokio::fs::create_dir_all(&vol_dir)
            .await
            .map_err(|e| AppError::Internal(format!(
                "Failed to create volume directory: {}", e
            )))?;
    }

    // Step 3: Extract the archive
    let archive_clone = archive_path.clone();
    let vol_dir_clone = vol_dir.clone();

    let extract_result = tokio::task::spawn_blocking(move || {
        extract_backup_archive(&archive_clone, &vol_dir_clone)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Restore task panicked: {}", e)))?;

    extract_result.map_err(|e| AppError::Internal(format!(
        "Failed to extract backup: {}", e
    )))?;

    // Step 4: Restart the pod
    tracing::info!("   Restarting pod '{}'...", pod_id);
    let start_result = tokio::process::Command::new("podman")
        .arg("pod")
        .arg("start")
        .arg(&pod_id)
        .output()
        .await;

    let pod_started = match start_result {
        Ok(o) if o.status.success() => true,
        _ => false,
    };

    tracing::info!("✅ Backup '{}' restored for pod '{}'", backup_id, pod_id);

    Ok(Json(json!({
        "success": true,
        "backup_id": backup_id,
        "pod_id": pod_id,
        "pod_restarted": pod_started,
        "message": format!("Backup '{}' restored successfully", backup_id),
    })))
}

/// Extract a backup tar.gz archive into the volume directory (blocking).
fn extract_backup_archive(
    archive_path: &std::path::Path,
    vol_dir: &std::path::Path,
) -> Result<(), std::io::Error> {
    use flate2::read::GzDecoder;

    let file = std::fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    // The backup was created with "data/" prefix, so we need to strip it
    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?.to_path_buf();

        // Strip the "data/" prefix
        let relative = path
            .strip_prefix("data")
            .unwrap_or(&path);

        let output_path = vol_dir.join(relative);

        // Create parent directories
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&output_path)?;
        } else {
            let mut output_file = std::fs::File::create(&output_path)?;
            std::io::copy(&mut entry, &mut output_file)?;
        }
    }

    Ok(())
}

// ─── DELETE /api/pods/:id/backups/:backup_id — Delete Backup ─────────────────

/// DELETE /api/pods/:id/backups/:backup_id
/// Deletes a specific backup (file + index entry).
pub async fn delete_backup(
    State(state): State<Arc<AppState>>,
    Path((pod_id, backup_id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let backups_path = backups_dir(&state, &pod_id);
    let mut index = read_backup_index(&backups_path).await;

    let backup_pos = index
        .iter()
        .position(|b| b.id == backup_id)
        .ok_or_else(|| AppError::NotFound(format!(
            "Backup '{}' not found for pod '{}'", backup_id, pod_id
        )))?;

    let backup = index.remove(backup_pos);

    // Delete the archive file
    let file_path = backups_path.join(&backup.filename);
    if file_path.exists() {
        tokio::fs::remove_file(&file_path)
            .await
            .map_err(|e| AppError::Internal(format!(
                "Failed to delete backup file: {}", e
            )))?;
    }

    // Update the index
    write_backup_index(&backups_path, &index).await?;

    tracing::info!("🗑️ Backup '{}' deleted for pod '{}'", backup_id, pod_id);

    Ok(Json(json!({
        "success": true,
        "backup_id": backup_id,
        "pod_id": pod_id,
        "message": format!("Backup '{}' deleted successfully", backup_id),
    })))
}
