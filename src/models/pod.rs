use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::container::{PortMapping, ResourceLimits, VolumeMount};

// ─── Request Types ───────────────────────────────────────────────────────────

/// Request body for creating a new Pod.
/// Maps to POST /api/pods
#[derive(Debug, Deserialize)]
pub struct CreatePodRequest {
    /// Pod name (e.g. "mc-proxy-1")
    pub name: String,
    /// Proxy sidecar configuration
    pub proxy: Option<ProxyConfig>,
    /// Containers to create inside the pod
    #[serde(default)]
    pub containers: Vec<PodContainerSpec>,
    /// Labels to apply to the pod
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// DNS servers for the pod
    #[serde(default)]
    pub dns_servers: Vec<String>,
    /// Hostname override
    pub hostname: Option<String>,
    /// Network to connect to
    pub network: Option<String>,
}

/// Configuration for the hev-socks5-tproxy proxy sidecar.
/// Uses TPROXY (no TUN) for transparent proxying with SOCKS5 authentication support.
#[derive(Debug, Deserialize)]
pub struct ProxyConfig {
    /// Whether the proxy is enabled
    pub enabled: bool,
    /// Proxy type ("hev-socks5-tproxy")
    #[serde(rename = "type")]
    pub proxy_type: Option<String>,
    /// Container image for the proxy sidecar
    pub image: Option<String>,
    /// SOCKS5 proxy URL (e.g. "socks5://user:pass@proxy.com:1080")
    /// Supports authentication: username and password are parsed from the URL.
    pub socks5_url: Option<String>,
    /// DNS server to use inside the proxy (default: "1.1.1.1")
    pub dns: Option<String>,
    /// Additional environment variables for the proxy container
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Log level for hev-socks5-tproxy (default: "warn")
    pub loglevel: Option<String>,
}

/// A container specification within a pod create request.
/// Reuses some types from the containers model but scoped to pods.
#[derive(Debug, Deserialize)]
pub struct PodContainerSpec {
    /// Container name within the pod
    pub name: String,
    /// Image to use
    pub image: String,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Port mappings (applied at pod level)
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    /// Resource limits
    pub limits: Option<ResourceLimits>,
    /// Volume mounts
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    /// Labels to apply to this container
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// Restart policy
    pub restart_policy: Option<String>,
    /// Command override
    #[serde(default)]
    pub command: Vec<String>,
    /// Entrypoint override
    pub entrypoint: Option<Vec<String>>,
}

/// Request body for adding a container to an existing pod.
/// Maps to POST /api/pods/:id/containers
#[derive(Debug, Deserialize)]
pub struct AddContainerToPodRequest {
    /// Container specification
    #[serde(flatten)]
    pub container: PodContainerSpec,
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// Standard pod response returned by the API
#[derive(Debug, Serialize)]
pub struct PodResponse {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: Option<String>,
    pub hostname: Option<String>,
    pub labels: HashMap<String, String>,
    pub containers: Vec<PodContainerInfo>,
    pub proxy_enabled: bool,
    pub infra_id: Option<String>,
}

/// Information about a container inside a pod
#[derive(Debug, Serialize)]
pub struct PodContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

/// Response for listing pods
#[derive(Debug, Serialize)]
pub struct PodListResponse {
    pub pods: Vec<PodSummary>,
    pub total: usize,
}

/// Summary of a pod in list responses
#[derive(Debug, Serialize)]
pub struct PodSummary {
    pub id: String,
    pub name: Option<String>,
    pub status: Option<String>,
    pub created: Option<String>,
    pub num_containers: Option<i64>,
    pub infra_id: Option<String>,
    pub labels: Option<HashMap<String, String>>,
}

/// Query parameters for DELETE /api/pods/:id
#[derive(Debug, Deserialize)]
pub struct DeletePodQuery {
    /// Whether to force remove (also removes containers)
    #[serde(default = "default_true")]
    pub force: bool,
}

fn default_true() -> bool {
    true
}

/// Response for pod lifecycle actions
#[derive(Debug, Serialize)]
pub struct PodActionResponse {
    pub success: bool,
    pub message: String,
    pub pod_id: String,
}

/// Query parameters for generating kube YAML
#[derive(Debug, Deserialize)]
pub struct GenerateKubeQuery {
    /// Whether to also generate a service definition
    #[serde(default)]
    pub service: bool,
}
