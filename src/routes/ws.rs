use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    response::IntoResponse,
};
use futures_util::StreamExt;
use podman_api::conn::TtyChunk;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

// ─── Query Parameters ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LogsStreamQuery {
    #[serde(default = "default_true")]
    pub stdout: bool,
    #[serde(default = "default_true")]
    pub stderr: bool,
    #[serde(default)]
    pub timestamps: bool,
    pub tail: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ConsoleQuery {
    #[serde(default = "default_shell")]
    pub shell: String,
}

fn default_true() -> bool {
    true
}

fn default_shell() -> String {
    "/bin/sh".to_string()
}

// ─── Shared Helpers ───────────────────────────────────────────────────────────────

/// Converts a TtyChunk into a (&'static str, Vec<u8>) pair for stream type + data.
fn tty_chunk_to_parts(chunk: TtyChunk) -> (&'static str, Vec<u8>) {
    match chunk {
        TtyChunk::StdOut(data) => ("stdout", data),
        TtyChunk::StdErr(data) => ("stderr", data),
        TtyChunk::StdIn(data) => ("stdin", data),
    }
}

/// Handles incoming WebSocket control frames (Ping/Pong/Close).
/// Returns true if the loop should break (connection closed or error).
async fn handle_ws_control(socket: &mut WebSocket, msg: Option<Result<Message, axum::Error>>) -> bool {
    match msg {
        Some(Ok(Message::Close(_))) | None => true,
        Some(Ok(Message::Ping(data))) => socket.send(Message::Pong(data)).await.is_err(),
        Some(Err(_)) => true,
        _ => false,
    }
}

// ─── WebSocket: Log Streaming ──────────────────────────────────────────────────────────

/// WS /api/containers/:id/logs/stream
pub async fn ws_logs_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LogsStreamQuery>,
) -> impl IntoResponse {
    tracing::info!("\u{1f4e1} WebSocket logs stream requested for container '{}'", id);
    ws.on_upgrade(move |socket| handle_logs_stream(socket, state, id, query))
}

async fn handle_logs_stream(
    mut socket: WebSocket,
    state: Arc<AppState>,
    id: String,
    query: LogsStreamQuery,
) {
    let container = state.podman.containers().get(&id);

    let mut opts_builder = podman_api::opts::ContainerLogsOpts::builder()
        .stdout(query.stdout)
        .stderr(query.stderr)
        .follow(true)
        .timestamps(query.timestamps);

    if let Some(tail) = query.tail {
        opts_builder = opts_builder.tail(tail.to_string());
    }

    let opts = opts_builder.build();

    let ack = json!({
        "type": "connected",
        "container_id": id,
        "message": "Log streaming started"
    });
    if socket.send(Message::Text(ack.to_string().into())).await.is_err() {
        return;
    }

    let mut logs_stream = container.logs(&opts);

    loop {
        tokio::select! {
            chunk = logs_stream.next() => {
                match chunk {
                    Some(Ok(tty_chunk)) => {
                        let (stream_type, data) = tty_chunk_to_parts(tty_chunk);
                        let text = String::from_utf8_lossy(&data);
                        let msg = json!({
                            "type": "log",
                            "stream": stream_type,
                            "data": text.trim_end(),
                        });
                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            tracing::info!("\u{1f4e1} Client disconnected from logs stream for '{}'", id);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "error",
                            "message": format!("Log stream error: {}", e),
                        }).to_string().into())).await;
                        break;
                    }
                    None => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "disconnected",
                            "message": "Log stream ended",
                        }).to_string().into())).await;
                        break;
                    }
                }
            }
            ws_msg = socket.recv() => {
                if handle_ws_control(&mut socket, ws_msg).await {
                    tracing::info!("\u{1f4e1} Client closed logs stream for '{}'", id);
                    break;
                }
            }
        }
    }
}

// ─── WebSocket: Interactive Console ──────────────────────────────────────────────────

/// WS /api/containers/:id/console
pub async fn ws_console(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<ConsoleQuery>,
) -> impl IntoResponse {
    tracing::info!("\u{1f5a5}\u{fe0f}  WebSocket console requested for container '{}'", id);
    ws.on_upgrade(move |socket| handle_console(socket, state, id, query))
}

