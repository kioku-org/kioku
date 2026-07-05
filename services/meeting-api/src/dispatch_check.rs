//! Generic env-gated dispatch-check call-out. Faithful port of dispatch_check.py.
//!
//! When `DISPATCH_CHECK_URL` is set, bot-create/dispatch entry points ask an external
//! authority "should this action proceed?" Unset (the OSS self-host default — and Kioku's
//! actual deployment) means always-allow, no call made. Fails OPEN on any transport error,
//! 5xx, or malformed response: blocking paying customers on a transient gate outage is worse
//! than letting a handful of disallowed actions through; the authority side reconciles later.

use crate::webhook_delivery::build_headers;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct DispatchCheckResult {
    pub allow: bool,
    pub reason: Option<String>,
    pub http_status: u16,
}

pub async fn dispatch_check(http: &reqwest::Client, user_id: i32, action: &str, context: Option<&Value>) -> DispatchCheckResult {
    let Ok(url) = std::env::var("DISPATCH_CHECK_URL") else {
        return DispatchCheckResult { allow: true, reason: None, http_status: 0 };
    };
    let url = url.trim();
    if url.is_empty() {
        return DispatchCheckResult { allow: true, reason: None, http_status: 0 };
    }

    let mut payload = json!({"user_id": user_id, "action": action});
    if let Some(ctx) = context {
        payload["context"] = ctx.clone();
    }
    // sort_keys-equivalent: serde_json's Map is insertion-ordered by default, but json! here
    // has a fixed, small key set — deterministic enough for this purpose (the Python version's
    // determinism goal was reproducible debug-replay, not a security property).
    let body_bytes = payload.to_string().into_bytes();

    let secret = std::env::var("DISPATCH_CHECK_SECRET").unwrap_or_default();
    let secret = secret.trim();
    let headers = build_headers(if secret.is_empty() { None } else { Some(secret) }, &body_bytes);

    let timeout_s: f64 = std::env::var("DISPATCH_CHECK_TIMEOUT_S").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);
    let endpoint = format!("{}/check", url.trim_end_matches('/'));

    let mut req = http.post(&endpoint).body(body_bytes).timeout(std::time::Duration::from_secs_f64(timeout_s));
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status >= 500 {
                tracing::warn!(status, action, user_id, "dispatch_check authority returned 5xx — failing open");
                return DispatchCheckResult { allow: true, reason: None, http_status: status };
            }
            match resp.json::<Value>().await {
                Ok(body) => {
                    let allow = body.get("allow").and_then(Value::as_bool).unwrap_or(true);
                    let reason = body.get("reason").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
                    DispatchCheckResult { allow, reason, http_status: status }
                }
                Err(_) => {
                    tracing::warn!(status, action, user_id, "dispatch_check authority returned non-JSON body — failing open");
                    DispatchCheckResult { allow: true, reason: None, http_status: status }
                }
            }
        }
        Err(e) => {
            tracing::warn!(action, user_id, error = %e, "dispatch_check transport failure — failing open");
            DispatchCheckResult { allow: true, reason: None, http_status: 599 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both cases share process-global env vars (DISPATCH_CHECK_URL) — combined into one test
    // so they can't interleave with cargo test's parallel-by-default test threads.
    #[tokio::test]
    async fn env_gated_behavior() {
        std::env::remove_var("DISPATCH_CHECK_URL");
        let http = reqwest::Client::new();

        let result = dispatch_check(&http, 1, "create-bot", None).await;
        assert!(result.allow);
        assert_eq!(result.http_status, 0);

        // A port nothing listens on — connection refused, exercising the transport-failure path.
        std::env::set_var("DISPATCH_CHECK_URL", "http://127.0.0.1:1");
        let result = dispatch_check(&http, 1, "create-bot", None).await;
        assert!(result.allow, "fail-open: unreachable authority must still allow");
        assert_eq!(result.http_status, 599);
        std::env::remove_var("DISPATCH_CHECK_URL");
    }
}
