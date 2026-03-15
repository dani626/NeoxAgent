use axum::extract::{Path, Query, State};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::sync::Arc;

use podman_api::opts::{ImageListOpts, ImageSearchOpts, PullOpts};

use crate::error::AppError;
use crate::models::images::{
    ImageInfo, ImageSearchQuery, ImageSearchResult, PullImageRequest, PullImageResponse,
};
use crate::AppState;

// ─── GET /api/images — List Images ───────────────────────────────────────────

/// GET /api/images
/// Lists all images available on the node.
pub async fn list_images(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🖼️ Listing images");

    let images = state
        .podman
        .images()
        .list(&ImageListOpts::builder().all(true).build())
        .await
        .map_err(|e| AppError::Podman(format!("Failed to list images: {}", e)))?;

    let image_list: Vec<ImageInfo> = images
        .iter()
        .map(|img| ImageInfo {
            id: img.id.clone().unwrap_or_default(),
            repo_tags: img.repo_tags.clone().unwrap_or_default(),
            repo_digests: img.repo_digests.clone().unwrap_or_default(),
            size: img.size,
            virtual_size: img.virtual_size,
            created: img.created,
            containers: img.containers,
            read_only: img.read_only,
            dangling: img.dangling,
        })
        .collect();

    let total = image_list.len();

    Ok(Json(json!({
        "images": image_list,
        "total": total,
    })))
}

// ─── POST /api/images/pull — Pull Image ──────────────────────────────────────

/// POST /api/images/pull
/// Pulls an image from a container registry.
pub async fn pull_image(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PullImageRequest>,
) -> Result<Json<Value>, AppError> {
    if req.image.trim().is_empty() {
        return Err(AppError::BadRequest("Image reference is required".into()));
    }

    tracing::info!("📥 Pulling image: {}", req.image);

    let opts = PullOpts::builder()
        .reference(&req.image)
        .build();

    let images = state.podman.images();
    let mut stream = images.pull(&opts);

    let mut pull_id: Option<String> = None;
    let mut last_error: Option<String> = None;

    while let Some(result) = stream.next().await {
        match result {
            Ok(report) => {
                if let Some(ref err) = report.error {
                    last_error = Some(err.clone());
                }
                if let Some(ref id) = report.id {
                    pull_id = Some(id.clone());
                }
                if let Some(ref stream_msg) = report.stream {
                    tracing::debug!("   Pull: {}", stream_msg.trim());
                }
            }
            Err(e) => {
                return Err(AppError::Podman(format!("Image pull failed: {}", e)));
            }
        }
    }

    if let Some(err) = last_error {
        return Err(AppError::Podman(format!("Image pull error: {}", err)));
    }

    tracing::info!("✅ Image '{}' pulled successfully", req.image);

    let response = PullImageResponse {
        success: true,
        image: req.image.clone(),
        id: pull_id,
        message: format!("Image '{}' pulled successfully", req.image),
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── DELETE /api/images/:id — Delete Image ───────────────────────────────────

/// DELETE /api/images/:id
/// Removes an image from the node.
pub async fn delete_image(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🗑️ Deleting image: {}", image_id);

    let image = state.podman.images().get(&image_id);

    image
        .remove()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to delete image '{}': {}", image_id, e)))?;

    tracing::info!("✅ Image '{}' deleted", image_id);

    Ok(Json(json!({
        "success": true,
        "image_id": image_id,
        "message": format!("Image '{}' deleted successfully", image_id),
    })))
}

// ─── GET /api/images/search — Search Images ──────────────────────────────────

/// GET /api/images/search?q=minecraft
/// Searches container registries for images.
pub async fn search_images(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImageSearchQuery>,
) -> Result<Json<Value>, AppError> {
    if query.q.trim().is_empty() {
        return Err(AppError::BadRequest("Search query 'q' is required".into()));
    }

    tracing::info!("🔍 Searching images: q={}, limit={}", query.q, query.limit);

    let opts = ImageSearchOpts::builder()
        .term(&query.q)
        .limit(query.limit as usize)
        .build();

    let results = state
        .podman
        .images()
        .search(&opts)
        .await
        .map_err(|e| AppError::Podman(format!("Image search failed: {}", e)))?;

    let search_results: Vec<ImageSearchResult> = results
        .iter()
        .map(|r| ImageSearchResult {
            index: r.index.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            stars: r.stars,
            official: r.official.clone(),
            automated: r.automated.clone(),
            tag: r.tag.clone(),
        })
        .collect();

    let total = search_results.len();

    Ok(Json(json!({
        "query": query.q,
        "results": search_results,
        "total": total,
    })))
}

// ─── WS /api/images/pull/stream — Pull with Progress ─────────────────────────

/// WS /api/images/pull/stream
/// WebSocket endpoint that streams pull progress in real-time.
/// Client sends: { "image": "docker.io/library/alpine" }
/// Server sends: { "stream": "...", "id": "...", "status": "..." } per chunk
pub async fn pull_image_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_pull_stream(socket, state))
}

