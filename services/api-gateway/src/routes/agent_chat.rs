use crate::auth::{resolve_token, user_scopes};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use futures_util::TryStreamExt;
use redis::AsyncCommands;
use serde_json::{json, Value};
use std::collections::HashSet;

pub fn router(state: &AppState) -> Router<AppState> {
    if state.config.agent_api_url.is_none() {
        return Router::new();
    }
    Router::new().route("/api/chat", post(agent_chat_proxy))
}

/// Fetches transcript context for the user's currently-active meeting(s), if any, for
/// injection into meeting-aware agent sessions. Mirrors Python's `_get_meeting_context`.
async fn get_meeting_context(state: &AppState, user_id: &str) -> Option<String> {
    let mut headers = HeaderMap::new();
    headers.insert("x-user-id", HeaderValue::from_str(user_id).ok()?);

    let bots_resp = state
        .http
        .get(format!("{}/bots/status", state.config.meeting_api_url))
        .headers(headers.clone())
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    if bots_resp.status() != StatusCode::OK {
        return None;
    }
    let bots_data: Value = bots_resp.json().await.ok()?;
    let running_bots = bots_data.get("running_bots").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if running_bots.is_empty() {
        return None;
    }

    let mut bot_platforms: Vec<(String, String)> = running_bots
        .iter()
        .filter_map(|b| {
            let platform = b.get("platform")?.as_str()?.to_string();
            let meeting_id = b.get("native_meeting_id")?.as_str()?.to_string();
            Some((meeting_id, platform))
        })
        .collect();

    if bot_platforms.is_empty() {
        if let Ok(resp) = state
            .http
            .get(format!("{}/meetings", state.config.transcription_collector_url))
            .headers(headers.clone())
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            if let Ok(data) = resp.json::<Value>().await {
                let list = data.get("meetings").cloned().unwrap_or(data);
                if let Some(arr) = list.as_array() {
                    for m in arr {
                        let status = m.get("status").and_then(|v| v.as_str()).unwrap_or("");
                        if !["active", "requested", "joining"].contains(&status) {
                            continue;
                        }
                        let mid = m
                            .get("native_meeting_id")
                            .or_else(|| m.get("platform_specific_id"))
                            .and_then(|v| v.as_str());
                        let plat = m.get("platform").and_then(|v| v.as_str());
                        if let (Some(mid), Some(plat)) = (mid, plat) {
                            bot_platforms.push((mid.to_string(), plat.to_string()));
                        }
                    }
                }
            }
        }
    }

    if bot_platforms.is_empty() {
        return None;
    }

    let mut active_meetings = Vec::new();
    for (meeting_id, platform) in bot_platforms {
        let mut segments = Vec::new();
        if let Ok(resp) = state
            .http
            .get(format!(
                "{}/transcripts/{}/{}",
                state.config.transcription_collector_url, platform, meeting_id
            ))
            .headers(headers.clone())
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            if let Ok(data) = resp.json::<Value>().await {
                let all: Vec<Value> = data.get("segments").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let start = all.len().saturating_sub(50);
                segments = all[start..].to_vec();
            }
        }

        let participants: HashSet<String> = segments
            .iter()
            .filter_map(|s| s.get("speaker").and_then(|v| v.as_str()).map(String::from))
            .filter(|s| !s.is_empty())
            .collect();
        let latest_segments: Vec<Value> = segments
            .iter()
            .map(|s| {
                json!({
                    "speaker": s.get("speaker").and_then(|v| v.as_str()).unwrap_or("Unknown"),
                    "text": s.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                    "timestamp": s.get("absolute_start_time")
                        .or_else(|| s.get("start_time"))
                        .or_else(|| s.get("start"))
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                })
            })
            .collect();

        active_meetings.push(json!({
            "meeting_id": meeting_id,
            "platform": platform,
            "status": "active",
            "participants": participants.into_iter().collect::<Vec<_>>(),
            "latest_segments": latest_segments,
        }));
    }

    if active_meetings.is_empty() {
        return None;
    }
    Some(json!({"active_meetings": active_meetings}).to_string())
}