async fn handle_console(
    mut socket: WebSocket,
    state: Arc<AppState>,
    id: String,
    query: ConsoleQuery,
) {
    let container = state.podman.containers().get(&id);

    let use_attach = match container.inspect().await {
        Ok(inspect) => inspect
            .config
            .as_ref()
            .and_then(|c| c.labels.as_ref())
            .and_then(|l| l.get("neox.server.type"))
            .is_some(),
        Err(e) => {
            let _ = socket.send(Message::Text(json!({
                "type": "error",
                "message": format!("Failed to inspect container: {}", e),
            }).to_string().into())).await;
            return;
        }
    };

    if use_attach {
        handle_console_attach(&mut socket, &state, &id).await;
    } else {
        handle_console_exec(&mut socket, &state, &id, &query.shell).await;
    }
}

async fn handle_console_attach(
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    id: &str,
) {
    let container = state.podman.containers().get(id);

    let attach_opts = podman_api::opts::ContainerAttachOpts::builder()
        .stdin(true)
        .stdout(true)
        .stderr(true)
        .build();

    let multiplexer = match container.attach(&attach_opts).await {
        Ok(m) => m,
        Err(e) => {
            let _ = socket.send(Message::Text(json!({
                "type": "error",
                "message": format!("Failed to attach to container: {}", e),
            }).to_string().into())).await;
            return;
        }
    };

    let ack = json!({
        "type": "connected",
        "mode": "attach",
        "container_id": id,
        "message": "Attached to container process (stdin/stdout)"
    });
    if socket.send(Message::Text(ack.to_string().into())).await.is_err() {
        return;
    }

    let (mut reader, mut writer) = multiplexer.split();

    loop {
        tokio::select! {
            chunk = reader.next() => {
                match chunk {
                    Some(Ok(tty_chunk)) => {
                        let (stream_type, data) = tty_chunk_to_parts(tty_chunk);
                        let text = String::from_utf8_lossy(&data);
                        let msg = json!({
                            "type": "output",
                            "stream": stream_type,
                            "data": text.as_ref(),
                        });
                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "error",
                            "message": format!("Attach read error: {}", e),
                        }).to_string().into())).await;
                        break;
                    }
                    None => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "disconnected",
                            "message": "Container process ended",
                        }).to_string().into())).await;
                        break;
                    }
                }
            }
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(cmd))) => {
                        let cmd_str = cmd.to_string();
                        let input = if cmd_str.ends_with('\n') {
                            cmd_str.into_bytes()
                        } else {
                            format!("{}\n", cmd_str).into_bytes()
                        };
                        use futures_util::io::AsyncWriteExt;
                        if writer.write_all(&input).await.is_err() {
                            let _ = socket.send(Message::Text(json!({
                                "type": "error",
                                "message": "Failed to write to container stdin",
                            }).to_string().into())).await;
                            break;
                        }
                    }
                    other => {
                        if handle_ws_control(socket, other).await {
                            tracing::info!("\u{1f5a5}\u{fe0f}  Console disconnected for '{}'", id);
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn handle_console_exec(
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    id: &str,
    shell: &str,
) {
    let container = state.podman.containers().get(id);

    let exec_opts = podman_api::opts::ExecCreateOpts::builder()
        .command([shell])
        .attach_stdin(true)
        .attach_stdout(true)
        .attach_stderr(true)
        .tty(true)
        .build();

    let exec = match container.create_exec(&exec_opts).await {
        Ok(e) => e,
        Err(e) => {
            let _ = socket.send(Message::Text(json!({
                "type": "error",
                "message": format!("Failed to create exec session: {}", e),
            }).to_string().into())).await;
            return;
        }
    };

    let start_opts = podman_api::opts::ExecStartOpts::builder().build();
    let multiplexer = match exec.start(&start_opts).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            let _ = socket.send(Message::Text(json!({
                "type": "error",
                "message": "Exec session started in detached mode (no stream)",
            }).to_string().into())).await;
            return;
        }
        Err(e) => {
            let _ = socket.send(Message::Text(json!({
                "type": "error",
                "message": format!("Failed to start exec session: {}", e),
            }).to_string().into())).await;
            return;
        }
    };

    let ack = json!({
        "type": "connected",
        "mode": "exec",
        "container_id": id,
        "shell": shell,
        "message": format!("Shell session started with {}", shell)
    });
    if socket.send(Message::Text(ack.to_string().into())).await.is_err() {
        return;
    }

    let (mut reader, mut writer) = multiplexer.split();

    loop {
        tokio::select! {
            chunk = reader.next() => {
                match chunk {
                    Some(Ok(tty_chunk)) => {
                        let (stream_type, data) = tty_chunk_to_parts(tty_chunk);
                        let text = String::from_utf8_lossy(&data);
                        let msg = json!({
                            "type": "output",
                            "stream": stream_type,
                            "data": text.as_ref(),
                        });
                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "error",
                            "message": format!("Exec read error: {}", e),
                        }).to_string().into())).await;
                        break;
                    }
                    None => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "disconnected",
                            "message": "Shell session ended",
                        }).to_string().into())).await;
                        break;
                    }
                }
            }
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(input))) => {
                        use futures_util::io::AsyncWriteExt;
                        if writer.write_all(&input.to_string().into_bytes()).await.is_err() {
                            let _ = socket.send(Message::Text(json!({
                                "type": "error",
                                "message": "Failed to write to exec stdin",
                            }).to_string().into())).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        use futures_util::io::AsyncWriteExt;
                        if writer.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                    other => {
                        if handle_ws_control(socket, other).await {
                            tracing::info!("\u{1f5a5}\u{fe0f}  Exec console disconnected for '{}'", id);
                            break;
                        }
                    }
                }
            }
        }
    }
}

