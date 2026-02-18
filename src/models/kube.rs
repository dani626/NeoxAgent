use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Deploy YAML ─────────────────────────────────────────────────────────────

/// POST /api/kube/deploy — Body for deploying a Kubernetes YAML
#[derive(Debug, Deserialize)]
pub struct DeployKubeRequest {
    /// The Kubernetes YAML content (as string)
    pub yaml: String,

    /// Optional: a name to identify this stack (defaults to the pod/deployment name in YAML)
    #[serde(default)]
    pub name: Option<String>,

    /// Whether to start the stack immediately after deploy (default: true)
    #[serde(default = "default_true")]
    pub start: bool,

    /// Extra labels to add to the deployed resources
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

/// Response after deploying a Kubernetes YAML
#[derive(Debug, Serialize)]
pub struct DeployKubeResponse {
    pub success: bool,
    pub stack_name: String,
    pub yaml_path: String,
    pub pods_created: Vec<String>,
    pub message: String,
}

// ─── Stack Info ──────────────────────────────────────────────────────────────

/// A deployed Kube stack (saved YAML + running pods)
#[derive(Debug, Serialize)]
pub struct KubeStack {
    /// Stack name (directory name in stacks/)
    pub name: String,

    /// Path to the YAML file on disk
    pub yaml_path: String,

    /// When the stack was deployed
    pub deployed_at: Option<String>,

    /// Labels applied to the stack
    pub labels: HashMap<String, String>,

    /// Current status: "running", "stopped", "unknown"
    pub status: String,
}

/// Container status within a stack
#[derive(Debug, Serialize)]
pub struct StackContainerStatus {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub pod_name: Option<String>,
}

/// Full stack status response
#[derive(Debug, Serialize)]
pub struct StackStatusResponse {
    pub stack_name: String,
    pub status: String,
    pub containers: Vec<StackContainerStatus>,
    pub total_containers: usize,
}

// ─── Generate Kube ───────────────────────────────────────────────────────────

/// Query params for POST /api/kube/generate/:pod_id
#[derive(Debug, Deserialize)]
pub struct GenerateKubeFromPodQuery {
    /// Whether to include a Kubernetes Service definition in the YAML
    #[serde(default)]
    pub service: bool,
}

/// Response for generating Kube YAML from an existing pod
#[derive(Debug, Serialize)]
pub struct GenerateKubeResponse {
    pub pod_id: String,
    pub pod_name: String,
    pub yaml: String,
    pub service_included: bool,
}
