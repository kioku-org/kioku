use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::Value;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/transcripts/:platform/:id/share", post(create_share))
        .route("/public/transcripts/:filename", get(read_share))
}

#[derive(Deserialize)]
struct ShareQuery {
    meeting_id: Option<i64>,
    ttl_seconds: Option<i64>,
}

fn format_ts(seconds: f64) -> String {
    let s = seconds as i64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

fn render_transcript(data: &Value) -> String {
    let mut lines = vec!["MEETING TRANSCRIPT".to_string(), String::new()];
    lines.push(format!("Platform: {}", data.get("platform").and_then(|v| v.as_str()).unwrap_or("")));
    lines.push(format!(
        "Meeting ID: {}",
        data.get("native_meeting_id").and_then(|v| v.as_str()).unwrap_or("")
    ));
    if let Some(st) = data.get("start_time").and_then(|v| v.as_str()) {
        lines.push(format!("Start: {st}"));
    }
    if let Some(et) = data.get("end_time").and_then(|v| v.as_str()) {
        lines.push(format!("End: {et}"));
    }
    lines.push(String::new());
    lines.push("---".to_string());
    lines.push(String::new());

    let segments = data.get("segments").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    for seg in segments {
        let text = seg.get("text").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let timestamp = if let Some(abs) = seg.get("absolute_start_time").and_then(|v| v.as_str()) {
            DateTime::parse_from_rfc3339(&abs.replace('Z', "+00:00"))
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|_| abs.to_string())
        } else {
            let secs = seg
                .get("start_time")
                .or_else(|| seg.get("start"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            format_ts(secs)
        };
        let speaker = seg.get("speaker").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("Unknown");
        lines.push(format!("[{timestamp}] {speaker}: {text}"));
    }
    lines.join("\n").trim().to_string() + "\n"
}

fn best_base_url(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(base) = &state.config.public_base_url {
        return base.trim_end_matches('/').to_string();
    }
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    format!("{proto}://{host}").trim_end_matches('/').to_string()
}

async fn create_share(
    State(state): State<AppState>,
    Path((platform, native_id)): Path<(String, String)>,
    Query(q): Query<ShareQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(api_key) = headers.get("x-api-key").and_then(|v| v.to_str().ok()).map(str::to_string) else {
        return (StatusCode::UNAUTHORIZED, "Missing X-API-Key").into_response();
    };

    let ttl = q
        .ttl_seconds
        .unwrap_or(state.config.transcript_share_ttl_seconds)
        .clamp(60, state.config.transcript_share_ttl_max_seconds);

    let url = format!(
        "{}/transcripts/{}/{}",
        state.config.transcription_collector_url, platform, native_id
    );
    let mut req = state.http.get(&url).header("X-API-Key", &api_key);
    if let Some(mid) = q.meeting_id {
        req = req.query(&[("meeting_id", mid)]);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, format!("Failed to reach transcription service: {e}")).into_response(),
    };
    if resp.status() != StatusCode::OK {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return (status, text).into_response();
    }
    // Fetched only to confirm the transcript exists and is accessible before minting a share
    // link; the text itself is rendered fresh from a live fetch on each read (see `read_share`).
    let _data: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Invalid transcript payload: {e}")).into_response(),
    };

    let share_id = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use rand::RngCore;
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    };
    let redis_key = format!("share:transcript:{share_id}");
    let metadata = serde_json::json!({
        "platform": platform,
        "native_meeting_id": native_id,
        "meeting_id": q.meeting_id,
        "api_key": api_key,
    });

    let mut redis = state.redis.clone();
    if let Err(e) = redis.set_ex::<_, _, ()>(&redis_key, metadata.to_string(), ttl as u64).await {
        return (StatusCode::SERVICE_UNAVAILABLE, format!("Failed to store share token: {e}")).into_response();
    }

    let base = best_base_url(&state, &headers);
    let public_url = format!("{base}/public/transcripts/{share_id}.txt");
    let expires_at = Utc::now() + chrono::Duration::seconds(ttl);

    Json(serde_json::json!({
        "share_id": share_id,
        "url": public_url,
        "expires_at": expires_at.to_rfc3339(),
        "expires_in_seconds": ttl,
    }))
    .into_response()
}

async fn read_share(State(state): State<AppState>, Path(filename): Path<String>) -> Response {
    let Some(share_id) = filename.strip_suffix(".txt") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let redis_key = format!("share:transcript:{share_id}");

    let mut redis = state.redis.clone();
    let metadata_json: Option<String> = match redis.get(&redis_key).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, format!("Failed to read share token: {e}")).into_response(),
    };
    let Some(metadata_json) = metadata_json else {
        return (StatusCode::NOT_FOUND, "Share link expired or not found").into_response();
    };
    let metadata: Value = match serde_json::from_str(&metadata_json) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid share metadata: {e}")).into_response(),
    };

    let platform = metadata.get("platform").and_then(|v| v.as_str()).unwrap_or("");
    let native_id = metadata.get("native_meeting_id").and_then(|v| v.as_str()).unwrap_or("");
    let api_key = metadata.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let meeting_id = metadata.get("meeting_id").and_then(|v| v.as_i64());

    let url = format!("{}/transcripts/{}/{}", state.config.transcription_collector_url, platform, native_id);
    let mut req = state.http.get(&url).header("X-API-Key", api_key).timeout(std::time::Duration::from_secs(30));
    if let Some(mid) = meeting_id {
        req = req.query(&[("meeting_id", mid)]);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) if e.is_timeout() => return (StatusCode::GATEWAY_TIMEOUT, "Transcript fetch timeout").into_response(),
        Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, format!("Failed to reach transcription service: {e}")).into_response(),
    };
    if resp.status() != StatusCode::OK {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return (status, format!("Failed to fetch transcript: {text}")).into_response();
    }
    let data: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Invalid transcript payload: {e}")).into_response(),
    };

    let text = render_transcript(&data);
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "text/plain; charset=utf-8".parse().unwrap());
    headers.insert("cache-control", "no-store, no-cache, must-revalidate".parse().unwrap());
    headers.insert("x-robots-tag", "noindex, nofollow, noarchive".parse().unwrap());
    (StatusCode::OK, headers, text).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_ts_hides_hours_when_zero() {
        assert_eq!(format_ts(65.0), "01:05");
        assert_eq!(format_ts(3665.0), "01:01:05");
    }

    #[test]
    fn render_transcript_skips_blank_segments() {
        let data = serde_json::json!({
            "platform": "zoom",
            "native_meeting_id": "123",
            "segments": [
                {"speaker": "Alice", "text": "hello", "start_time": 5.0},
                {"speaker": "Bob", "text": "   "},
            ]
        });
        let out = render_transcript(&data);
        assert!(out.contains("Alice: hello"));
        assert!(!out.contains("Bob"));
    }
}
