use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;


use crate::error::AppError;
use crate::models::files::{
    DownloadQuery, FileContentResponse, FileEntry, FilePathQuery, ListDirectoryResponse,
    RenameRequest, UploadQuery,
};
use crate::AppState;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Resolves the absolute filesystem path for a pod's volume directory.
/// The volume root is `{volumes_dir}/{pod_id}/`.
fn resolve_volume_root(state: &Arc<AppState>, pod_id: &str) -> PathBuf {
    state.config.podman.volumes_dir.join(pod_id)
}

/// Security: Resolves a user-provided relative path against the volume root,
/// ensuring it does not escape via `..` or symlinks.
///
/// Returns the safe, canonicalized absolute path.
fn safe_resolve(volume_root: &StdPath, user_path: &str) -> Result<PathBuf, AppError> {
    // Reject obviously malicious patterns
    let normalized = user_path.replace('\\', "/");
    if normalized.contains("..") {
        return Err(AppError::BadRequest(
            "Path traversal detected: '..' is not allowed".into(),
        ));
    }

    // Strip leading slash so the join makes it relative
    let relative = normalized.trim_start_matches('/');

    // Join and canonicalize
    let target = volume_root.join(relative);

    // For checking, we need the volume root to exist first
    // If target doesn't exist yet (e.g., creating dirs), we check the parent
    let check_path = if target.exists() {
        target
            .canonicalize()
            .map_err(|e| AppError::Internal(format!("Failed to canonicalize path: {}", e)))?
    } else {
        // For new files, verify the parent directory is inside the volume root
        let parent = target
            .parent()
            .ok_or_else(|| AppError::BadRequest("Invalid path".into()))?;
        if !parent.exists() {
            return Err(AppError::NotFound(format!(
                "Parent directory does not exist: {}",
                parent.display()
            )));
        }
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| AppError::Internal(format!("Failed to canonicalize parent: {}", e)))?;
        // Return the canonical parent joined with the filename
        let filename = target
            .file_name()
            .ok_or_else(|| AppError::BadRequest("Invalid filename".into()))?;
        canonical_parent.join(filename)
    };

    // Ensure the canonical root exists
    let canonical_root = if volume_root.exists() {
        volume_root
            .canonicalize()
            .map_err(|e| AppError::Internal(format!("Volume root error: {}", e)))?
    } else {
        volume_root.to_path_buf()
    };

    // Security check: the resolved path must start with the volume root
    if !check_path.starts_with(&canonical_root) {
        return Err(AppError::BadRequest(
            "Access denied: path is outside the server volume".into(),
        ));
    }

    Ok(check_path)
}

/// Ensure the volume root directory exists, creating it if needed.
async fn ensure_volume_root(root: &StdPath) -> Result<(), AppError> {
    if !root.exists() {
        tokio::fs::create_dir_all(root)
            .await
            .map_err(|e| AppError::Internal(format!(
                "Failed to create volume directory '{}': {}", root.display(), e
            )))?;
    }
    Ok(())
}