async fn agent_chat_proxy(State(state): State<AppState>, req: Request<Body>) -> Response {
    let Some(agent_api_url) = state.config.agent_api_url.clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Agent API not configured").into_response();
    };

    let orig_headers = req.headers().clone();
    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Failed to read body: {e}")).into_response(),
    };

    let mut meeting_context: Option<String> = None;
    if let Ok(parsed) = serde_json::from_slice::<Value>(&body) {
        let user_id = parsed.get("user_id").and_then(|v| v.as_str()).map(String::from);
        let session_id = parsed.get("session_id").and_then(|v| v.as_str()).map(String::from);
        if let (Some(user_id), Some(session_id)) = (user_id, session_id) {
            let mut redis = state.redis.clone();
            let meta_raw: Option<String> = redis.hget(format!("agent:sessions:{user_id}"), &session_id).await.ok().flatten();
            if let Some(meta_raw) = meta_raw {
                if let Ok(meta) = serde_json::from_str::<Value>(&meta_raw) {
                    if meta.get("meeting_aware").and_then(|v| v.as_bool()).unwrap_or(false) {
                        let mut internal_uid = user_id.clone();
                        if let Some(client_key) = orig_headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
                            if let Some(user_data) = resolve_token(&state, client_key).await {
                                if let Some(uid) = user_data.get("user_id") {
                                    internal_uid = uid.to_string().trim_matches('"').to_string();
                                }
                            }
                        }
                        meeting_context = get_meeting_context(&state, &internal_uid).await;
                    }
                }
            }
        }
    }

    let excluded = ["host", "content-length", "transfer-encoding", "x-user-id", "x-user-scopes", "x-user-limits",
        "x-user-webhook-url", "x-user-webhook-secret", "x-user-webhook-events"];
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in orig_headers.iter() {
        if !excluded.contains(&name.as_str().to_lowercase().as_str()) {
            headers.insert(name.clone(), value.clone());
        }
    }
    if let Some(client_key) = orig_headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        if let Some(user_data) = resolve_token(&state, client_key).await {
            if let Some(uid) = user_data.get("user_id") {
                if let Ok(v) = HeaderValue::from_str(uid.to_string().trim_matches('"')) {
                    headers.insert(HeaderName::from_static("x-user-id"), v);
                }
            }
            if let Ok(v) = HeaderValue::from_str(&user_scopes(&user_data).join(",")) {
                headers.insert(HeaderName::from_static("x-user-scopes"), v);
            }
            let limits = user_data.get("max_concurrent").and_then(|v| v.as_i64()).unwrap_or(1);
            headers.insert(HeaderName::from_static("x-user-limits"), limits.to_string().parse().unwrap());
        }
    }
    if let Some(ctx) = &meeting_context {
        if let Ok(v) = HeaderValue::from_str(ctx) {
            headers.insert(HeaderName::from_static("x-meeting-context"), v);
            tracing::info!("Meeting context injected ({} bytes)", ctx.len());
        }
    }

    let sse_client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to build client: {e}")).into_response(),
    };

    let url = format!("{agent_api_url}/api/chat");
    let resp = match sse_client.post(&url).headers(headers).body(body).send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, format!("Agent API unavailable: {e}")).into_response(),
    };

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut resp_headers = HeaderMap::new();
    for (name, value) in resp.headers().iter() {
        let lname = name.as_str().to_lowercase();
        if !["content-length", "transfer-encoding", "content-encoding"].contains(&lname.as_str()) {
            resp_headers.insert(name.clone(), value.clone());
        }
    }

    let stream = resp.bytes_stream().map_err(std::io::Error::other);
    (status, resp_headers, Body::from_stream(stream)).into_response()
}
