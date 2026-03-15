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

// ─── Query Parameters ────────────────────────────────────────────────────────

/// Query parameters for the logs WebSocket stream.
#[derive(Debug, Deserialize)]
pub struct LogsStreamQuery {
    /// Include stdout (default: true)
    #[serde(default = "default_true")]
    pub stdout: bool,
    /// Include stderr (default: true)
    #[serde(default = "default_true")]
    pub stderr: bool,
    /// Include timestamps (default: false)
    #[serde(default)]
    pub timestamps: bool,
    /// Number of lines from the end to start streaming from
    pub tail: Option<usize>,
}

/// Query parameters for the console WebSocket.
#[derive(Debug, Deserialize)]
pub struct ConsoleQuery {
    /// Shell to use for generic containers (default: /bin/sh)
    #[serde(default = "default_shell")]
    pub shell: String,
}

fn default_true() -> bool {
    true
}

fn default_shell() -> String {
    "/bin/sh".to_string()
}

// ─── WebSocket: Log Streaming ────────────────────────────────────────────────

/// WS /api/containers/:id/logs/stream
///
/// Streams container logs in real-time via WebSocket.
/// Each message is a JSON object:
/// ```json
/// { "stream": "stdout"|"stderr", "data": "log line content" }
/// ```
pub async fn ws_logs_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LogsStreamQuery>,
) -> impl IntoResponse {
    tracing::info!("📡 WebSocket logs stream requested for container '{}'", id);
    ws.on_upgrade(move |socket| handle_logs_stream(socket, state, id, query))
}

async fn handle_logs_stream(
    mut socket: WebSocket,
    state: Arc<AppState>,
    id: String,
    query: LogsStreamQuery,
) {
    let container = state.podman.containers().get(&id);

    // Build logs options with follow=true for streaming
    let mut opts_builder = podman_api::opts::ContainerLogsOpts::builder()
        .stdout(query.stdout)
        .stderr(query.stderr)
        .follow(true)
        .timestamps(query.timestamps);

    if let Some(tail) = query.tail {
        opts_builder = opts_builder.tail(tail.to_string());
    }

    let opts = opts_builder.build();

    // Send initial connection acknowledgment
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
            // Read from Podman logs stream
            chunk = logs_stream.next() => {
                match chunk {
                    Some(Ok(tty_chunk)) => {
                        let (stream_type, data) = match tty_chunk {
                            TtyChunk::StdOut(data) => ("stdout", data),
                            TtyChunk::StdErr(data) => ("stderr", data),
                            TtyChunk::StdIn(data) => ("stdin", data),
                        };

                        let text = String::from_utf8_lossy(&data);
                        let msg = json!({
                            "type": "log",
                            "stream": stream_type,
                            "data": text.trim_end(),
                        });

                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            tracing::info!("📡 Client disconnected from logs stream for '{}'", id);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let error_msg = json!({
                            "type": "error",
                            "message": format!("Log stream error: {}", e),
                        });
                        let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
                        break;
                    }
                    None => {
                        // Stream ended (container stopped?)
                        let end_msg = json!({
                            "type": "disconnected",
                            "message": "Log stream ended",
                        });
                        let _ = socket.send(Message::Text(end_msg.to_string().into())).await;
                        break;
                    }
                }
            }

            // Check for incoming WebSocket messages (ping/pong, close)
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("📡 Client closed logs stream for '{}'", id);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) => break,
                    _ => {} // Ignore other messages
                }
            }
        }
    }
}

// ─── WebSocket: Interactive Console ──────────────────────────────────────────

/// WS /api/containers/:id/console
///
/// Bidirectional WebSocket for interactive console access.
/// - Server → Client: stdout/stderr output from the container process
/// - Client → Server: commands/input to send to stdin
///
/// For game servers (label neox.server.type is set): uses container.attach()
/// to connect directly to PID 1's stdin (e.g., Minecraft console).
///
/// For generic containers: creates an exec session with /bin/sh.
pub async fn ws_console(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<ConsoleQuery>,
) -> impl IntoResponse {
    tracing::info!("🖥️  WebSocket console requested for container '{}'", id);
    ws.on_upgrade(move |socket| handle_console(socket, state, id, query))
}

async fn handle_console(
    mut socket: WebSocket,
    state: Arc<AppState>,
    id: String,
    query: ConsoleQuery,
) {
    let container = state.podman.containers().get(&id);

    // Detect server type from labels to decide attach vs exec
    let use_attach = match container.inspect().await {
        Ok(inspect) => {
            let is_game_server = inspect
                .config
                .as_ref()
                .and_then(|c| c.labels.as_ref())
                .and_then(|l| l.get("neox.server.type"))
                .is_some();
            is_game_server
        }
        Err(e) => {
            let error_msg = json!({
                "type": "error",
                "message": format!("Failed to inspect container: {}", e),
            });
            let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
            return;
        }
    };

    if use_attach {
        handle_console_attach(&mut socket, &state, &id).await;
    } else {
        handle_console_exec(&mut socket, &state, &id, &query.shell).await;
    }
}

