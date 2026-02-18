use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::kube::{
    DeployKubeRequest, DeployKubeResponse, GenerateKubeFromPodQuery, GenerateKubeResponse,
    KubeStack, StackContainerStatus, StackStatusResponse,
};
use crate::AppState;

// ─── Deploy Kubernetes YAML ──────────────────────────────────────────────────

/// POST /api/kube/deploy
/// Uploads a Kubernetes YAML and deploys it using `podman play kube`.
pub async fn deploy_kube(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeployKubeRequest>,
) -> Result<Json<Value>, AppError> {
    if req.yaml.trim().is_empty() {
        return Err(AppError::BadRequest("YAML content is required".into()));
    }

    // Parse YAML to extract the resource name for the stack
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&req.yaml)
        .map_err(|e| AppError::BadRequest(format!("Invalid YAML: {}", e)))?;

    let resource_name = yaml_value
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unnamed-stack")
        .to_string();

    let stack_name = req.name.unwrap_or_else(|| resource_name.clone());

    if stack_name.is_empty() {
        return Err(AppError::BadRequest(
            "Stack name could not be determined. Provide a 'name' field or ensure metadata.name exists in the YAML.".into()
        ));
    }

    tracing::info!("📦 Deploying Kube stack '{}'", stack_name);

    // Create the stacks directory: {data_dir}/stacks/{stack_name}/
    let stacks_dir = state.config.agent.data_dir.join("stacks").join(&stack_name);
    tokio::fs::create_dir_all(&stacks_dir)
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to create stacks directory '{}': {}", stacks_dir.display(), e
        )))?;

    // Inject neox labels into the YAML metadata
    let final_yaml = req.yaml.clone();
    if !req.labels.is_empty() {
        // We'll prepend label comments for traceability; actual labeling
        // happens by modifying the YAML or via podman label flags.
        // For robustness, we keep the original YAML as-is and pass labels separately.
        tracing::info!("   Labels: {:?}", req.labels);
    }

    // Save the YAML to disk
    let yaml_path = stacks_dir.join("pod.yaml");
    tokio::fs::write(&yaml_path, &final_yaml)
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to save YAML to '{}': {}", yaml_path.display(), e
        )))?;

    tracing::info!("   YAML saved to: {}", yaml_path.display());

    // Save deployment metadata
    let metadata = json!({
        "name": stack_name,
        "deployed_at": chrono::Utc::now().to_rfc3339(),
        "labels": req.labels,
        "resource_name": resource_name,
        "start": req.start,
    });
    let meta_path = stacks_dir.join("metadata.json");
    tokio::fs::write(&meta_path, serde_json::to_string_pretty(&metadata).unwrap())
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to save metadata: {}", e
        )))?;

    // Execute `podman play kube` to deploy the YAML
    let mut cmd = tokio::process::Command::new("podman");
    cmd.arg("play").arg("kube");

    if req.start {
        // Default: podman play kube starts the pods
        cmd.arg("--start");
    } else {
        cmd.arg("--no-start");
    }

    cmd.arg(yaml_path.to_str().unwrap());

    tracing::info!("   Running: podman play kube {}", yaml_path.display());

    let output = cmd.output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'podman play kube': {}. Is podman installed and in PATH?", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Podman(format!(
            "podman play kube failed: {}", stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    tracing::info!("✅ Stack '{}' deployed successfully", stack_name);

    // Parse output to extract pod/container IDs
    let pods_created: Vec<String> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect();

    let response = DeployKubeResponse {
        success: true,
        stack_name: stack_name.clone(),
        yaml_path: yaml_path.display().to_string(),
        pods_created,
        message: format!("Stack '{}' deployed successfully", stack_name),
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── List Stacks ─────────────────────────────────────────────────────────────

/// GET /api/kube/stacks
/// Lists all deployed Kube stacks.
pub async fn list_stacks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, AppError> {
    let stacks_dir = state.config.agent.data_dir.join("stacks");

    // If the stacks directory doesn't exist yet, return empty list
    if !stacks_dir.exists() {
        return Ok(Json(json!({
            "stacks": Vec::<KubeStack>::new(),
            "total": 0,
        })));
    }

    let mut stacks: Vec<KubeStack> = Vec::new();

    let mut entries = tokio::fs::read_dir(&stacks_dir)
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to read stacks directory: {}", e
        )))?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        AppError::Internal(format!("Failed to read directory entry: {}", e))
    })? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let stack_name = entry
            .file_name()
            .to_str()
            .unwrap_or("unknown")
            .to_string();

        let yaml_path = path.join("pod.yaml");
        let meta_path = path.join("metadata.json");

        // Read metadata if it exists
        let (deployed_at, labels) = if meta_path.exists() {
            match tokio::fs::read_to_string(&meta_path).await {
                Ok(content) => {
                    let meta: serde_json::Value =
                        serde_json::from_str(&content).unwrap_or_default();
                    let deployed = meta
                        .get("deployed_at")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let lbls: HashMap<String, String> = meta
                        .get("labels")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    (deployed, lbls)
                }
                Err(_) => (None, HashMap::new()),
            }
        } else {
            (None, HashMap::new())
        };

        // Check if pods from this stack are running
        let status = get_stack_status_string(&stack_name).await;

        stacks.push(KubeStack {
            name: stack_name,
            yaml_path: yaml_path.display().to_string(),
            deployed_at,
            labels,
            status,
        });
    }

    let total = stacks.len();

    Ok(Json(json!({
        "stacks": stacks,
        "total": total,
    })))
}

