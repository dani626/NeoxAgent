use serde::{Deserialize, Serialize};

// ─── Query Parameters ────────────────────────────────────────────────────────

/// Query params for file and directory operations.
/// Used by GET /files, GET /files/content, PUT /files/content, DELETE /files, etc.
#[derive(Debug, Deserialize)]
pub struct FilePathQuery {
    /// Path relative to the server's volume root, e.g. "/" or "/plugins/MyPlugin.jar"
    #[serde(default = "default_root")]
    pub path: String,
}

fn default_root() -> String {
    "/".to_string()
}

/// Query for upload endpoint (target directory)
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    /// Directory path to upload into (relative to volume root)
    #[serde(default = "default_root")]
    pub path: String,
}

/// Query for download endpoint
#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    /// File or directory to download (relative to volume root)
    pub path: String,
}

// ─── Request Bodies ──────────────────────────────────────────────────────────

/// POST /api/pods/:id/files/rename — Rename/move a file or directory
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    /// Current path (relative to volume root)
    pub from: String,
    /// New path (relative to volume root)
    pub to: String,
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// A single file or directory entry
#[derive(Debug, Serialize)]
pub struct FileEntry {
    /// File or directory name
    pub name: String,
    /// "file" or "dir"
    #[serde(rename = "type")]
    pub entry_type: String,
    /// Size in bytes (0 for directories)
    pub size: u64,
    /// Last modified time (ISO 8601)
    pub modified: String,
    /// Unix permissions string (e.g., "rw-r--r--"), empty on Windows
    pub permissions: String,
    /// Number of children (only for directories)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<usize>,
}

/// Response for listing a directory
#[derive(Debug, Serialize)]
pub struct ListDirectoryResponse {
    /// The requested path
    pub path: String,
    /// Items in the directory
    pub items: Vec<FileEntry>,
}

/// Response for reading file content
#[derive(Debug, Serialize)]
pub struct FileContentResponse {
    /// File path (relative to volume root)
    pub path: String,
    /// File content as UTF-8 string
    pub content: String,
    /// File size in bytes
    pub size: u64,
    /// Last modified time
    pub modified: String,
}