// ─── WebSocket: Stats Streaming ──────────────────────────────────────────────────────────

/// WS /api/containers/:id/stats
pub async fn ws_stats_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::info!("\u{1f4ca} WebSocket stats stream requested for container '{}'", id);
    ws.on_upgrade(move |socket| handle_stats_stream(socket, state, id))
}

async fn handle_stats_stream(
    mut socket: WebSocket,
    state: Arc<AppState>,
    id: String,
) {
    let container = state.podman.containers().get(&id);

    let ack = json!({
        "type": "connected",
        "container_id": id,
        "message": "Stats streaming started"
    });
    if socket.send(Message::Text(ack.to_string().into())).await.is_err() {
        return;
    }

    let mut stats_stream = container.stats_stream(Some(1));

    loop {
        tokio::select! {
            chunk = stats_stream.next() => {
                match chunk {
                    Some(Ok(stats_response)) => {
                        let stats_msg = format_stats_response(&id, &stats_response);
                        if socket.send(Message::Text(stats_msg.to_string().into())).await.is_err() {
                            tracing::info!("\u{1f4ca} Client disconnected from stats stream for '{}'", id);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "error",
                            "message": format!("Stats stream error: {}", e),
                        }).to_string().into())).await;
                        break;
                    }
                    None => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "disconnected",
                            "message": "Stats stream ended",
                        }).to_string().into())).await;
                        break;
                    }
                }
            }
            ws_msg = socket.recv() => {
                if handle_ws_control(&mut socket, ws_msg).await {
                    tracing::info!("\u{1f4ca} Client closed stats stream for '{}'", id);
                    break;
                }
            }
        }
    }
}

fn format_stats_response(
    container_id: &str,
    stats_response: &serde_json::Value,
) -> serde_json::Value {
    let stats = stats_response
        .get("Stats")
        .and_then(|s| s.as_array())
        .and_then(|arr| arr.first());

    if let Some(stat) = stats {
        let cpu_percent = stat.get("CPU").and_then(|c| c.as_f64()).unwrap_or(0.0);
        let mem_usage = stat.get("MemUsage").and_then(|m| m.as_u64()).unwrap_or(0);
        let mem_limit = stat.get("MemLimit").and_then(|m| m.as_u64()).unwrap_or(0);
        let mem_usage_mb = mem_usage / 1024 / 1024;
        let mem_limit_mb = mem_limit / 1024 / 1024;
        let mem_percent = if mem_limit > 0 {
            (mem_usage as f64 / mem_limit as f64) * 100.0
        } else {
            0.0
        };
        let net_input = stat.get("NetInput").and_then(|n| n.as_u64()).unwrap_or(0);
        let net_output = stat.get("NetOutput").and_then(|n| n.as_u64()).unwrap_or(0);
        let block_input = stat.get("BlockInput").and_then(|b| b.as_u64()).unwrap_or(0);
        let block_output = stat.get("BlockOutput").and_then(|b| b.as_u64()).unwrap_or(0);
        let pids = stat.get("PIDs").and_then(|p| p.as_u64()).unwrap_or(0);

        json!({
            "type": "stats",
            "container_id": container_id,
            "timestamp": crate::time_utils::now_rfc3339(),
            "cpu_percent": (cpu_percent * 100.0).round() / 100.0,
            "memory_used_mb": mem_usage_mb,
            "memory_limit_mb": mem_limit_mb,
            "memory_percent": (mem_percent * 100.0).round() / 100.0,
            "network": {
                "rx_bytes": net_input,
                "tx_bytes": net_output,
            },
            "disk": {
                "read_bytes": block_input,
                "write_bytes": block_output,
            },
            "pids": pids,
        })
    } else {
        json!({
            "type": "stats",
            "container_id": container_id,
            "timestamp": crate::time_utils::now_rfc3339(),
            "raw": stats_response,
        })
    }
}
