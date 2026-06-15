//! CORS origin validation with support for exact origins and wildcard subdomain patterns.
//!
//! Patterns supported in `config.cors_origins`:
//!   - Exact:    "https://panel.neoxhost.com"
//!   - Wildcard: "*.neoxhost.com"  →  matches https://<any>.neoxhost.com

use axum::http::HeaderValue;
use tower_http::cors::AllowOrigin;

/// Returns true if `origin` matches the given pattern.
///
/// - `"*.example.com"` matches `"https://foo.example.com"`, `"https://bar.example.com"`, etc.
/// - Everything else is treated as an exact string match.
fn origin_matches_pattern(origin: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        // suffix is e.g. ".neoxhost.com"
        // origin must end with that suffix and have a scheme prefix
        // e.g. "https://panel.neoxhost.com" ends with ".neoxhost.com" ✓
        origin.ends_with(suffix)
    } else {
        origin == pattern
    }
}

/// Builds an `AllowOrigin` policy from a list of origin patterns.
///
/// - If `patterns` is empty → `AllowOrigin::any()` (permissive, dev only).
/// - Otherwise → `AllowOrigin::predicate(...)` that checks each pattern.
pub fn build_allow_origin(patterns: Vec<String>) -> AllowOrigin {
    if patterns.is_empty() {
        tracing::warn!(
            "⚠️  cors_origins is empty — allowing ALL origins (not safe for production)"
        );
        return AllowOrigin::any();
    }

    tracing::info!("🌐 CORS allowed origins ({}):", patterns.len());
    for p in &patterns {
        tracing::info!("   • {}", p);
    }

    AllowOrigin::predicate(move |origin: &HeaderValue, _parts| {
        let origin_str = match origin.to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };
        patterns
            .iter()
            .any(|pat| origin_matches_pattern(origin_str, pat))
    })
}

#[cfg(test)]
mod tests {
    use super::origin_matches_pattern;

    #[test]
    fn exact_match() {
        assert!(origin_matches_pattern(
            "https://panel.neoxhost.com",
            "https://panel.neoxhost.com"
        ));
        assert!(!origin_matches_pattern(
            "https://evil.com",
            "https://panel.neoxhost.com"
        ));
    }

    #[test]
    fn wildcard_subdomain() {
        assert!(origin_matches_pattern(
            "https://panel.neoxhost.com",
            "*.neoxhost.com"
        ));
        assert!(origin_matches_pattern(
            "https://app.neoxhost.com",
            "*.neoxhost.com"
        ));
        // Should NOT match a different domain
        assert!(!origin_matches_pattern(
            "https://evil.neoxhost.com.evil.com",
            "*.neoxhost.com"
        ));
        assert!(!origin_matches_pattern(
            "https://totallynotneoxhost.com",
            "*.neoxhost.com"
        ));
    }

    #[test]
    fn wildcard_does_not_match_apex() {
        // "*.neoxhost.com" should NOT match "https://neoxhost.com" (no subdomain)
        // because it doesn't end with ".neoxhost.com" — it IS "neoxhost.com"
        // Add the apex explicitly if needed.
        assert!(!origin_matches_pattern(
            "https://neoxhost.com",
            "*.neoxhost.com"
        ));
    }
}
