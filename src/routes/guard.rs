//! # neox-guard
//!
//! Host-level IP leak protection.
//!
//! ## Problem
//! When the VPS reboots, Podman restores pods via `restart: on-failure` but
//! does NOT guarantee that the hev-socks5-tproxy sidecar starts BEFORE the
//! main containers. There is a window where main containers can send traffic
//! through the raw VPS IP before the sidecar installs its iptables rules.
//!
//! ## Solution
//! A systemd `oneshot` service (`neox-guard.service`) that:
//!   1. Runs BEFORE `podman.service` and `podman-restart.service`
//!   2. Installs a DROP rule on the FORWARD chain (blocks container traffic)
//!   3. Stays "active" (RemainAfterExit) so the rule persists
//!
//! The sidecar, after confirming hev is healthy and FAILSAFE is active,
//! calls POST /api/guard/lift to remove the host FORWARD DROP rule.
//!
//! ## Endpoints
//!   POST /api/guard/install  — write + enable neox-guard.service
//!   POST /api/guard/lift     — remove host FORWARD DROP (called by sidecar)
//!   GET  /api/guard/status   — check if guard rule is active

use axum::Json;
use serde_json::{json, Value};

use crate::error::AppError;

const SERVICE_NAME: &str = "neox-guard.service";
const SERVICE_PATH: &str = "/etc/systemd/system/neox-guard.service";
const IPTABLES_COMMENT: &str = "neox-guard-forward-drop";

// ─── Service unit content ─────────────────────────────────────────────────────

fn guard_service_unit() -> String {
    format!(
        r#"[Unit]
Description=Neox host-level container IP leak guard
Documentation=https://github.com/dani626/NeoxAgent
# Must run BEFORE Podman restores containers on boot
Before=podman.service podman-restart.service network-online.target
After=network.target
DefaultDependencies=no
ConditionPathExists=/sbin/iptables

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/sbin/iptables -I FORWARD 1 -m comment --comment "{comment}" -j DROP
ExecStop=/sbin/iptables -D FORWARD -m comment --comment "{comment}" -j DROP
ExecStopPost=/sbin/iptables -D FORWARD -m comment --comment "{comment}" -j DROP

[Install]
WantedBy=multi-user.target
"#,
        comment = IPTABLES_COMMENT
    )
}

// ─── POST /api/guard/install ──────────────────────────────────────────────────

/// Writes the neox-guard.service unit file, reloads systemd, and enables it.
/// Safe to call multiple times (idempotent).
pub async fn install_guard() -> Result<Json<Value>, AppError> {
    tracing::info!("\u{1f6e1}\u{fe0f}  Installing neox-guard.service");

    // 1. Write unit file
    let unit = guard_service_unit();
    tokio::fs::write(SERVICE_PATH, &unit)
        .await
        .map_err(|e| AppError::Internal(
            format!("Failed to write {}: {}", SERVICE_PATH, e)
        ))?;

    // 2. Reload systemd daemon
    run_cmd("systemctl", &["daemon-reload"]).await?;

    // 3. Enable (so it survives reboots)
    run_cmd("systemctl", &["enable", SERVICE_NAME]).await?;

    // 4. Start now (installs the DROP rule immediately)
    run_cmd("systemctl", &["start", SERVICE_NAME]).await?;

    tracing::info!("\u{2705} neox-guard.service installed, enabled and started");

    Ok(Json(json!({
        "success": true,
        "service": SERVICE_NAME,
        "path": SERVICE_PATH,
        "message": "Host-level guard active. FORWARD DROP rule installed. Call /api/guard/lift after proxy sidecar is healthy.",
    })))
}

// ─── POST /api/guard/lift ─────────────────────────────────────────────────────

/// Stops neox-guard.service (removes the FORWARD DROP rule).
/// Called by the sidecar once hev is running and FAILSAFE is confirmed active.
/// After this, only hev-marked packets can leave the pod network namespace.
pub async fn lift_guard() -> Result<Json<Value>, AppError> {
    tracing::info!("\u{1f513} Lifting neox-guard (proxy sidecar is healthy)");

    // Stop the service — ExecStop removes the iptables rule
    run_cmd("systemctl", &["stop", SERVICE_NAME]).await?;

    // Belt-and-suspenders: also delete the rule directly in case systemd
    // already cleaned it or the service was in a weird state.
    let _ = run_cmd(
        "iptables",
        &["-D", "FORWARD", "-m", "comment", "--comment", IPTABLES_COMMENT, "-j", "DROP"],
    ).await;

    tracing::info!("\u{2705} neox-guard lifted. HEV_FAILSAFE is now the only safety net.");

    Ok(Json(json!({
        "success": true,
        "message": "Host FORWARD DROP rule removed. HEV_FAILSAFE (inside pod netns) remains active.",
    })))
}

// ─── GET /api/guard/status ────────────────────────────────────────────────────

/// Returns whether the host FORWARD DROP guard rule is currently active.
pub async fn guard_status() -> Result<Json<Value>, AppError> {
    // Check if the iptables rule exists
    let rule_active = check_rule_exists().await;

    // Check systemd service state
    let svc_output = tokio::process::Command::new("systemctl")
        .args(["show", SERVICE_NAME, "--property=ActiveState,UnitFileState", "--no-pager"])
        .output()
        .await
        .ok();

    let (active_state, unit_file_state) = svc_output
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let mut active = "unknown".to_string();
            let mut ufs = "unknown".to_string();
            for line in stdout.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    match k.trim() {
                        "ActiveState" => active = v.trim().to_string(),
                        "UnitFileState" => ufs = v.trim().to_string(),
                        _ => {}
                    }
                }
            }
            (active, ufs)
        })
        .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

    Ok(Json(json!({
        "guard_rule_active": rule_active,
        "service_name": SERVICE_NAME,
        "service_active_state": active_state,
        "service_enabled": unit_file_state == "enabled",
        "iptables_comment": IPTABLES_COMMENT,
    })))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn check_rule_exists() -> bool {
    tokio::process::Command::new("iptables")
        .args(["-C", "FORWARD", "-m", "comment", "--comment", IPTABLES_COMMENT, "-j", "DROP"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn run_cmd(cmd: &str, args: &[&str]) -> Result<(), AppError> {
    let output = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| AppError::Internal(
            format!("Failed to run '{} {}': {}", cmd, args.join(" "), e)
        ))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(
            format!("'{}' failed: {}", cmd, stderr.trim())
        ));
    }

    Ok(())
}
