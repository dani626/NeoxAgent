use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Request Types ───────────────────────────────────────────────────────────

/// Request body for creating a new network.
/// Maps to POST /api/networks
#[derive(Debug, Deserialize)]
pub struct CreateNetworkRequest {
    /// Network name
    pub name: String,
    /// Network driver (e.g. "bridge", "macvlan"), defaults to "bridge"
    pub driver: Option<String>,
    /// Whether DNS resolution is enabled
    #[serde(default)]
    pub dns_enabled: bool,
    /// Whether this is an internal network (no external routing)
    #[serde(default)]
    pub internal: bool,
    /// Whether IPv6 is enabled
    #[serde(default)]
    pub ipv6_enabled: bool,
    /// Subnet specification (CIDR format, e.g. "10.89.0.0/24")
    pub subnet: Option<String>,
    /// Gateway address
    pub gateway: Option<String>,
    /// Labels
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// Standard network response
#[derive(Debug, Serialize)]
pub struct NetworkResponse {
    pub name: String,
    pub id: Option<String>,
    pub driver: Option<String>,
    pub dns_enabled: bool,
    pub internal: bool,
    pub ipv6_enabled: bool,
    pub subnets: Vec<SubnetInfo>,
    pub labels: HashMap<String, String>,
    pub created: Option<String>,
    pub network_interface: Option<String>,
}

/// Subnet information
#[derive(Debug, Serialize)]
pub struct SubnetInfo {
    pub subnet: Option<String>,
    pub gateway: Option<String>,
}

/// Response for listing networks
#[derive(Debug, Serialize)]
pub struct NetworkListResponse {
    pub networks: Vec<NetworkResponse>,
    pub total: usize,
}

/// Query parameters for DELETE /api/networks/:id
#[derive(Debug, Deserialize)]
pub struct DeleteNetworkQuery {
    /// Whether to force remove (also disconnects containers)
    #[serde(default)]
    pub force: bool,
}
