use axum::{
    extract::{Path, State, Query},
    routing::{get, post, delete},
    Json, Router,
};
use std::sync::Arc;
use serde::Deserialize;

use crate::AppState;
use crate::error::AppError;
use crate::models::volume::{VolumeResponse, CreateVolumeRequest};
use crate::services::podman;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_volumes).post(create_volume))
        .route("/:name", get(inspect_volume).delete(delete_volume))
}

async fn list_volumes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<VolumeResponse>>, AppError> {
    let volumes = podman::list_volumes(&state).await?;
    Ok(Json(volumes))
}

async fn create_volume(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<Json<VolumeResponse>, AppError> {
    let volume = podman::create_volume(&state, req).await?;
    Ok(Json(volume))
}

async fn inspect_volume(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<VolumeResponse>, AppError> {
    let volume = podman::inspect_volume(&state, &name).await?;
    Ok(Json(volume))
}

#[derive(Deserialize)]
struct DeleteParams {
    force: Option<bool>,
}

async fn delete_volume(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<DeleteParams>,
) -> Result<(), AppError> {
    podman::delete_volume(&state, &name, params.force.unwrap_or(false)).await?;
    Ok(())
}