/// Format unix permissions as a human-readable string (e.g., "rwxr-xr-x").
#[cfg(unix)]
fn format_permissions(metadata: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    let mode = metadata.permissions().mode();
    let mut perm = String::with_capacity(9);
    let flags = [
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'),
        (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    for (bit, ch) in &flags {
        perm.push(if mode & bit != 0 { *ch } else { '-' });
    }
    perm
}

/// Format permissions — Windows does not have Unix-style permissions.
#[cfg(not(unix))]
fn format_permissions(_metadata: &std::fs::Metadata) -> String {
    String::new()
}

/// Format the modification time from file metadata.
fn format_modified(metadata: &std::fs::Metadata) -> String {
    metadata
        .modified()
        .ok()
        .map(|t| crate::time_utils::system_time_to_rfc3339(t))
        .unwrap_or_default()
}

// ─── GET /api/pods/:id/files — List Directory ────────────────────────────────

/// GET /api/pods/:id/files?path=/
/// Lists the contents of a directory within a pod's volume.
pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<Value>, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;
    let target = safe_resolve(&volume_root, &query.path)?;

    if !target.is_dir() {
        return Err(AppError::BadRequest(format!(
            "'{}' is not a directory", query.path
        )));
    }

    tracing::info!("📂 Listing files: pod={}, path={}", pod_id, query.path);

    let mut items: Vec<FileEntry> = Vec::new();

    let mut entries = tokio::fs::read_dir(&target)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read directory: {}", e)))?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        AppError::Internal(format!("Failed to read entry: {}", e))
    })? {
        let metadata = entry.metadata().await.map_err(|e| {
            AppError::Internal(format!("Failed to read metadata: {}", e))
        })?;

        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = metadata.is_dir();
        let size = if is_dir { 0 } else { metadata.len() };
        let modified = format_modified(&metadata);
        let permissions = format_permissions(&metadata);

        // Count children for directories
        let children = if is_dir {
            match tokio::fs::read_dir(entry.path()).await {
                Ok(mut dir) => {
                    let mut count = 0;
                    while dir.next_entry().await.ok().flatten().is_some() {
                        count += 1;
                    }
                    Some(count)
                }
                Err(_) => Some(0),
            }
        } else {
            None
        };

        items.push(FileEntry {
            name,
            entry_type: if is_dir { "dir".into() } else { "file".into() },
            size,
            modified,
            permissions,
            children,
        });
    }

    // Sort: directories first, then files, alphabetically
    items.sort_by(|a, b| {
        let dir_cmp = (a.entry_type == "file").cmp(&(b.entry_type == "file"));
        dir_cmp.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let response = ListDirectoryResponse {
        path: query.path,
        items,
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── GET /api/pods/:id/files/content — Read File ─────────────────────────────

/// GET /api/pods/:id/files/content?path=/server.properties
/// Reads the content of a file as UTF-8 text.
pub async fn read_file(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<Value>, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;
    let target = safe_resolve(&volume_root, &query.path)?;

    if !target.is_file() {
        return Err(AppError::BadRequest(format!(
            "'{}' is not a file", query.path
        )));
    }

    // Prevent reading huge files (limit to 10MB by default)
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|e| AppError::NotFound(format!("File not found: {}", e)))?;

    const MAX_READ_SIZE: u64 = 10 * 1024 * 1024; // 10MB
    if metadata.len() > MAX_READ_SIZE {
        return Err(AppError::BadRequest(format!(
            "File is too large to read as text ({} bytes, max {} bytes). Use the download endpoint instead.",
            metadata.len(), MAX_READ_SIZE
        )));
    }

    tracing::info!("📄 Reading file: pod={}, path={}", pod_id, query.path);

    let content = tokio::fs::read_to_string(&target)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read file: {}", e)))?;

    let response = FileContentResponse {
        path: query.path,
        content,
        size: metadata.len(),
        modified: format_modified(&metadata),
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── PUT /api/pods/:id/files/content — Write File ────────────────────────────

/// PUT /api/pods/:id/files/content?path=/server.properties
/// Writes text content to a file (creates or overwrites).
pub async fn write_file(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<FilePathQuery>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;
    let target = safe_resolve(&volume_root, &query.path)?;

    // Target must not be a directory
    if target.exists() && target.is_dir() {
        return Err(AppError::BadRequest(format!(
            "'{}' is a directory, not a file", query.path
        )));
    }

    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::BadRequest(
                "Request body must contain a 'content' field with string value".into(),
            )
        })?;

    tracing::info!("✏️ Writing file: pod={}, path={}, size={}", pod_id, query.path, content.len());

    // Ensure parent directory exists
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create parent directory: {}", e)))?;
    }

    tokio::fs::write(&target, content)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write file: {}", e)))?;

    let metadata = tokio::fs::metadata(&target).await.ok();
    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

    Ok(Json(json!({
        "success": true,
        "path": query.path,
        "size": size,
        "message": format!("File '{}' written successfully", query.path),
    })))
}

// ─── POST /api/pods/:id/files/create-dir — Create Directory ──────────────────

/// POST /api/pods/:id/files/create-dir?path=/plugins/new
/// Creates a new directory (and any necessary parents).
pub async fn create_directory(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<Value>, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;
    let target = safe_resolve(&volume_root, &query.path)?;

    if target.exists() {
        return Err(AppError::BadRequest(format!(
            "'{}' already exists", query.path
        )));
    }

    tracing::info!("📁 Creating directory: pod={}, path={}", pod_id, query.path);

    tokio::fs::create_dir_all(&target)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create directory: {}", e)))?;

    Ok(Json(json!({
        "success": true,
        "path": query.path,
        "message": format!("Directory '{}' created", query.path),
    })))
}

// ─── POST /api/pods/:id/files/rename — Rename/Move ───────────────────────────

/// POST /api/pods/:id/files/rename
/// Renames or moves a file/directory within the pod's volume.
pub async fn rename_file(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Json(req): Json<RenameRequest>,
) -> Result<Json<Value>, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;

    let from = safe_resolve(&volume_root, &req.from)?;
    let to = safe_resolve(&volume_root, &req.to)?;

    if !from.exists() {
        return Err(AppError::NotFound(format!(
            "Source '{}' does not exist", req.from
        )));
    }

    if to.exists() {
        return Err(AppError::BadRequest(format!(
            "Destination '{}' already exists", req.to
        )));
    }

    tracing::info!("🔄 Renaming: pod={}, from={}, to={}", pod_id, req.from, req.to);

    // Ensure parent dir of destination exists
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create destination parent: {}", e)))?;
    }

    tokio::fs::rename(&from, &to)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to rename: {}", e)))?;

    Ok(Json(json!({
        "success": true,
        "from": req.from,
        "to": req.to,
        "message": format!("Renamed '{}' to '{}'", req.from, req.to),
    })))
}

