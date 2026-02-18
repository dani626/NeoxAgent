use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

use podman_api::opts::{NetworkCreateOpts, NetworkListOpts};

use crate::error::AppError;
use crate::models::network::{
    CreateNetworkRequest, DeleteNetworkQuery, NetworkResponse, SubnetInfo,
};
use crate::AppState;

// ─── List Networks ───────────────────────────────────────────────────────────

/// GET /api/networks
/// Lists all Podman networks (Netavark).
pub async fn list_networks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, AppError> {
    let networks = state.podman.networks()
        .list(&NetworkListOpts::builder().build())
        .await
        .map_err(|e| AppError::Podman(format!("Failed to list networks: {}", e)))?;

    let responses: Vec<NetworkResponse> = networks.iter().map(|n| {
        build_network_response(n)
    }).collect();

    let total = responses.len();

    Ok(Json(json!({
        "networks": responses,
        "total": total,
    })))
}

// ─── Create Network ──────────────────────────────────────────────────────────

/// POST /api/networks
/// Creates a new Podman network.
pub async fn create_network(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNetworkRequest>,
) -> Result<Json<Value>, AppError> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Network name is required".into()));
    }

    tracing::info!("🌐 Creating network '{}'", req.name);

    let mut builder = NetworkCreateOpts::builder()
        .name(&req.name)
        .dns_enabled(req.dns_enabled)
        .internal(req.internal)
        .ipv6_enabled(req.ipv6_enabled);

    if let Some(driver) = &req.driver {
        builder = builder.driver(driver.as_str());
    }

    if !req.labels.is_empty() {
        builder = builder.labels(req.labels.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    // Subnet configuration
    if let Some(subnet) = &req.subnet {
        let mut subnet_obj = podman_api::models::Subnet {
            subnet: Some(subnet.clone()),
            gateway: None,
            lease_range: None,
        };
        if let Some(gw) = &req.gateway {
            subnet_obj.gateway = Some(gw.clone());
        }
        builder = builder.subnets([subnet_obj]);
    }

    let net_opts = builder.build();

    // Networks.create() returns the models::Network directly
    let info = state.podman.networks()
        .create(&net_opts)
        .await
        .map_err(|e| AppError::Podman(format!("Failed to create network '{}': {}", req.name, e)))?;

    tracing::info!("✅ Network '{}' created", req.name);

    let response = build_network_response(&info);

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Inspect Network ─────────────────────────────────────────────────────────

/// GET /api/networks/:id
/// Gets detailed information about a network.
pub async fn get_network(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    // networks().get() returns the API handle, .inspect() returns models::Network
    let info = state.podman.networks().get(&id)
        .inspect()
        .await
        .map_err(|e| AppError::Podman(format!("Failed to inspect network '{}': {}", id, e)))?;

    let response = build_network_response(&info);

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Delete Network ──────────────────────────────────────────────────────────

/// DELETE /api/networks/:id
/// Deletes a Podman network.
pub async fn delete_network(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<DeleteNetworkQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("🗑️ Deleting network '{}' (force: {})", id, query.force);

    let network = state.podman.networks().get(&id);

    if query.force {
        network.remove()
            .await
            .map_err(|e| AppError::Podman(format!("Failed to force-remove network '{}': {}", id, e)))?;
    } else {
        network.delete()
            .await
            .map_err(|e| AppError::Podman(format!("Failed to delete network '{}': {}", id, e)))?;
    }

    tracing::info!("✅ Network '{}' deleted", id);

    Ok(Json(json!({
        "success": true,
        "message": format!("Network '{}' deleted successfully", id),
        "network_id": id,
    })))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Build a NetworkResponse from a podman_api Network model.
fn build_network_response(net: &podman_api::models::Network) -> NetworkResponse {
    let subnets: Vec<SubnetInfo> = net.subnets.as_ref()
        .map(|subs| {
            subs.iter().map(|s| SubnetInfo {
                subnet: s.subnet.clone(),
                gateway: s.gateway.clone(),
            }).collect()
        })
        .unwrap_or_default();

    let labels = net.labels.clone().unwrap_or_default();

    NetworkResponse {
        name: net.name.clone().unwrap_or_default(),
        id: net.id.clone(),
        driver: net.driver.clone(),
        dns_enabled: net.dns_enabled.unwrap_or(false),
        internal: net.internal.unwrap_or(false),
        ipv6_enabled: net.ipv_6_enabled.unwrap_or(false),
        subnets,
        labels,
        created: net.created.map(|t| t.to_rfc3339()),
        network_interface: net.network_interface.clone(),
    }
}
