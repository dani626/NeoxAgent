use serde::{Deserialize, Serialize};

// ─── Request Types ───────────────────────────────────────────────────────────

/// POST /api/pods/:id/backups — Optional body to configure backup behavior
#[derive(Debug, Deserialize)]
pub struct CreateBackupRequest {
    /// Whether to stop the server before backup for data consistency.
    /// Overrides the config default if provided.
    #[serde(default)]
    pub stop_server: Option<bool>,

    /// Optional description/tag for this backup
    #[serde(default)]
    pub description: Option<String>,
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// Metadata for a single backup file
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackupInfo {
    /// Unique backup ID (UUID v4)
    pub id: String,
    /// Pod/server ID this backup belongs to
    pub pod_id: String,
    /// Backup file size in bytes
    pub size_bytes: u64,
    /// Size in MB for display
    pub size_mb: f64,
    /// When the backup was created (ISO 8601)
    pub created_at: String,
    /// SHA256 checksum of the archive
    pub checksum_sha256: String,
    /// Whether the server was stopped during backup
    pub server_was_stopped: bool,
    /// The backup filename on disk
    pub filename: String,
    /// Optional user-provided description
    pub description: Option<String>,
}

/// Response for listing backups
#[derive(Debug, Serialize)]
pub struct BackupListResponse {
    pub pod_id: String,
    pub backups: Vec<BackupInfo>,
    pub total: usize,
}
