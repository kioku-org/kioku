//! Guards service-to-service internal routes (bot callbacks) with the shared
//! INTERNAL_API_SECRET. Faithful port of collector/auth.py's `require_internal_secret` —
//! closes CVE-2026-25058 / GHSA-w73r-2449-qwgh (previously-open internal routes).
//!
//! - INTERNAL_API_SECRET unset + DEV_MODE=false → 503 (fail-closed)
//! - INTERNAL_API_SECRET set, X-Internal-Secret absent/mismatched → 403
//! - INTERNAL_API_SECRET unset + DEV_MODE=true → allow (local dev)

use crate::state::AppState;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub async fn require_internal_secret(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if state.config.internal_api_secret.is_empty() {
        if state.config.dev_mode {
            return next.run(request).await;
        }
        return (StatusCode::SERVICE_UNAVAILABLE, "INTERNAL_API_SECRET not configured").into_response();
    }

    let provided = request.headers().get("X-Internal-Secret").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !constant_time_eq(provided, &state.config.internal_api_secret) {
        return (StatusCode::FORBIDDEN, "Invalid internal secret").into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq("abc123", "abc123"));
    }

    #[test]
    fn constant_time_eq_rejects_mismatch() {
        assert!(!constant_time_eq("abc123", "abc124"));
        assert!(!constant_time_eq("short", "longer-string"));
        assert!(!constant_time_eq("", "nonempty"));
    }
}