async fn handle_pull_stream(mut socket: WebSocket, state: Arc<AppState>) {
    // Wait for the client to send the image reference
    let image_ref = match socket.recv().await {
        Some(Ok(Message::Text(text))) => {
            // Parse JSON: { "image": "..." }
            match serde_json::from_str::<Value>(&text.to_string()) {
                Ok(val) => {
                    val.get("image")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                }
                Err(_) => {
                    // Treat as plain text image reference
                    text.to_string()
                }
            }
        }
        _ => {
            let _ = socket
                .send(Message::Text(
                    json!({"error": "Expected image reference as first message"}).to_string().into(),
                ))
                .await;
            return;
        }
    };

    if image_ref.is_empty() {
        let _ = socket
            .send(Message::Text(
                json!({"error": "Image reference cannot be empty"}).to_string().into(),
            ))
            .await;
        return;
    }

    tracing::info!("📥 [WS] Pulling image with stream: {}", image_ref);

    // Send initial status
    let _ = socket
        .send(Message::Text(
            json!({
                "image": &image_ref,
                "status": "Starting pull",
                "type": "start",
            })
            .to_string().into(),
        ))
        .await;

    let opts = PullOpts::builder().reference(&image_ref).build();
    let images = state.podman.images();
    let mut stream = images.pull(&opts);

    while let Some(result) = stream.next().await {
        let msg = match result {
            Ok(report) => {
                if let Some(ref err) = report.error {
                    json!({
                        "image": &image_ref,
                        "status": "error",
                        "error": err,
                        "type": "error",
                    })
                } else {
                    json!({
                        "image": &image_ref,
                        "id": report.id,
                        "stream": report.stream,
                        "images": report.images,
                        "status": "Downloading",
                        "type": "progress",
                    })
                }
            }
            Err(e) => {
                json!({
                    "image": &image_ref,
                    "status": "error",
                    "error": e.to_string(),
                    "type": "error",
                })
            }
        };

        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
            // Client disconnected
            break;
        }
    }

    // Send completion message
    let _ = socket
        .send(Message::Text(
            json!({
                "image": &image_ref,
                "status": "Pull complete",
                "type": "complete",
            })
            .to_string().into(),
        ))
        .await;

    tracing::info!("✅ [WS] Image pull stream complete: {}", image_ref);
}

// ─── GET /api/images/inspect — Inspect Image ─────────────────────────────

#[derive(serde::Deserialize)]
pub struct InspectQuery {
    pub id: String,
}

/// GET /api/images/inspect?id=...
/// Returns detailed metadata about an image.
pub async fn inspect_image(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InspectQuery>,
) -> Result<Json<Value>, AppError> {
    let image_id = query.id;
    tracing::info!("🔍 Inspecting image: {}", image_id);

    let image = state.podman.images().get(&image_id);

    let inspect = image
        .inspect()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to inspect image '{}': {}", image_id, e)))?;

    Ok(Json(json!(inspect)))
}
