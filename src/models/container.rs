use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Request Types ───────────────────────────────────────────────────────────

/// Request body for creating a new container.
/// Maps to POST /api/containers
#[derive(Debug, Deserialize)]
pub struct CreateContainerRequest {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    pub limits: Option<ResourceLimits>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    pub restart_policy: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub command: Vec<String>,
    pub entrypoint: Option<Vec<String>>,
}

/// Request body for renaming a container.
/// Maps to POST /api/containers/:id/rename
#[derive(Debug, Deserialize)]
pub struct RenameContainerRequest {
    pub name: String,
}

/// Port mapping configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

/// Resource limits for a container
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResourceLimits {
    pub memory_mb: Option<u64>,
    pub cpu_cores: Option<f64>,
    pub disk_mb: Option<u64>,
    pub network_speed_mbps: Option<u64>,
}

/// Volume mount configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VolumeMount {
    pub host_path: String,
    pub container_path: String,
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// Standard container response returned by the API
#[derive(Debug, Serialize)]
pub struct ContainerResponse {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub created_at: Option<String>,
    pub ports: Vec<PortMapping>,
    pub limits: Option<ResourceLimits>,
    pub labels: HashMap<String, String>,
}

/// Summary of a container in list responses
#[derive(Debug, Serialize)]
pub struct ContainerSummary {
    pub id: String,
    pub names: Option<Vec<String>>,
    pub image: Option<String>,
    pub state: Option<String>,
    pub status: Option<String>,
    pub created: Option<i64>,
    pub ports: serde_json::Value,
    pub labels: Option<HashMap<String, String>>,
}

/// Query parameters for DELETE /api/containers/:id
#[derive(Debug, Deserialize)]
pub struct DeleteContainerQuery {
    #[serde(default)]
    pub remove_volumes: bool,
    #[serde(default)]
    pub force: bool,
}

/// Query parameters for POST /api/containers/:id/stop
#[derive(Debug, Deserialize)]
pub struct StopContainerQuery {
    pub timeout: Option<u64>,
}

/// Query parameters for GET /api/containers/:id/logs
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub tail: Option<usize>,
}
