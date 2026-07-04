//! Core meeting bot lifecycle: spawn / list / get / stop.
//!
//! ponytail: this is a deliberately-scoped MVP slice of meetings.py (2300+ lines with URL
//! parsing, dedup, webhook config extraction, dry-run mode, Zoom/Teams-specific env, S3/cookie
//! config, scheduler timeouts, and callback-driven status transitions). Status here is set
//! synchronously on spawn/stop rather than waiting for bot callbacks, because callbacks.py
//! (Pack J classifier, exit-callback handling) isn't ported yet — see meeting-api-rs task list.
//! Do not point production traffic at this until that lands.

use crate::models::Meeting;
use crate::runtime_backend;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct MeetingCreate {
    pub platform: String,
    pub native_meeting_id: String,
    pub bot_name: Option<String>,
    pub language: Option<String>,
    pub task: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MeetingResponse {
    pub id: i32,
    pub user_id: i32,
    pub platform: String,
    pub native_meeting_id: Option<String>,
    pub status: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<Meeting> for MeetingResponse {
    fn from(m: Meeting) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            platform: m.platform,
            native_meeting_id: m.platform_specific_id,
            status: m.status,
            created_at: m.created_at,
        }
    }
}

fn json_error(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({"detail": detail}))).into_response()
}

const ACTIVE_STATUSES: &[&str] = &["requested", "joining", "awaiting_admission", "active"];

pub async fn request_bot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MeetingCreate>,
) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };

    let existing: Option<(i32,)> = sqlx::query_as(
        "SELECT id FROM meetings WHERE user_id = $1 AND platform = $2 AND platform_specific_id = $3 \
         AND status = ANY($4) ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user.user_id)
    .bind(&req.platform)
    .bind(&req.native_meeting_id)
    .bind(ACTIVE_STATUSES)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    if existing.is_some() {
        return json_error(StatusCode::CONFLICT, "An active or requested meeting already exists for this platform and meeting ID");
    }

    if user.max_concurrent > 0 {
        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM meetings WHERE user_id = $1 AND status = ANY($2) AND platform != 'browser_session'",
        )
        .bind(user.user_id)
        .bind(ACTIVE_STATUSES)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
        if active_count >= user.max_concurrent {
            return json_error(StatusCode::FORBIDDEN, "User has reached the maximum concurrent bot limit.");
        }
    }

    let backend = match runtime_backend::choose_backend_for_spawn(&state.db, &state.config).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Backend selection failed: {e}")),
    };
    let data = json!({"runtime_backend": backend});

    let meeting: Meeting = match sqlx::query_as(
        "INSERT INTO meetings (user_id, platform, platform_specific_id, status, data) \
         VALUES ($1, $2, $3, 'requested', $4) RETURNING *",
    )
    .bind(user.user_id)
    .bind(&req.platform)
    .bind(&req.native_meeting_id)
    .bind(&data)
    .fetch_one(&state.db)
    .await
    {
        Ok(m) => m,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to create meeting: {e}")),
    };

    let backend_url = runtime_backend::backend_url_for_name(&state.config, backend);
    let spawn_resp = state
        .http
        .post(format!("{backend_url}/containers"))
        .json(&json!({
            "profile": "meeting",
            "config": {
                "image": state.config.bot_image_name,
                "env": {
                    "BOT_CONFIG": json!({
                        "meeting_id": meeting.id,
                        "platform": req.platform,
                        "native_meeting_id": req.native_meeting_id,
                        "bot_name": req.bot_name,
                        "language": req.language,
                        "task": req.task,
                        "redisUrl": state.config.bot_redis_url,
                        "meetingApiCallbackUrl": format!("{}/bots/internal/callback/exited", state.config.bot_meeting_api_url),
                        "internalSecret": state.config.internal_api_secret,
                    }).to_string(),
                },
            },
            "user_id": user.user_id.to_string(),
            "callback_url": format!("{}/bots/internal/callback/exited", state.config.meeting_api_url),
            "callback_headers": {"X-Internal-Secret": state.config.internal_api_secret},
            "metadata": {"meeting_id": meeting.id},
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;

    let spawn_json = match spawn_resp {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.unwrap_or(json!({})),
        _ => {
            let _ = sqlx::query("UPDATE meetings SET status = 'failed' WHERE id = $1").bind(meeting.id).execute(&state.db).await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to start bot container");
        }
    };
    let container_name = spawn_json.get("name").and_then(|v| v.as_str()).unwrap_or_default();

    // ponytail: real system waits for the bot's `active`/`joining` callback (callbacks.py, not
    // yet ported) to flip status; setting active synchronously here so this slice is testable
    // end-to-end, but this is not how the callback-driven state machine actually behaves.
    let updated: Meeting = match sqlx::query_as(
        "UPDATE meetings SET bot_container_id = $1, status = 'active' WHERE id = $2 RETURNING *",
    )
    .bind(container_name)
    .bind(meeting.id)
    .fetch_one(&state.db)
    .await
    {
        Ok(m) => m,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to update meeting: {e}")),
    };

    (StatusCode::CREATED, Json(MeetingResponse::from(updated))).into_response()
}

pub async fn get_bots_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let meetings: Vec<Meeting> = sqlx::query_as(
        "SELECT * FROM meetings WHERE user_id = $1 AND status = ANY($2) ORDER BY created_at DESC",
    )
    .bind(user.user_id)
    .bind(ACTIVE_STATUSES)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(meetings.into_iter().map(MeetingResponse::from).collect::<Vec<_>>()).into_response()
}

pub async fn get_bot_by_id(State(state): State<AppState>, headers: HeaderMap, Path(meeting_id): Path<i32>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let meeting: Option<Meeting> = sqlx::query_as("SELECT * FROM meetings WHERE id = $1 AND user_id = $2")
        .bind(meeting_id)
        .bind(user.user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    match meeting {
        Some(m) => Json(MeetingResponse::from(m)).into_response(),
        None => json_error(StatusCode::NOT_FOUND, "Meeting not found"),
    }
}

pub async fn stop_bot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((platform, native_meeting_id)): Path<(String, String)>,
) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };

    let meetings: Vec<Meeting> = sqlx::query_as(
        "SELECT * FROM meetings WHERE user_id = $1 AND platform = $2 AND platform_specific_id = $3 \
         ORDER BY created_at DESC",
    )
    .bind(user.user_id)
    .bind(&platform)
    .bind(&native_meeting_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let non_terminal: Vec<&Meeting> = meetings.iter().filter(|m| m.status != "completed" && m.status != "failed").collect();
    if non_terminal.is_empty() {
        if meetings.is_empty() {
            return json_error(StatusCode::NOT_FOUND, "No meeting found to stop.");
        }
        return Json(json!({"message": format!("Meeting already {}.", meetings[0].status)})).into_response();
    }

    // ponytail: direct stop, not the durable Redis-outbox + sweep-consumer retry pattern
    // (container_stop_outbox.py) — a transient runtime-api failure here just fails the
    // request rather than being retried. Port the outbox before relying on this for prod.
    for meeting in &non_terminal {
        if let Some(container_name) = &meeting.bot_container_id {
            let backend_url = runtime_backend::backend_url_for(&state.config, meeting);
            let _ = state
                .http
                .delete(format!("{backend_url}/containers/{container_name}"))
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await;
        }
        let _ = sqlx::query("UPDATE meetings SET status = 'completed', end_time = now() WHERE id = $1")
            .bind(meeting.id)
            .execute(&state.db)
            .await;
    }

    (StatusCode::ACCEPTED, Json(json!({"message": "Stop request accepted and is being processed."}))).into_response()
}
