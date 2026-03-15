use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeResponse {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: String,
    pub labels: std::collections::HashMap<String, String>,
    pub options: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVolumeRequest {
    pub name: String,
    pub driver: Option<String>,
    pub labels: Option<std::collections::HashMap<String, String>>,
    pub options: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct VolumeListResponse {
    pub volumes: Vec<VolumeResponse>,
}
