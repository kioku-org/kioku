//! /speak, /chat, /screen, /avatar, /events — faithful port of voice_agent.py. Thin wrappers
//! that publish Redis pub/sub commands to the bot container; all endpoint paths and Redis
//! channels are frozen (bot-side consumer).

use crate::models::Meeting;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

fn json_error(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({"detail": detail}))).into_response()
}

const ACTIVE_STATUSES: &[&str] = &["requested", "joining", "awaiting_admission", "active"];

async fn find_active_meeting(state: &AppState, user_id: i32, platform: &str, native_meeting_id: &str) -> Option<Meeting> {
    sqlx::query_as(
        "SELECT * FROM meetings WHERE user_id = $1 AND platform = $2 AND platform_specific_id = $3 \
         AND status = ANY($4) ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(platform)
    .bind(native_meeting_id)
    .bind(ACTIVE_STATUSES)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
}

async fn find_meeting_any_status(state: &AppState, user_id: i32, platform: &str, native_meeting_id: &str) -> Option<Meeting> {
    sqlx::query_as("SELECT * FROM meetings WHERE user_id = $1 AND platform = $2 AND platform_specific_id = $3 ORDER BY created_at DESC LIMIT 1")
        .bind(user_id)
        .bind(platform)
        .bind(native_meeting_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

async fn publish_command(state: &AppState, meeting_id: i32, command: Value) {
    let mut redis = state.redis.clone();
    let channel = format!("bot_commands:meeting:{meeting_id}");
    let _: Result<(), _> = redis::AsyncCommands::publish(&mut redis, &channel, command.to_string()).await;
}

pub async fn bot_speak(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>, Json(req): Json<Value>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };

    let command = if let Some(text) = req.get("text").and_then(Value::as_str) {
        json!({
            "action": "speak", "meeting_id": meeting.id, "text": text,
            "provider": req.get("provider").and_then(Value::as_str).unwrap_or("piper"),
            "voice": req.get("voice").and_then(Value::as_str).unwrap_or("auto"),
        })
    } else if req.get("audio_url").is_some() || req.get("audio_base64").is_some() {
        json!({
            "action": "speak_audio", "meeting_id": meeting.id,
            "audio_url": req.get("audio_url"), "audio_base64": req.get("audio_base64"),
            "format": req.get("format").and_then(Value::as_str).unwrap_or("wav"),
            "sample_rate": req.get("sample_rate").and_then(Value::as_i64).unwrap_or(24000),
        })
    } else {
        return json_error(StatusCode::BAD_REQUEST, "Must provide one of: text, audio_url, or audio_base64");
    };

    publish_command(&state, meeting.id, command).await;
    (StatusCode::ACCEPTED, Json(json!({"message": "Speak command sent", "meeting_id": meeting.id}))).into_response()
}

pub async fn bot_speak_stop(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };
    publish_command(&state, meeting.id, json!({"action": "speak_stop", "meeting_id": meeting.id})).await;
    (StatusCode::ACCEPTED, Json(json!({"message": "Speak stop command sent", "meeting_id": meeting.id}))).into_response()
}

pub async fn bot_chat_send(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>, Json(req): Json<Value>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };
    let Some(text) = req.get("text").and_then(Value::as_str) else {
        return json_error(StatusCode::BAD_REQUEST, "text is required");
    };
    publish_command(&state, meeting.id, json!({"action": "chat_send", "meeting_id": meeting.id, "text": text})).await;
    (StatusCode::ACCEPTED, Json(json!({"message": "Chat message sent", "meeting_id": meeting.id}))).into_response()
}

pub async fn bot_chat_read(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_meeting_any_status(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Meeting not found");
    };

    let mut redis = state.redis.clone();
    let raw: Vec<String> = redis::AsyncCommands::lrange(&mut redis, format!("meeting:{}:chat_messages", meeting.id), 0, -1).await.unwrap_or_default();
    let mut messages: Vec<Value> = raw.iter().filter_map(|r| serde_json::from_str(r).ok()).collect();
    if messages.is_empty() {
        messages = meeting.data.get("chat_messages").and_then(Value::as_array).cloned().unwrap_or_default();
    }
    Json(json!({"messages": messages, "meeting_id": meeting.id})).into_response()
}

pub async fn bot_screen_show(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>, Json(req): Json<Value>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };

    let content_type = req.get("type").and_then(Value::as_str).unwrap_or("");
    if !["image", "video", "url", "html"].contains(&content_type) {
        return json_error(StatusCode::BAD_REQUEST, "type must be one of: image, video, url, html");
    }
    if content_type == "html" && req.get("html").and_then(Value::as_str).map(str::is_empty).unwrap_or(true) {
        return json_error(StatusCode::BAD_REQUEST, "html content is required for type=html");
    }
    if content_type != "html" && req.get("url").and_then(Value::as_str).map(str::is_empty).unwrap_or(true) {
        return json_error(StatusCode::BAD_REQUEST, &format!("url is required for type={content_type}"));
    }

    publish_command(
        &state,
        meeting.id,
        json!({
            "action": "screen_show", "meeting_id": meeting.id, "type": content_type,
            "url": req.get("url"), "html": req.get("html"),
            "start_share": req.get("start_share").and_then(Value::as_bool).unwrap_or(true),
        }),
    )
    .await;
    (StatusCode::ACCEPTED, Json(json!({"message": "Screen content command sent", "meeting_id": meeting.id}))).into_response()
}

pub async fn bot_screen_stop(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };
    publish_command(&state, meeting.id, json!({"action": "screen_stop", "meeting_id": meeting.id})).await;
    (StatusCode::ACCEPTED, Json(json!({"message": "Screen stop command sent", "meeting_id": meeting.id}))).into_response()
}

pub async fn bot_avatar_set(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>, Json(req): Json<Value>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };
    if req.get("url").and_then(Value::as_str).map(str::is_empty).unwrap_or(true) && req.get("image_base64").and_then(Value::as_str).map(str::is_empty).unwrap_or(true) {
        return json_error(StatusCode::BAD_REQUEST, "Either 'url' or 'image_base64' must be provided");
    }
    publish_command(&state, meeting.id, json!({"action": "avatar_set", "meeting_id": meeting.id, "url": req.get("url"), "image_base64": req.get("image_base64")})).await;
    (StatusCode::ACCEPTED, Json(json!({"message": "Avatar set command sent", "meeting_id": meeting.id}))).into_response()
}

pub async fn bot_avatar_reset(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };
    publish_command(&state, meeting.id, json!({"action": "avatar_reset", "meeting_id": meeting.id})).await;
    (StatusCode::ACCEPTED, Json(json!({"message": "Avatar reset command sent", "meeting_id": meeting.id}))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    #[serde(default = "default_events_limit")]
    limit: i64,
}
fn default_events_limit() -> i64 {
    20
}

pub async fn bot_events(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>, Query(q): Query<EventsQuery>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_active_meeting(&state, user.user_id, &platform, &native_meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Active meeting not found");
    };

    let mut redis = state.redis.clone();
    let raw: Vec<String> = redis::AsyncCommands::lrange(&mut redis, format!("va:meeting:{}:event_log", meeting.id), -(q.limit as isize), -1).await.unwrap_or_default();
    let events: Vec<Value> = raw.iter().filter_map(|r| serde_json::from_str(r).ok()).collect();
    let count = events.len();
    Json(json!({"events": events, "meeting_id": meeting.id, "count": count})).into_response()
}
