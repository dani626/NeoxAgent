use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Unified error type for the agent
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    /// Podman API error
    Podman(String),
    /// Configuration error
    Config(String),
    /// Authentication failed
    Unauthorized,
    /// Resource not found
    NotFound(String),
    /// Bad request / validation error
    BadRequest(String),
    /// Internal server error
    Internal(String),
    /// I/O error (file operations, etc.)
    Io(std::io::Error),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Podman(msg) => write!(f, "Podman error: {}", msg),
            AppError::Config(msg) => write!(f, "Config error: {}", msg),
            AppError::Unauthorized => write!(f, "Unauthorized"),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
            AppError::Io(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Podman(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::Config(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Invalid or missing API key".into()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::Io(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        let body = json!({
            "error": true,
            "message": message,
        });

        (status, axum::Json(body)).into_response()
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}
