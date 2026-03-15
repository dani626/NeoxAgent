use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Request Types ───────────────────────────────────────────────────────────

/// Request body for creating a new container.
/// Maps to POST /api/containers
#[derive(Debug, Deserialize)]
pub struct CreateContainerRequest {
    /// Container name (e.g. "minecraft-survival")
    pub name: String,
    /// Image to use (e.g. "docker.io/itzg/minecraft-server:latest")
    pub image: String,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Port mappings
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    /// Resource limits
    pub limits: Option<ResourceLimits>,
    /// Volume mounts
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    /// Network to connect to (default: "default") — reserved for Phase 3
    #[allow(dead_code)]
    pub network: Option<String>,
    /// Restart policy (default from config)
    pub restart_policy: Option<String>,
    /// Labels to apply
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// Command override
    #[serde(default)]
    pub command: Vec<String>,
    /// Entrypoint override
    pub entrypoint: Option<Vec<String>>,
}

/// Port mapping configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PortMapping {
    /// Host port
    pub host: u16,
    /// Container port
    pub container: u16,
    /// Protocol (tcp/udp), defaults to "tcp"
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

/// Resource limits for a container
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResourceLimits {
    /// Memory limit in megabytes
    pub memory_mb: Option<u64>,
    /// CPU core limit (e.g. 2.0 for 2 cores)
    pub cpu_cores: Option<f64>,
    /// Disk limit in megabytes (tracked by neoxagent, not enforced by Podman)
    pub disk_mb: Option<u64>,
    /// Network speed limit in megabits per second (enforced via tc)
    pub network_speed_mbps: Option<u64>,
}

/// Volume mount configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VolumeMount {
    /// Path on the host
    pub host_path: String,
    /// Path inside the container
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

/// Response for container list
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ContainerListResponse {
    pub containers: Vec<ContainerSummary>,
    pub total: usize,
}

/// Summary of a container in list responses
#[derive(Debug, Serialize)]
#[allow(dead_code)]
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

/// Simple action response (start, stop, restart, kill)
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ActionResponse {
    pub success: bool,
    pub message: String,
    pub container_id: String,
}

/// Query parameters for DELETE /api/containers/:id
#[derive(Debug, Deserialize)]
pub struct DeleteContainerQuery {
    /// Whether to also remove associated volumes
    #[serde(default)]
    pub remove_volumes: bool,
    /// Whether to force remove a running container
    #[serde(default)]
    pub force: bool,
}

/// Query parameters for POST /api/containers/:id/stop
#[derive(Debug, Deserialize)]
pub struct StopContainerQuery {
    /// Timeout in seconds before sending SIGKILL (default: 10)
    pub timeout: Option<u64>,
}

/// Query parameters for GET /api/containers/:id/logs
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Number of lines to show from the end of the logs
    pub tail: Option<usize>,
}