// ─── Stack Up ────────────────────────────────────────────────────────────────

/// POST /api/kube/stacks/:name/up
/// Starts a previously deployed stack by re-running `podman play kube`.
pub async fn stack_up(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let yaml_path = state
        .config
        .agent
        .data_dir
        .join("stacks")
        .join(&name)
        .join("pod.yaml");

    if !yaml_path.exists() {
        return Err(AppError::NotFound(format!(
            "Stack '{}' not found. YAML file does not exist at '{}'",
            name,
            yaml_path.display()
        )));
    }

    tracing::info!("▶️ Starting stack '{}'", name);

    let output = tokio::process::Command::new("podman")
        .arg("play")
        .arg("kube")
        .arg("--start")
        .arg(yaml_path.to_str().unwrap())
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'podman play kube': {}", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Podman(format!(
            "Failed to start stack '{}': {}", name, stderr.trim()
        )));
    }

    tracing::info!("✅ Stack '{}' started", name);

    Ok(Json(json!({
        "success": true,
        "message": format!("Stack '{}' started successfully", name),
        "stack_name": name,
    })))
}

// ─── Stack Down ──────────────────────────────────────────────────────────────

/// POST /api/kube/stacks/:name/down
/// Tears down a stack using `podman play kube --down`.
pub async fn stack_down(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let yaml_path = state
        .config
        .agent
        .data_dir
        .join("stacks")
        .join(&name)
        .join("pod.yaml");

    if !yaml_path.exists() {
        return Err(AppError::NotFound(format!(
            "Stack '{}' not found", name
        )));
    }

    tracing::info!("⏹️ Tearing down stack '{}'", name);

    let output = tokio::process::Command::new("podman")
        .arg("play")
        .arg("kube")
        .arg("--down")
        .arg(yaml_path.to_str().unwrap())
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'podman play kube --down': {}", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Podman(format!(
            "Failed to tear down stack '{}': {}", name, stderr.trim()
        )));
    }

    tracing::info!("✅ Stack '{}' torn down", name);

    Ok(Json(json!({
        "success": true,
        "message": format!("Stack '{}' torn down successfully", name),
        "stack_name": name,
    })))
}

// ─── Delete Stack ────────────────────────────────────────────────────────────

/// DELETE /api/kube/stacks/:name
/// Tears down the stack and removes all associated files (YAML, metadata, volumes).
pub async fn delete_stack(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let stack_dir = state
        .config
        .agent
        .data_dir
        .join("stacks")
        .join(&name);

    if !stack_dir.exists() {
        return Err(AppError::NotFound(format!(
            "Stack '{}' not found", name
        )));
    }

    tracing::info!("🗑️ Deleting stack '{}' (teardown + remove files)", name);

    // First, tear down the running pods
    let yaml_path = stack_dir.join("pod.yaml");
    if yaml_path.exists() {
        let output = tokio::process::Command::new("podman")
            .arg("play")
            .arg("kube")
            .arg("--down")
            .arg(yaml_path.to_str().unwrap())
            .output()
            .await;

        match output {
            Ok(o) if !o.status.success() => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!(
                    "Warning: teardown of stack '{}' had issues: {}",
                    name,
                    stderr.trim()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Warning: could not teardown stack '{}': {}",
                    name, e
                );
            }
            _ => {}
        }
    }

    // Then remove the stack directory entirely
    tokio::fs::remove_dir_all(&stack_dir)
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to remove stack directory '{}': {}", stack_dir.display(), e
        )))?;

    tracing::info!("✅ Stack '{}' deleted", name);

    Ok(Json(json!({
        "success": true,
        "message": format!("Stack '{}' deleted successfully (pods torn down, files removed)", name),
        "stack_name": name,
    })))
}

// ─── Stack Status ────────────────────────────────────────────────────────────

