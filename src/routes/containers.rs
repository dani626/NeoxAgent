use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::AppError;
use crate::models::container::{
    CreateContainerRequest, DeleteContainerQuery, StopContainerQuery, LogsQuery, RenameContainerRequest,
};
use crate::services::podman;
use crate::AppState;

/// GET /api/containers
/// Lists all containers managed by Podman on this node.
pub async fn list_containers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, AppError> {
    let list = podman::list_containers(&state).await?;
    let total = list.len();

    Ok(Json(json!({
        "containers": list,
        "total": total,
    })))
}

/// POST /api/containers
/// Creates a new container from the provided configuration.
pub async fn create_container(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateContainerRequest>,
) -> Result<Json<Value>, AppError> {
    // Validate required fields
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Container name is required".into()));
    }
    if req.image.is_empty() {
        return Err(AppError::BadRequest("Image is required".into()));
    }

    tracing::info!("📦 Creating container '{}' with image '{}'", req.name, req.image);

    let response = podman::create_container(&state, req).await?;

    tracing::info!("✅ Container '{}' created: {}", response.name, response.id);

    Ok(Json(serde_json::to_value(response).unwrap()))
}

/// GET /api/containers/:id
/// Returns detailed information about a single container.
pub async fn get_container(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let response = podman::inspect_container(&state, &id).await?;
    Ok(Json(serde_json::to_value(response).unwrap()))
}

/// DELETE /api/containers/:id
/// Deletes a container. Optionally removes associated volumes and forces deletion.
pub async fn delete_container(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<DeleteContainerQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!(
        "🗑️  Deleting container '{}' (force={}, remove_volumes={})",
        id,
        query.force,
        query.remove_volumes
    );

    podman::delete_container(&state, &id, query.force, query.remove_volumes).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Container '{}' deleted", id),
        "container_id": id,
    })))
}

/// POST /api/containers/:id/start
/// Starts a stopped container.
pub async fn start_container(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("▶️  Starting container '{}'", id);
    podman::start_container(&state, &id).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Container '{}' started", id),
        "container_id": id,
    })))
}

/// POST /api/containers/:id/stop
/// Gracefully stops a running container with optional timeout.
pub async fn stop_container(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<StopContainerQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("⏹️  Stopping container '{}' (timeout={:?})", id, query.timeout);
    podman::stop_container(&state, &id, query.timeout).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Container '{}' stopped", id),
        "container_id": id,
    })))
}

/// POST /api/containers/:id/restart
/// Restarts a container.
pub async fn restart_container(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🔄 Restarting container '{}'", id);
    podman::restart_container(&state, &id).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Container '{}' restarted", id),
        "container_id": id,
    })))
}

/// POST /api/containers/:id/kill
/// Force-kills a container (SIGKILL).
pub async fn kill_container(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("💀 Killing container '{}'", id);
    podman::kill_container(&state, &id).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Container '{}' killed", id),
        "container_id": id,
    })))
}

/// POST /api/containers/:id/rename
/// Renames a container.
pub async fn rename_container(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<RenameContainerRequest>,
) -> Result<Json<Value>, AppError> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("New container name is required".into()));
    }

    tracing::info!("🏷️ Renaming container '{}' to '{}'", id, req.name);
    state.podman.containers().get(&id).rename(&req.name).await
        .map_err(|e| AppError::Podman(format!("Failed to rename container: {}", e)))?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Container '{}' renamed to '{}'", id, req.name),
        "container_id": id,
        "new_name": req.name,
    })))
}

/// GET /api/containers/:id/logs
pub async fn get_container_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> Result<String, AppError> {
    podman::get_container_logs(&state, &id, query.tail).await
}