// ─── DELETE /api/pods/:id/files — Delete File/Directory ──────────────────────

/// DELETE /api/pods/:id/files?path=/logs/old.log
/// Deletes a file or directory (recursively).
pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<Value>, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;
    let target = safe_resolve(&volume_root, &query.path)?;

    if !target.exists() {
        return Err(AppError::NotFound(format!(
            "'{}' does not exist", query.path
        )));
    }

    // Prevent deleting the volume root itself
    let canonical_root = volume_root
        .canonicalize()
        .unwrap_or_else(|_| volume_root.clone());
    let canonical_target = target
        .canonicalize()
        .unwrap_or_else(|_| target.clone());
    if canonical_target == canonical_root {
        return Err(AppError::BadRequest(
            "Cannot delete the volume root directory".into(),
        ));
    }

    tracing::info!("🗑️ Deleting: pod={}, path={}", pod_id, query.path);

    if target.is_dir() {
        tokio::fs::remove_dir_all(&target)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to delete directory: {}", e)))?;
    } else {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to delete file: {}", e)))?;
    }

    Ok(Json(json!({
        "success": true,
        "path": query.path,
        "message": format!("'{}' deleted successfully", query.path),
    })))
}

// ─── POST /api/pods/:id/files/upload — Upload File ───────────────────────────

