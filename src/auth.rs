use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Middleware that validates the API key from the Authorization header.
///
/// The expected API key is injected as a request extension (`String`) by the
/// extension layer configured in `main.rs`.
///
/// Excludes /api/health from authentication (for health checks).
pub async fn auth_middleware(
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let path = request.uri().path().to_string();

    // Skip auth for health check endpoint
    if path == "/api/health" {
        return Ok(next.run(request).await);
    }

    // Get the expected API key from the extension injected by the layer
    let expected_key = request
        .extensions()
        .get::<ApiKey>()
        .map(|k| k.0.as_str())
        .unwrap_or("");

    // Extract token from Authorization header
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let mut token = if let Some(bearer) = auth_header.strip_prefix("Bearer ") {
        bearer.trim().to_string()
    } else {
        String::new()
    };

    // If still empty, check query params (useful for WebSockets)
    if token.is_empty() {
        if let Some(query) = request.uri().query() {
            for pair in query.split('&') {
                let mut parts = pair.splitn(2, '=');
                if let (Some("token"), Some(val)) = (parts.next(), parts.next()) {
                    token = val.to_string();
                    break;
                }
            }
        }
    }

    if token.is_empty() || token != expected_key {
        tracing::warn!(
            "🔒 Unauthorized request to {} from {:?}",
            path,
            request.headers().get("x-forwarded-for")
        );

        let body = json!({
            "error": true,
            "message": "Invalid or missing API key. Use Authorization: Bearer <API_KEY>",
        });

        return Err((StatusCode::UNAUTHORIZED, axum::Json(body)).into_response());
    }

    Ok(next.run(request).await)
}

/// Newtype wrapper for the API key, to be used as a request extension.
#[derive(Clone)]
pub struct ApiKey(pub String);
