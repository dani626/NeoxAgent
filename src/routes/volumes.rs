use std::sync::Arc;

use axum::{
    routing::get,
    Router,
};
use axum::extract::{Path, State};
use axum::Json;

use crate::AppState;
use crate::error::AppError;
use crate::models::volume::{CreateVolumeRequest, VolumeListResponse, VolumeResponse};
use crate::services::podman as podman_service;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_volumes).post(create_volume))
        .route("/:name", get(inspect_volume).delete(delete_volume))
}

pub async fn list_volumes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<VolumeListResponse>, AppError> {
    let volumes = podman_service::list_volumes(&state).await?;
    Ok(Json(VolumeListResponse { volumes }))
}

pub async fn create_volume(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<Json<VolumeResponse>, AppError> {
    let volume = podman_service::create_volume(&state, req).await?;
    Ok(Json(volume))
}

pub async fn inspect_volume(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<VolumeResponse>, AppError> {
    let volume = podman_service::inspect_volume(&state, &name).await?;
    Ok(Json(volume))
}

pub async fn delete_volume(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    podman_service::delete_volume(&state, &name, false).await?;
    Ok(Json(serde_json::json!({ "message": "Volume deleted successfully" })))
}