/// GET /api/kube/stacks/:name/status
/// Returns the status of all containers within a deployed stack.
pub async fn stack_status(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let stack_dir = state
        .config
        .agent
        .data_dir
        .join("stacks")
        .join(&name);

    if !stack_dir.exists() {
        return Err(AppError::NotFound(format!(
            "Stack '{}' not found", name
        )));
    }

    tracing::info!("📊 Fetching status for stack '{}'", name);

    // Use `podman ps` with filters to find containers belonging to this stack
    // Pods created by `play kube` are named after the YAML metadata.name
    let output = tokio::process::Command::new("podman")
        .arg("ps")
        .arg("--all")
        .arg("--format")
        .arg("json")
        .arg("--filter")
        .arg(format!("pod={}", name))
        .output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'podman ps': {}", e
        )))?;

    let mut containers: Vec<StackContainerStatus> = Vec::new();

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            // podman ps --format json outputs an array of container objects
            let ps_output: Vec<serde_json::Value> =
                serde_json::from_str(&stdout).unwrap_or_default();

            for ctr in &ps_output {
                let id = ctr
                    .get("Id")
                    .or_else(|| ctr.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let names = ctr
                    .get("Names")
                    .or_else(|| ctr.get("names"))
                    .and_then(|v| {
                        if let Some(arr) = v.as_array() {
                            arr.first().and_then(|n| n.as_str())
                        } else {
                            v.as_str()
                        }
                    })
                    .unwrap_or("")
                    .to_string();

                let image = ctr
                    .get("Image")
                    .or_else(|| ctr.get("image"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let state = ctr
                    .get("State")
                    .or_else(|| ctr.get("state"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let status = ctr
                    .get("Status")
                    .or_else(|| ctr.get("status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let pod_name = ctr
                    .get("PodName")
                    .or_else(|| ctr.get("pod_name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                containers.push(StackContainerStatus {
                    id,
                    name: names,
                    image,
                    status,
                    state,
                    pod_name,
                });
            }
        }
    }

    // Also check the pod-level status
    let pod_output = tokio::process::Command::new("podman")
        .arg("pod")
        .arg("inspect")
        .arg(&name)
        .arg("--format")
        .arg("json")
        .output()
        .await;

    let overall_status = match &pod_output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let pod_info: serde_json::Value =
                serde_json::from_str(&stdout).unwrap_or_default();
            pod_info
                .get("State")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        }
        _ => {
            // Derive status from containers
            if containers.is_empty() {
                "stopped".to_string()
            } else if containers.iter().all(|c| c.state == "running") {
                "running".to_string()
            } else if containers.iter().any(|c| c.state == "running") {
                "degraded".to_string()
            } else {
                "stopped".to_string()
            }
        }
    };

    let total = containers.len();

    let response = StackStatusResponse {
        stack_name: name,
        status: overall_status,
        containers,
        total_containers: total,
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Generate Kube YAML from Pod ─────────────────────────────────────────────

/// POST /api/kube/generate/:pod_id
/// Exports an existing Pod to Kubernetes YAML using `podman generate kube`.
pub async fn generate_kube_from_pod(
    State(_state): State<Arc<AppState>>,
    Path(pod_id): Path<String>,
    Query(query): Query<GenerateKubeFromPodQuery>,
) -> Result<Json<Value>, AppError> {
    tracing::info!("📄 Generating Kube YAML for pod '{}'", pod_id);

    let mut cmd = tokio::process::Command::new("podman");
    cmd.arg("generate").arg("kube");

    if query.service {
        cmd.arg("--service");
    }

    cmd.arg(&pod_id);

    let output = cmd.output()
        .await
        .map_err(|e| AppError::Internal(format!(
            "Failed to execute 'podman generate kube': {}", e
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Podman(format!(
            "Failed to generate Kube YAML for '{}': {}", pod_id, stderr.trim()
        )));
    }

    let yaml = String::from_utf8_lossy(&output.stdout).to_string();

    // Try to get the pod name from the YAML
    let pod_name = serde_yaml::from_str::<serde_yaml::Value>(&yaml)
        .ok()
        .and_then(|v| {
            v.get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| pod_id.clone());

    let response = GenerateKubeResponse {
        pod_id: pod_id.clone(),
        pod_name,
        yaml,
        service_included: query.service,
    };

    Ok(Json(serde_json::to_value(response).unwrap()))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Quick helper to get a stack's status string via `podman pod inspect`.
async fn get_stack_status_string(stack_name: &str) -> String {
    let output = tokio::process::Command::new("podman")
        .arg("pod")
        .arg("inspect")
        .arg(stack_name)
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let pod_info: serde_json::Value =
                serde_json::from_str(&stdout).unwrap_or_default();
            pod_info
                .get("State")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        }
        _ => "not_deployed".to_string(),
    }
}
