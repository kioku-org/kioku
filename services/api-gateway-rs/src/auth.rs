use crate::state::AppState;
use redis::AsyncCommands;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Short hash of the full token for cache/rate-limit keys (avoids storing/leaking raw keys).
pub fn token_hash(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    hex::encode(digest)[..16].to_string()
}

/// Validate a token via admin-api, with a 60s Redis cache. Mirrors Python's `_resolve_token`.
pub async fn resolve_token(state: &AppState, api_key: &str) -> Option<Value> {
    let cache_key = format!("gateway:token:{}", token_hash(api_key));

    {
        let mut redis = state.redis.clone();
        if let Ok(Some(cached)) = redis.get::<_, Option<String>>(&cache_key).await {
            if let Ok(v) = serde_json::from_str::<Value>(&cached) {
                return Some(v);
            }
        }
    }

    let mut req = state
        .http
        .post(format!("{}/internal/validate", state.config.admin_api_url))
        .json(&serde_json::json!({ "token": api_key }));
    if !state.config.internal_api_secret.is_empty() {
        req = req.header("X-Internal-Secret", &state.config.internal_api_secret);
    }

    let resp = match req
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Token validation failed: {e}");
            return None;
        }
    };

    if resp.status() != reqwest::StatusCode::OK {
        return None;
    }

    let user_data: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return None,
    };

    {
        let mut redis = state.redis.clone();
        let _: Result<(), _> = redis
            .set_ex(&cache_key, user_data.to_string(), 60)
            .await;
    }

    Some(user_data)
}

pub fn user_scopes(user_data: &Value) -> Vec<String> {
    user_data
        .get("scopes")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// Check whether `path` requires a scope the token doesn't have. `None` = allowed.
pub fn scope_violation(path: &str, scopes: &[String]) -> bool {
    for (prefix, required) in crate::config::ROUTE_SCOPES {
        if path.starts_with(prefix) {
            return !required.iter().any(|r| scopes.iter().any(|s| s == r));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_deterministic_and_short() {
        let a = token_hash("cmp_abc123");
        let b = token_hash("cmp_abc123");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
        assert_ne!(a, token_hash("cmp_other"));
    }

    #[test]
    fn scope_violation_blocks_missing_scope() {
        let scopes = vec!["tx".to_string()];
        assert!(scope_violation("/bots", &scopes)); // needs bot|browser
        assert!(!scope_violation("/transcripts/foo", &scopes)); // has tx
        assert!(!scope_violation("/unmapped/path", &scopes)); // no scope requirement
    }
}
