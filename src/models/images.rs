use serde::{Deserialize, Serialize};

// ─── Image Types ─────────────────────────────────────────────────────────────

/// Response for a single image in the list
#[derive(Debug, Serialize)]
pub struct ImageInfo {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub repo_digests: Vec<String>,
    pub size: Option<i64>,
    pub virtual_size: Option<i64>,
    pub created: Option<i64>,
    pub containers: Option<i64>,
    pub read_only: Option<bool>,
    pub dangling: Option<bool>,
}

/// Request body for pulling an image
#[derive(Debug, Deserialize)]
pub struct PullImageRequest {
    /// Full image reference (e.g. "docker.io/library/alpine:latest")
    pub image: String,
}

/// Response after pulling an image
#[derive(Debug, Serialize)]
pub struct PullImageResponse {
    pub success: bool,
    pub image: String,
    pub id: Option<String>,
    pub message: String,
}

/// Query parameters for image search
#[derive(Debug, Deserialize)]
pub struct ImageSearchQuery {
    /// Search term (e.g. "minecraft")
    pub q: String,
    /// Maximum number of results (default: 25)
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    25
}

/// A search result from a container registry
#[derive(Debug, Serialize)]
pub struct ImageSearchResult {
    pub index: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub stars: Option<i64>,
    pub official: Option<String>,
    pub automated: Option<String>,
    pub tag: Option<String>,
}

// ─── Systemd Types ───────────────────────────────────────────────────────────

/// Response for systemd service generation
#[derive(Debug, Serialize)]
pub struct SystemdGenerateResponse {
    pub pod_id: String,
    pub service_name: String,
    pub service_file_path: String,
    pub unit_content: String,
    pub message: String,
}

/// Response for systemd status
#[derive(Debug, Serialize)]
pub struct SystemdStatusResponse {
    pub pod_id: String,
    pub service_name: String,
    pub active_state: String,
    pub sub_state: String,
    pub enabled: bool,
    pub description: String,
}
