//! Dual-mode auth: trusted gateway headers (normal deployment, set by api-gateway after it
//! validates the client's token) or standalone API keys. Faithful port of auth.py.

use crate::state::AppState;
use axum::http::HeaderMap;

#[derive(Debug, Clone)]
pub struct AuthedUser {
    pub user_id: i32,
    pub scopes: Vec<String>,
    pub max_concurrent: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid or missing API key")]
    Forbidden,
    #[error("Authentication required")]
    Unauthorized,
}

pub fn validate_request(state: &AppState, headers: &HeaderMap) -> Result<AuthedUser, AuthError> {
    if let Some(uid) = headers.get("X-User-ID").and_then(|v| v.to_str().ok()) {
        if let Ok(user_id) = uid.parse::<i32>() {
            let scopes = headers
                .get("X-User-Scopes")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .split(',')
                .map(str::to_string)
                .collect();
            let max_concurrent = headers
                .get("X-User-Limits")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(1);
            return Ok(AuthedUser { user_id, scopes, max_concurrent });
        }
    }

    let api_key = headers.get("X-API-Key").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !state.config.api_keys.is_empty() {
        if api_key.is_empty() || !state.config.api_keys.iter().any(|k| k == api_key) {
            return Err(AuthError::Forbidden);
        }
        return Ok(AuthedUser { user_id: 0, scopes: vec!["*".to_string()], max_concurrent: 999 });
    }

    // No API keys configured at all: dev mode, everything is allowed.
    Ok(AuthedUser { user_id: 0, scopes: vec!["*".to_string()], max_concurrent: 999 })
}