/// POST /api/pods/:id/files/upload?path=/plugins
/// Uploads a file via multipart form data into the specified directory.
pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;
    let target_dir = safe_resolve(&volume_root, &query.path)?;

    if !target_dir.is_dir() {
        return Err(AppError::BadRequest(format!(
            "Upload target '{}' is not a directory", query.path
        )));
    }

    tracing::info!("📤 Uploading file: pod={}, target_dir={}", pod_id, query.path);

    let mut uploaded_files: Vec<Value> = Vec::new();

    // Max upload size: 100MB per file
    const MAX_UPLOAD_SIZE: usize = 100 * 1024 * 1024;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::BadRequest(format!("Invalid multipart data: {}", e))
    })? {
        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload".to_string());

        // Sanitize filename: no path separators allowed
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            return Err(AppError::BadRequest(format!(
                "Invalid filename: '{}'", filename
            )));
        }

        let data = field.bytes().await.map_err(|e| {
            AppError::Internal(format!("Failed to read upload data: {}", e))
        })?;

        if data.len() > MAX_UPLOAD_SIZE {
            return Err(AppError::BadRequest(format!(
                "File '{}' exceeds maximum upload size of {}MB",
                filename,
                MAX_UPLOAD_SIZE / 1024 / 1024
            )));
        }

        let file_path = target_dir.join(&filename);

        // Security re-check after join
        let canonical_root = volume_root
            .canonicalize()
            .unwrap_or_else(|_| volume_root.clone());
        // Since the parent (target_dir) was already validated and filename has no separators,
        // this should be safe, but we verify anyway
        if let Ok(canonical_file) = file_path.parent().unwrap_or(&target_dir).canonicalize() {
            if !canonical_file.starts_with(&canonical_root) {
                return Err(AppError::BadRequest("Upload path escapes volume".into()));
            }
        }

        tokio::fs::write(&file_path, &data)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to save file: {}", e)))?;

        tracing::info!("   Saved: {} ({} bytes)", filename, data.len());

        uploaded_files.push(json!({
            "name": filename,
            "size": data.len(),
            "path": format!("{}/{}", query.path.trim_end_matches('/'), filename),
        }));
    }

    if uploaded_files.is_empty() {
        return Err(AppError::BadRequest("No files were uploaded".into()));
    }

    Ok(Json(json!({
        "success": true,
        "uploaded": uploaded_files,
        "total": uploaded_files.len(),
        "message": format!("{} file(s) uploaded to '{}'", uploaded_files.len(), query.path),
    })))
}

// ─── GET /api/pods/:id/files/download — Download as tar.gz ───────────────────

/// GET /api/pods/:id/files/download?path=/world
/// Downloads a file or directory as a tar.gz archive.
pub async fn download_file(
    State(state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, AppError> {
    let volume_root = resolve_volume_root(&state, &pod_id);
    ensure_volume_root(&volume_root).await?;
    let target = safe_resolve(&volume_root, &query.path)?;

    if !target.exists() {
        return Err(AppError::NotFound(format!(
            "'{}' does not exist", query.path
        )));
    }

    tracing::info!("📥 Downloading: pod={}, path={}", pod_id, query.path);

    if target.is_file() {
        // For a single file, just return the raw bytes
        let content = tokio::fs::read(&target)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read file: {}", e)))?;

        let filename = target
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".into());

        let response = Response::builder()
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            )
            .header(header::CONTENT_LENGTH, content.len())
            .body(Body::from(content))
            .map_err(|e| AppError::Internal(format!("Failed to build response: {}", e)))?;

        Ok(response)
    } else {
        // For a directory, create a tar.gz archive
        let dir_name = target
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".into());

        let archive_filename = format!("{}.tar.gz", dir_name);

        // Create tar.gz in memory using synchronous tar + flate2
        let target_clone = target.clone();
        let dir_name_clone = dir_name.clone();
        let archive_bytes = tokio::task::spawn_blocking(move || {
            create_tar_gz(&target_clone, &dir_name_clone)
        })
        .await
        .map_err(|e| AppError::Internal(format!("Archive task failed: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to create archive: {}", e)))?;

        tracing::info!(
            "   Archive created: {} ({} bytes)",
            archive_filename,
            archive_bytes.len()
        );

        let response = Response::builder()
            .header(header::CONTENT_TYPE, "application/gzip")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", archive_filename),
            )
            .header(header::CONTENT_LENGTH, archive_bytes.len())
            .body(Body::from(archive_bytes))
            .map_err(|e| AppError::Internal(format!("Failed to build response: {}", e)))?;

        Ok(response)
    }
}

/// Creates a tar.gz archive from a directory path. Runs on a blocking thread.
fn create_tar_gz(dir_path: &StdPath, archive_name: &str) -> Result<Vec<u8>, std::io::Error> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let buf = Vec::new();
    let encoder = GzEncoder::new(buf, Compression::new(6));
    let mut tar = tar::Builder::new(encoder);

    // Add the directory contents under the archive name
    tar.append_dir_all(archive_name, dir_path)?;

    let encoder = tar.into_inner()?;
    encoder.finish()
}
