use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::container::{PortMapping, ResourceLimits, VolumeMount};

// ─── Request Types ───────────────────────────────────────────────────────────

/// Request body for creating a new Pod.
/// Maps to POST /api/pods
#[derive(Debug, Deserialize)]
pub struct CreatePodRequest {
    pub name: String,
    pub proxy: Option<ProxyConfig>,
    #[serde(default)]
    pub containers: Vec<PodContainerSpec>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub dns_servers: Vec<String>,
    pub hostname: Option<String>,
    pub network: Option<String>,
}

/// Configuration for the hev-socks5-tproxy proxy sidecar.
#[derive(Debug, Deserialize)]
pub struct ProxyConfig {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub proxy_type: Option<String>,
    pub image: Option<String>,
    pub socks5_url: Option<String>,
    pub dns: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub loglevel: Option<String>,
    /// UDP tunneling mode for hev-socks5-tproxy.
    /// - 'tcp': tunnel UDP over TCP (default, works with most SOCKS5 proxies
    ///   that don't support UDP ASSOCIATE)
    /// - 'udp': native SOCKS5 UDP ASSOCIATE (requires proxy support)
    pub udp_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PodContainerSpec {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    pub limits: Option<ResourceLimits>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    pub restart_policy: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    pub entrypoint: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct AddContainerToPodRequest {
    #[serde(flatten)]
    pub container: PodContainerSpec,
}

#[derive(Debug, Deserialize)]
pub struct RenamePodRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProxyRequest {
    pub proxy: ProxyConfig,
}

// ─── Response Types ──────────────────────────────────────────────────────────

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
    /// SOCKS5 proxy URL persisted as pod label neox.proxy.url (Opción B).
    /// Only present when proxy_enabled is true and the label exists.
    pub proxy_url: Option<String>,
    pub infra_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PodContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct PodListResponse {
    pub pods: Vec<PodSummary>,
    pub total: usize,
}

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

#[derive(Debug, Deserialize)]
pub struct DeletePodQuery {
    #[serde(default = "default_true")]
    pub force: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct PodActionResponse {
    pub success: bool,
    pub message: String,
    pub pod_id: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateKubeQuery {
    #[serde(default)]
    pub service: bool,
}
