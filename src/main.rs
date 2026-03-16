//! # neoxagent
//!
//! Lightweight Podman management agent for the Jexactyl panel.
//! Runs on each node and exposes a REST API to manage containers and pods.
//!
//! ## Architecture
//! ```
//! Panel (Next.js) → HTTPS/API Key → neoxagent (this) → podman.sock → Podman Engine
//! ```

mod auth;
mod config;
mod error;
mod models;
mod routes;
mod services;
mod time_utils;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, post},
    Extension, Router,
};
use podman_api::Podman;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::ApiKey;
use crate::config::Config;

/// Shared application state, available to all route handlers.
pub struct AppState {
    pub podman: Podman,
    pub config: Config,
}

#[tokio::main]
async fn main() {
    // Initialize structured logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "neoxagent=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("🦀 neoxagent v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("   Lightweight Podman Management Agent");
    tracing::info!("   ──────────────────────────────────");

    // Load configuration
    let config = Config::load_default().unwrap_or_else(|e| {
        tracing::error!("Failed to load config.toml: {}", e);
        tracing::info!("Make sure config.toml exists in the current directory.");
        std::process::exit(1);
    });

    tracing::info!("📋 Config loaded");
    tracing::info!("   Socket: {}", config.podman.socket);
    tracing::info!("   Listen: {}:{}", config.agent.host, config.agent.port);

    // Connect to Podman
    let socket_uri = &config.podman.socket;
    let podman = if socket_uri.contains("://") {
        Podman::new(socket_uri).unwrap_or_else(|e| {
            tracing::error!("Failed to create Podman client with URI {}: {}", socket_uri, e);
            std::process::exit(1);
        })
    } else {
        Podman::new(format!("unix://{}", socket_uri)).unwrap_or_else(|e| {
            tracing::error!("Failed to create Podman client with socket {}: {}", socket_uri, e);
            std::process::exit(1);
        })
    };

    // Verify Podman connection
    match podman.info().await {
        Ok(info) => {
            let version = info
                .version
                .as_ref()
                .and_then(|v| v.version.as_deref())
                .unwrap_or("unknown");
            let containers = info
                .store
                .as_ref()
                .and_then(|s| s.container_store.as_ref())
                .and_then(|cs| cs.number)
                .unwrap_or(0);
            let rootless = info
                .host
                .as_ref()
                .and_then(|h| h.security.as_ref())
                .and_then(|s| s.rootless)
                .unwrap_or(false);

            tracing::info!("🐙 Podman connected!");
            tracing::info!("   Version: {}", version);
            tracing::info!("   Containers: {}", containers);
            tracing::info!("   Rootless: {}", rootless);
        }
        Err(e) => {
            tracing::error!("❌ Failed to connect to Podman: {}", e);
            tracing::error!("   Socket path: {}", config.podman.socket);
            tracing::error!("   Make sure Podman is running and the socket is accessible.");
            tracing::error!("   For rootless: systemctl --user start podman.socket");
            std::process::exit(1);
        }
    }

    // Create shared state
    let api_key = ApiKey(config.agent.api_key.clone());
    let bind_addr = format!("{}:{}", config.agent.host, config.agent.port);

    let state = Arc::new(AppState { podman, config });

    // Build router with all routes
    let app = Router::new()
        // ─── System routes ──────────────────────────────────────────
        .route("/api/health", get(routes::system::health))
        .route("/api/system/info", get(routes::system::system_info))
        .route("/api/system/resources", get(routes::system::system_resources))
        // ─── Container CRUD routes ──────────────────────────────────
        .route(
            "/api/containers",
            get(routes::containers::list_containers)
                .post(routes::containers::create_container),
        )
        .route(
            "/api/containers/{id}",
            get(routes::containers::get_container)
                .delete(routes::containers::delete_container),
        )
        .route(
            "/api/containers/{id}/logs",
            get(routes::containers::get_container_logs),
        )
        // ─── Container lifecycle routes ─────────────────────────────
        .route(
            "/api/containers/{id}/rename",
            post(routes::containers::rename_container),
        )
        .route(
            "/api/containers/{id}/start",
            post(routes::containers::start_container),
        )
        .route(
            "/api/containers/{id}/stop",
            post(routes::containers::stop_container),
        )
        .route(
            "/api/containers/{id}/restart",
            post(routes::containers::restart_container),
        )
        .route(
            "/api/containers/{id}/kill",
            post(routes::containers::kill_container),
        )
        // ─── Phase 2: Real-time WebSocket routes ────────────────────
        .route(
            "/api/containers/{id}/logs/stream",
            get(routes::ws::ws_logs_stream),
        )
        .route(
            "/api/containers/{id}/console",
            get(routes::ws::ws_console),
        )
        .route(
            "/api/containers/{id}/stats",
            get(routes::ws::ws_stats_stream),
        )
        // ─── Phase 3: Pod CRUD routes ───────────────────────────────
        .route(
            "/api/pods",
            get(routes::pods::list_pods)
                .post(routes::pods::create_pod),
        )
        .route(
            "/api/pods/{id}",
            get(routes::pods::get_pod)
                .delete(routes::pods::delete_pod),
        )
        .route(
            "/api/pods/{id}/logs",
            get(routes::pods::get_pod_logs),
        )
        // ─── Phase 3: Pod container management ─────────────────────
        .route(
            "/api/pods/{id}/containers",
            post(routes::pods::add_container_to_pod),
        )
        // ─── Volumes ────────────────────────────────────────────────
        .nest("/api/volumes", routes::volumes::router())
        // ─── Phase 3: Pod lifecycle routes ──────────────────────────
        .route(
            "/api/pods/{id}/rename",
            post(routes::pods::rename_pod),
        )
        .route(
            "/api/pods/{id}/proxy",
            post(routes::pods::update_proxy),
        )
        .route(
            "/api/pods/{id}/start",
            post(routes::pods::start_pod),
        )
        .route(
            "/api/pods/{id}/stop",
            post(routes::pods::stop_pod),
        )
        .route(
            "/api/pods/{id}/restart",
            post(routes::pods::restart_pod),
        )
        .route(
            "/api/pods/{id}/logs/stream",
            get(routes::ws::ws_pod_logs_stream),
        )
        // ─── Phase 3: Pod container management ─────────────────────
        .route(
            "/api/pods/{id}/containers",
            get(routes::pods::list_pod_containers)
                .post(routes::pods::add_container_to_pod),
        )
        // ─── Phase 3: Kube YAML generation ─────────────────────────
        .route(
            "/api/pods/{id}/kube",
            get(routes::pods::generate_kube_yaml),
        )
        // ─── Phase 3: Network CRUD routes ───────────────────────────
        .route(
            "/api/networks",
            get(routes::networks::list_networks)
                .post(routes::networks::create_network),
        )
        .route(
            "/api/networks/{id}",
            get(routes::networks::get_network)
                .delete(routes::networks::delete_network),
        )
        // ─── Phase 4: Kubernetes YAML Support ───────────────────────
        .route(
            "/api/kube/deploy",
            post(routes::kube::deploy_kube),
        )
        .route(
            "/api/kube/stacks",
            get(routes::kube::list_stacks),
        )
        .route(
            "/api/kube/stacks/{name}/up",
            post(routes::kube::stack_up),
        )
        .route(
            "/api/kube/stacks/{name}/down",
            post(routes::kube::stack_down),
        )
        .route(
            "/api/kube/stacks/{name}",
            delete(routes::kube::delete_stack),
        )
        .route(
            "/api/kube/stacks/{name}/status",
            get(routes::kube::stack_status),
        )
        .route(
            "/api/kube/generate/{pod_id}",
            post(routes::kube::generate_kube_from_pod),
        )
        // ─── Phase 5: File Manager ──────────────────────────────────
        .route(
            "/api/pods/{id}/files",
            get(routes::files::list_files)
                .delete(routes::files::delete_file),
        )
        .route(
            "/api/pods/{id}/files/content",
            get(routes::files::read_file)
                .put(routes::files::write_file),
        )
        .route(
            "/api/pods/{id}/files/create-dir",
            post(routes::files::create_directory),
        )
        .route(
            "/api/pods/{id}/files/rename",
            post(routes::files::rename_file),
        )
        .route(
            "/api/pods/{id}/files/upload",
            post(routes::files::upload_file),
        )
        .route(
            "/api/pods/{id}/files/download",
            get(routes::files::download_file),
        )
        // ─── Phase 6: Backups ─────────────────────────────────────
        .route(
            "/api/pods/{id}/backups",
            get(routes::backups::list_backups)
                .post(routes::backups::create_backup),
        )
        .route(
            "/api/pods/{id}/backups/{backup_id}",
            get(routes::backups::get_backup_info)
                .delete(routes::backups::delete_backup),
        )
        .route(
            "/api/pods/{id}/backups/{backup_id}/download",
            get(routes::backups::download_backup),
        )
        .route(
            "/api/pods/{id}/backups/{backup_id}/restore",
            post(routes::backups::restore_backup),
        )
        // ─── Phase 7: Images ─────────────────────────────────────
        .route(
            "/api/images",
            get(routes::images::list_images),
        )
        .route(
            "/api/images/pull",
            post(routes::images::pull_image),
        )
        .route(
            "/api/images/search",
            get(routes::images::search_images),
        )
        .route(
            "/api/images/pull/stream",
            get(routes::images::pull_image_stream),
        )
        .route(
            "/api/images/{id}",
            delete(routes::images::delete_image),
        )
        .route(
            "/api/images/{id}/inspect",
            get(routes::images::inspect_image),
        )
        // ─── Phase 7: Systemd ────────────────────────────────────
        .route(
            "/api/pods/{id}/systemd/generate",
            post(routes::systemd::generate_systemd),
        )
        .route(
            "/api/pods/{id}/systemd/enable",
            post(routes::systemd::enable_systemd),
        )
        .route(
            "/api/pods/{id}/systemd/disable",
            post(routes::systemd::disable_systemd),
        )
        .route(
            "/api/pods/{id}/systemd/status",
            get(routes::systemd::systemd_status),
        )
        // ─── Middleware ─────────────────────────────────────────────
        .layer(middleware::from_fn(auth::auth_middleware))
        .layer(Extension(api_key))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        // ─── Shared state ───────────────────────────────────────────
        .with_state(state.clone());

    tracing::info!("──────────────────────────────────────────");
    tracing::info!("🚀 neoxagent listening on {}", bind_addr);
    tracing::info!("   Phase 1: API REST Base       — Active");
    tracing::info!("   Phase 2: Real-time (WS)      — Active");
    tracing::info!("   Phase 3: Pods + hev-socks5-tproxy — Active");
    tracing::info!("   Phase 4: Kube YAML Support   — Active");
    tracing::info!("   Phase 5: File Manager        — Active");
    tracing::info!("   Phase 6: Backups             — Active");
    tracing::info!("   Phase 7: Images + Systemd    — Active");
    tracing::info!("──────────────────────────────────────────");
    tracing::info!("   System:");
    tracing::info!("     GET  /api/health");
    tracing::info!("     GET  /api/system/info");
    tracing::info!("     GET  /api/system/resources");
    tracing::info!("   Containers:");
    tracing::info!("     GET    /api/containers");
    tracing::info!("     POST   /api/containers");
    tracing::info!("     GET    /api/containers/{{id}}");
    tracing::info!("     DELETE /api/containers/{{id}}");
    tracing::info!("     POST   /api/containers/{{id}}/start");
    tracing::info!("     POST   /api/containers/{{id}}/stop");
    tracing::info!("     POST   /api/containers/{{id}}/restart");
    tracing::info!("     POST   /api/containers/{{id}}/kill");
    tracing::info!("   Real-time (WebSocket):");
    tracing::info!("     WS   /api/containers/{{id}}/logs/stream");
    tracing::info!("     WS   /api/containers/{{id}}/console");
    tracing::info!("     WS   /api/containers/{{id}}/stats");
    tracing::info!("   Pods:");
    tracing::info!("     GET    /api/pods");
    tracing::info!("     POST   /api/pods");
    tracing::info!("     GET    /api/pods/{{id}}");
    tracing::info!("     DELETE /api/pods/{{id}}");
    tracing::info!("     POST   /api/pods/{{id}}/start");
    tracing::info!("     POST   /api/pods/{{id}}/stop");
    tracing::info!("     POST   /api/pods/{{id}}/restart");
    tracing::info!("     GET    /api/pods/{{id}}/containers");
    tracing::info!("     POST   /api/pods/{{id}}/containers");
    tracing::info!("     GET    /api/pods/{{id}}/kube");
    tracing::info!("   Networks:");
    tracing::info!("     GET    /api/networks");
    tracing::info!("     POST   /api/networks");
    tracing::info!("     GET    /api/networks/{{id}}");
    tracing::info!("     DELETE /api/networks/{{id}}");
    tracing::info!("   Kube YAML:");
    tracing::info!("     POST   /api/kube/deploy");
    tracing::info!("     GET    /api/kube/stacks");
    tracing::info!("     POST   /api/kube/stacks/{{name}}/up");
    tracing::info!("     POST   /api/kube/stacks/{{name}}/down");
    tracing::info!("     DELETE /api/kube/stacks/{{name}}");
    tracing::info!("     GET    /api/kube/stacks/{{name}}/status");
    tracing::info!("     POST   /api/kube/generate/{{pod_id}}");
    tracing::info!("   File Manager:");
    tracing::info!("     GET    /api/pods/{{id}}/files?path=/");
    tracing::info!("     GET    /api/pods/{{id}}/files/content?path=/");
    tracing::info!("     PUT    /api/pods/{{id}}/files/content?path=/");
    tracing::info!("     POST   /api/pods/{{id}}/files/create-dir?path=/");
    tracing::info!("     POST   /api/pods/{{id}}/files/rename");
    tracing::info!("     DELETE /api/pods/{{id}}/files?path=/");
    tracing::info!("     POST   /api/pods/{{id}}/files/upload?path=/");
    tracing::info!("     GET    /api/pods/{{id}}/files/download?path=/");
    tracing::info!("   Backups:");
    tracing::info!("     GET    /api/pods/{{id}}/backups");
    tracing::info!("     POST   /api/pods/{{id}}/backups");
    tracing::info!("     GET    /api/pods/{{id}}/backups/{{backup_id}}");
    tracing::info!("     GET    /api/pods/{{id}}/backups/{{backup_id}}/download");
    tracing::info!("     POST   /api/pods/{{id}}/backups/{{backup_id}}/restore");
    tracing::info!("     DELETE /api/pods/{{id}}/backups/{{backup_id}}");
    tracing::info!("   Images:");
    tracing::info!("     GET    /api/images");
    tracing::info!("     POST   /api/images/pull");
    tracing::info!("     DELETE /api/images/{{id}}");
    tracing::info!("     GET    /api/images/search?q=");
    tracing::info!("     WS     /api/images/pull/stream");
    tracing::info!("   Systemd:");
    tracing::info!("     POST   /api/pods/{{id}}/systemd/generate");
    tracing::info!("     POST   /api/pods/{{id}}/systemd/enable");
    tracing::info!("     POST   /api/pods/{{id}}/systemd/disable");
    tracing::info!("     GET    /api/pods/{{id}}/systemd/status");
    tracing::info!("──────────────────────────────────────────");

    // Start server (with or without TLS)
    if state.config.tls.enabled {
        let cert_path = &state.config.tls.cert_path;
        let key_path = &state.config.tls.key_path;

        if cert_path.is_empty() || key_path.is_empty() {
            tracing::error!("❌ TLS enabled but cert_path or key_path is empty in config.toml");
            std::process::exit(1);
        }

        tracing::info!("🔒 TLS enabled (native rustls)");
        tracing::info!("   Cert: {}", cert_path);
        tracing::info!("   Key:  {}", key_path);

        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            cert_path,
            key_path,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::error!("❌ Failed to load TLS certificates: {}", e);
            tracing::error!("   Make sure the cert and key files exist and are valid PEM format.");
            std::process::exit(1);
        });

        let addr: std::net::SocketAddr = bind_addr.parse().unwrap_or_else(|e| {
            tracing::error!("Failed to parse bind address '{}': {}", bind_addr, e);
            std::process::exit(1);
        });

        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await
            .unwrap_or_else(|e| {
                tracing::error!("TLS Server error: {}", e);
                std::process::exit(1);
            });
    } else {
        tracing::info!("🔓 TLS disabled (plain HTTP)");

        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to bind to {}: {}", bind_addr, e);
                std::process::exit(1);
            });

        axum::serve(listener, app)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Server error: {}", e);
                std::process::exit(1);
            });
    }
}
