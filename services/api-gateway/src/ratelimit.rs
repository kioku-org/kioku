use crate::auth::token_hash;
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

const WINDOW_SECONDS: i64 = 60;
const SKIP_PATHS: &[&str] = &["/", "/health", "/docs", "/openapi.json", "/redoc"];

fn now_unix() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

/// Pure arithmetic half of the sliding-window check (Redis I/O happens in `check`).
/// `count` is the cardinality of the window *after* the current request was added.
fn decide(count: i64, limit: i64, window: i64, now: f64, window_start: f64) -> (bool, i64, i64) {
    if limit <= 0 {
        return (true, 0, 0);
    }
    let remaining = (limit - count).max(0);
    if count > limit {
        let retry_after = (window as f64 - (now - window_start)) as i64 + 1;
        (false, 0, retry_after)
    } else {
        (true, remaining, 0)
    }
}

async fn check(state: &AppState, key: &str, limit: i64) -> anyhow::Result<(bool, i64, i64)> {
    if limit <= 0 {
        return Ok((true, 0, 0));
    }
    let now = now_unix();
    let window_start = now - WINDOW_SECONDS as f64;

    let mut redis = state.redis.clone();
    let (_, _, count, _): (i64, i64, i64, bool) = redis::pipe()
        .atomic()
        .zrembyscore(key, 0, window_start)
        .zadd(key, now.to_string(), now)
        .zcard(key)
        .expire(key, WINDOW_SECONDS + 1)
        .query_async(&mut redis)
        .await?;

    Ok(decide(count, limit, WINDOW_SECONDS, now, window_start))
}

pub async fn middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if SKIP_PATHS.contains(&path.as_str()) || state.config.rate_limit_rpm <= 0 {
        return next.run(request).await;
    }

    let headers = request.headers();
    let api_key = headers.get("x-api-key").and_then(|v| v.to_str().ok()).unwrap_or("");
    let admin_key = headers.get("x-admin-api-key").and_then(|v| v.to_str().ok()).unwrap_or("");
    let is_ws = headers
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    let (bucket, limit) = if is_ws {
        let id = if !api_key.is_empty() { token_hash(api_key) } else { addr.ip().to_string() };
        (format!("ratelimit:ws:{id}"), state.config.rate_limit_ws_rpm)
    } else if path.starts_with("/admin") {
        let id = if !admin_key.is_empty() { token_hash(admin_key) } else { addr.ip().to_string() };
        (format!("ratelimit:admin:{id}"), state.config.rate_limit_admin_rpm)
    } else {
        let id = if !api_key.is_empty() { token_hash(api_key) } else { addr.ip().to_string() };
        (format!("ratelimit:api:{id}"), state.config.rate_limit_rpm)
    };

    match check(&state, &bucket, limit).await {
        Ok((true, remaining, _)) => {
            let mut response = next.run(request).await;
            response
                .headers_mut()
                .insert("x-ratelimit-remaining", remaining.to_string().parse().unwrap());
            response
                .headers_mut()
                .insert("x-ratelimit-limit", limit.to_string().parse().unwrap());
            response
        }
        Ok((false, _, retry_after)) => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.to_string())],
            axum::Json(serde_json::json!({"detail": "Rate limit exceeded", "retry_after": retry_after})),
        )
            .into_response(),
        Err(_) => {
            // Redis failure — don't block requests, matches Python's fail-open behavior.
            next.run(request).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_limit_allows_and_reports_remaining() {
        let (allowed, remaining, retry_after) = decide(5, 10, 60, 1000.0, 940.0);
        assert!(allowed);
        assert_eq!(remaining, 5);
        assert_eq!(retry_after, 0);
    }

    #[test]
    fn over_limit_blocks_with_retry_after() {
        // 30s into a 60s window (window_start = now - 30) → 30s left, +1s grace.
        let (allowed, remaining, retry_after) = decide(11, 10, 60, 1000.0, 970.0);
        assert!(!allowed);
        assert_eq!(remaining, 0);
        assert_eq!(retry_after, 31);
    }

    #[test]
    fn zero_limit_disables_check() {
        assert_eq!(decide(999, 0, 60, 1000.0, 940.0), (true, 0, 0));
    }
}
