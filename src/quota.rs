use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;

/// Struct containing details about the filesystem hosting a path.
pub struct FsInfo {
    pub fstype: String,
    pub mountpoint: String,
}

/// Generates a deterministic project ID from a directory path.
/// Maps to a valid u32 project ID (> 10000 to avoid conflicts with reserved system IDs).
pub fn get_project_id(path: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash_val = hasher.finish();
    (hash_val % 100000000) as u32 + 10000
}

/// Detects the filesystem type and mount point of the directory containing the path.
pub fn get_fs_info(path: &str) -> Option<FsInfo> {
    let output = Command::new("df")
        .args(["-T", path])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let out_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = out_str.lines().collect();
    if lines.len() < 2 {
        return None;
    }

    // Line 1 is the header, line 2 has the actual data
    let parts: Vec<&str> = lines[1].split_whitespace().collect();
    // Expected columns: Filesystem, Type, 1K-blocks, Used, Available, Use%, Mounted on
    if parts.len() < 7 {
        return None;
    }

    let fstype = parts[1].to_string();
    let mountpoint = parts[6].to_string();

    Some(FsInfo { fstype, mountpoint })
}

/// Applies a Linux project quota (Option A: ext4/XFS project quotas) to a given host directory path.
pub fn apply_project_quota(path: &str, limit_mb: u64) -> Result<(), String> {
    // 1. Ensure the directory exists
    if !std::path::Path::new(path).exists() {
        if let Err(e) = std::fs::create_dir_all(path) {
            return Err(format!("Failed to create directory '{}': {}", path, e));
        }
    }

    let project_id = get_project_id(path);
    let fs_info = get_fs_info(path).ok_or_else(|| "Failed to get filesystem info".to_string())?;

    tracing::info!(
        "💾 Applying disk quota for '{}': {} MB (Project ID: {}, Filesystem: {}, Mount: {})",
        path, limit_mb, project_id, fs_info.fstype, fs_info.mountpoint
    );

    // 2. Set the project ID attribute on the directory (unified FS_IOC_FSSETXATTR ioctl via chattr +P)
    let chattr_out = Command::new("chattr")
        .args(["+P", "-p", &project_id.to_string(), path])
        .output();

    match chattr_out {
        Ok(output) if output.status.success() => {
            tracing::debug!("Successfully assigned project ID to directory");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("chattr failed: {}", stderr.trim()));
        }
        Err(e) => {
            return Err(format!("Failed to execute chattr: {}", e));
        }
    }

    // 3. Set the actual quota limit based on filesystem type
    let quota_res = match fs_info.fstype.as_str() {
        "xfs" => {
            // XFS quota tool: limit -p bsoft=XXm bhard=XXm <project_id> <mountpoint>
            Command::new("xfs_quota")
                .args([
                    "-x",
                    "-c",
                    &format!("limit -p bsoft={limit_mb}m bhard={limit_mb}m {project_id}"),
                    &fs_info.mountpoint,
                ])
                .output()
        }
        "ext4" => {
            // ext4 quota tool: setquota -P <project_id> <soft_kb> <hard_kb> 0 0 <mountpoint>
            let limit_kb = limit_mb * 1024;
            Command::new("setquota")
                .args([
                    "-P",
                    &project_id.to_string(),
                    &limit_kb.to_string(),
                    &limit_kb.to_string(),
                    "0",
                    "0",
                    &fs_info.mountpoint,
                ])
                .output()
        }
        _ => {
            return Err(format!(
                "Filesystem type '{}' does not support project quotas. Supported types: ext4, xfs.",
                fs_info.fstype
            ));
        }
    };

    match quota_res {
        Ok(output) if output.status.success() => {
            tracing::info!("✅ Disk quota of {} MB applied successfully to {}", limit_mb, path);
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("quota tool failed: {}", stderr.trim()))
        }
        Err(e) => Err(format!("Failed to execute quota command: {}", e)),
    }
}

/// Safe wrapper to apply a quota limit only if resource limits define a non-zero disk limit.
pub fn apply_quota_if_needed(host_path: &str, limits: &Option<crate::models::container::ResourceLimits>) {
    if let Some(ref limits_ref) = limits {
        if let Some(disk_mb) = limits_ref.disk_mb {
            if disk_mb > 0 {
                if let Err(e) = apply_project_quota(host_path, disk_mb) {
                    tracing::warn!(
                        "⚠️ Could not apply disk quota to path '{}' (limit: {} MB): {}. \
                         Note: Project quotas require root/CAP_SYS_RESOURCE privileges and quota-enabled filesystems.",
                        host_path, disk_mb, e
                    );
                }
            }
        }
    }
}