/// Attach-based console for game servers (stdin goes directly to PID 1)
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
            let error_msg = json!({
                "type": "error",
                "message": format!("Failed to attach to container: {}", e),
            });
            let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
            return;
        }
    };

    // Send connection acknowledgment
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
            // Read output from container
            chunk = reader.next() => {
                match chunk {
                    Some(Ok(tty_chunk)) => {
                        let (stream_type, data) = match tty_chunk {
                            TtyChunk::StdOut(data) => ("stdout", data),
                            TtyChunk::StdErr(data) => ("stderr", data),
                            TtyChunk::StdIn(data) => ("stdin", data),
                        };

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
                        let error_msg = json!({
                            "type": "error",
                            "message": format!("Attach read error: {}", e),
                        });
                        let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
                        break;
                    }
                    None => {
                        let end_msg = json!({
                            "type": "disconnected",
                            "message": "Container process ended",
                        });
                        let _ = socket.send(Message::Text(end_msg.to_string().into())).await;
                        break;
                    }
                }
            }

            // Read commands from the WebSocket client
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(cmd))) => {
                        // Send command to container stdin (add newline)
                        let cmd_str = cmd.to_string();
                        let input = if cmd_str.ends_with('\n') {
                            cmd_str.into_bytes()
                        } else {
                            format!("{}\n", cmd_str).into_bytes()
                        };

                        use futures_util::io::AsyncWriteExt;
                        if writer.write_all(&input).await.is_err() {
                            let error_msg = json!({
                                "type": "error",
                                "message": "Failed to write to container stdin",
                            });
                            let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("🖥️  Console disconnected for '{}'", id);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

/// Exec-based console for generic containers (spawns a shell process)
async fn handle_console_exec(
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    id: &str,
    shell: &str,
) {
    let container = state.podman.containers().get(id);

    // Create an exec session with a shell
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
            let error_msg = json!({
                "type": "error",
                "message": format!("Failed to create exec session: {}", e),
            });
            let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
            return;
        }
    };

    // Start the exec session
    let start_opts = podman_api::opts::ExecStartOpts::builder().build();
    let multiplexer = match exec.start(&start_opts).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            let error_msg = json!({
                "type": "error",
                "message": "Exec session started in detached mode (no stream)",
            });
            let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
            return;
        }
        Err(e) => {
            let error_msg = json!({
                "type": "error",
                "message": format!("Failed to start exec session: {}", e),
            });
            let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
            return;
        }
    };

    // Send connection acknowledgment
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
            // Read output from exec
            chunk = reader.next() => {
                match chunk {
                    Some(Ok(tty_chunk)) => {
                        let (stream_type, data) = match tty_chunk {
                            TtyChunk::StdOut(data) => ("stdout", data),
                            TtyChunk::StdErr(data) => ("stderr", data),
                            TtyChunk::StdIn(data) => ("stdin", data),
                        };

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
                        let error_msg = json!({
                            "type": "error",
                            "message": format!("Exec read error: {}", e),
                        });
                        let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
                        break;
                    }
                    None => {
                        let end_msg = json!({
                            "type": "disconnected",
                            "message": "Shell session ended",
                        });
                        let _ = socket.send(Message::Text(end_msg.to_string().into())).await;
                        break;
                    }
                }
            }

            // Read commands from WebSocket client
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(input))) => {
                        use futures_util::io::AsyncWriteExt;
                        let bytes = input.to_string().into_bytes();
                        if writer.write_all(&bytes).await.is_err() {
                            let error_msg = json!({
                                "type": "error",
                                "message": "Failed to write to exec stdin",
                            });
                            let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        // Support raw binary input for terminal emulators
                        use futures_util::io::AsyncWriteExt;
                        if writer.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("🖥️  Exec console disconnected for '{}'", id);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

// ─── WebSocket: Stats Streaming ──────────────────────────────────────────────

/// WS /api/containers/:id/stats
///
/// Streams real-time resource usage statistics for a container.
/// Each message is a JSON object with CPU, memory, network, and disk metrics.
pub async fn ws_stats_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::info!("📊 WebSocket stats stream requested for container '{}'", id);
    ws.on_upgrade(move |socket| handle_stats_stream(socket, state, id))
}

async fn handle_stats_stream(
    mut socket: WebSocket,
    state: Arc<AppState>,
    id: String,
) {
    let container = state.podman.containers().get(&id);

    // Send initial connection acknowledgment
    let ack = json!({
        "type": "connected",
        "container_id": id,
        "message": "Stats streaming started"
    });
    if socket.send(Message::Text(ack.to_string().into())).await.is_err() {
        return;
    }

    // Use stats_stream with ~1 second interval
    let mut stats_stream = container.stats_stream(Some(1));

    loop {
        tokio::select! {
            chunk = stats_stream.next() => {
                match chunk {
                    Some(Ok(stats_response)) => {
                        // The ContainerStats200Response is a serde_json::Value
                        // We extract and format the relevant fields
                        let stats_msg = format_stats_response(&id, &stats_response);

                        if socket.send(Message::Text(stats_msg.to_string().into())).await.is_err() {
                            tracing::info!("📊 Client disconnected from stats stream for '{}'", id);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let error_msg = json!({
                            "type": "error",
                            "message": format!("Stats stream error: {}", e),
                        });
                        let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
                        break;
                    }
                    None => {
                        let end_msg = json!({
                            "type": "disconnected",
                            "message": "Stats stream ended",
                        });
                        let _ = socket.send(Message::Text(end_msg.to_string().into())).await;
                        break;
                    }
                }
            }

            // Handle incoming WebSocket messages
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("📊 Client closed stats stream for '{}'", id);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

/// Formats the raw Podman stats response into our clean API format.
///
/// ContainerStats200Response is a type alias for serde_json::Value,
/// so we extract the stats array and format each entry.
fn format_stats_response(
    container_id: &str,
    stats_response: &serde_json::Value,
) -> serde_json::Value {
    // Podman stats response has a "Stats" array with container stats
    let stats = stats_response
        .get("Stats")
        .and_then(|s| s.as_array())
        .and_then(|arr| arr.first());

    if let Some(stat) = stats {
        // Extract CPU percentage
        let cpu_percent = stat.get("CPU").and_then(|c| c.as_f64()).unwrap_or(0.0);

        // Extract memory usage
        let mem_usage = stat.get("MemUsage").and_then(|m| m.as_u64()).unwrap_or(0);
        let mem_limit = stat.get("MemLimit").and_then(|m| m.as_u64()).unwrap_or(0);
        let mem_usage_mb = mem_usage / 1024 / 1024;
        let mem_limit_mb = mem_limit / 1024 / 1024;
        let mem_percent = if mem_limit > 0 {
            (mem_usage as f64 / mem_limit as f64) * 100.0
        } else {
            0.0
        };

        // Extract network I/O
        let net_input = stat.get("NetInput").and_then(|n| n.as_u64()).unwrap_or(0);
        let net_output = stat.get("NetOutput").and_then(|n| n.as_u64()).unwrap_or(0);

        // Extract block I/O
        let block_input = stat.get("BlockInput").and_then(|b| b.as_u64()).unwrap_or(0);
        let block_output = stat.get("BlockOutput").and_then(|b| b.as_u64()).unwrap_or(0);

        // Extract PIDs
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
        // Fallback: return raw stats if format is unexpected
        json!({
            "type": "stats",
            "container_id": container_id,
            "timestamp": crate::time_utils::now_rfc3339(),
            "raw": stats_response,
        })
    }
}

// ─── WebSocket: Pod Log Streaming ───────────────────────────────────────────

/*
/// WS /api/pods/:id/logs/stream
pub async fn ws_pod_logs_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LogsStreamQuery>,
) -> impl IntoResponse {
    tracing::info!("📡 WebSocket pod logs stream requested for '{}'", id);
    ws.on_upgrade(move |socket| handle_pod_logs_stream(socket, state, id, query))
}

async fn handle_pod_logs_stream(
    mut socket: WebSocket,
    state: Arc<AppState>,
    id: String,
    _query: LogsStreamQuery,
) {
    let pod = state.podman.pods().get(&id);
    let mut logs_stream = pod.logs();

    // Initial ack
    let ack = json!({
        "type": "connected",
        "pod_id": id,
        "message": "Pod log streaming started"
    });
    if socket.send(Message::Text(ack.to_string().into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            chunk = logs_stream.next() => {
                match chunk {
                    Some(Ok(tty_chunk)) => {
                        let (stream_type, data) = match tty_chunk {
                            TtyChunk::StdOut(data) => ("stdout", data),
                            TtyChunk::StdErr(data) => ("stderr", data),
                            TtyChunk::StdIn(data) => ("stdin", data),
                        };

                        let text = String::from_utf8_lossy(&data);
                        let msg = json!({
                            "type": "log",
                            "stream": stream_type,
                            "data": text.trim_end(),
                        });

                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let error_msg = json!({
                            "type": "error",
                            "message": format!("Pod log stream error: {}", e),
                        });
                        let _ = socket.send(Message::Text(error_msg.to_string().into())).await;
                        break;
                    }
                    None => {
                        let end_msg = json!({
                            "type": "disconnected",
                            "message": "Pod log stream ended",
                        });
                        let _ = socket.send(Message::Text(end_msg.to_string().into())).await;
                        break;
                    }
                }
            }

            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("📡 Client closed pod log stream for '{}'", id);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
*/
