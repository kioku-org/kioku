//! Core meeting bot lifecycle: spawn / list / get / stop.
//!
//! ponytail: request_bot covers the golden path (URL construction for google_meet/zoom/teams,
//! webhook config storage, recording/capture-mode + cookie-backend forwarding, MeetingToken
//! minting, meeting_sessions pre-registration) but still deliberately skips: browser_session
//! mode, agent-only mode, Zoom/Teams native-SDK env vars, dry_run test mode, and per-user
//! bot_config/automatic_leave timeout overrides (uses SYSTEM_DEFAULTS unconditionally) — none of
//! meetings.py's 2300+ lines for those are ported. Stop goes through the real durable outbox +
//! Pack J classifier + state machine, matching production behavior.

use crate::classifier::classify_stopped_exit;
use crate::collector_pipeline::mint_meeting_token;
use crate::container_stop_outbox::enqueue_stop;
use crate::meeting_status::{publish_meeting_status_change, update_meeting_status, StatusUpdateOptions};
use crate::models::Meeting;
use crate::runtime_backend;
use crate::schemas::MeetingStatus;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct MeetingCreate {
    pub platform: String,
    pub native_meeting_id: String,
    pub bot_name: Option<String>,
    pub language: Option<String>,
    pub task: Option<String>,
    pub transcription_tier: Option<String>,
    pub passcode: Option<String>,
    pub recording_enabled: Option<bool>,
    pub transcribe_enabled: Option<bool>,
    pub video: Option<bool>,
    pub authenticated: Option<bool>,
}

/// Faithful (but base_host-less) port of Python's `Platform.construct_meeting_url` — enough for
/// the three real platforms the bot supports. Returns None if the id doesn't look valid for the
/// platform, matching Python's contract (caller 422s in that case).
fn construct_meeting_url(platform: &str, native_id: &str, passcode: Option<&str>) -> Option<String> {
    match platform {
        "google_meet" => Some(format!("https://meet.google.com/{native_id}")),
        "zoom" => {
            if !native_id.chars().all(|c| c.is_ascii_digit()) || native_id.len() < 9 || native_id.len() > 11 {
                return None;
            }
            let mut url = format!("https://zoom.us/j/{native_id}");
            if let Some(pw) = passcode {
                url.push_str(&format!("?pwd={pw}"));
            }
            Some(url)
        }
        "teams" => {
            if !native_id.chars().all(|c| c.is_ascii_digit()) || native_id.len() < 10 || native_id.len() > 15 {
                return None;
            }
            let mut url = format!("https://teams.live.com/meet/{native_id}");
            if let Some(pw) = passcode {
                url.push_str(&format!("?p={pw}"));
            }
            Some(url)
        }
        _ => None,
    }
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

    let Some(meeting_url) = construct_meeting_url(&req.platform, &req.native_meeting_id, req.passcode.as_deref()) else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "Cannot construct meeting URL for this platform/native_meeting_id");
    };

    let backend = match runtime_backend::choose_backend_for_spawn(&state.db, &state.config).await {
        Ok(b) => b,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Backend selection failed: {e}")),
    };

    let transcribe_enabled = req.transcribe_enabled.unwrap_or(true);
    let recording_enabled = if req.video.unwrap_or(false) { true } else { req.recording_enabled.unwrap_or(true) };
    let capture_modes: Vec<&str> = if req.video.unwrap_or(false) { vec!["audio", "video"] } else { vec!["audio"] };

    let mut data = json!({
        "runtime_backend": backend,
        "transcribe_enabled": transcribe_enabled,
        "recording_enabled": recording_enabled,
        "capture_modes": capture_modes,
    });
    if let Some(passcode) = &req.passcode {
        data["passcode"] = json!(passcode);
    }
    // Webhook config, forwarded by api-gateway from the user's stored webhook settings — read
    // here (not at delivery time) so webhooks.rs's send_*_webhook calls always have it in
    // meeting.data, matching meetings.py.
    if let Some(webhook_url) = headers.get("x-user-webhook-url").and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty()) {
        data["webhook_url"] = json!(webhook_url);
        if let Some(secret) = headers.get("x-user-webhook-secret").and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty()) {
            data["webhook_secret"] = json!(secret);
        }
        if let Some(events) = headers.get("x-user-webhook-events").and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty()) {
            let events_map: serde_json::Map<String, Value> = events.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|e| (e.to_string(), json!(true))).collect();
            data["webhook_events"] = Value::Object(events_map);
        }
    }

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
    publish_meeting_status_change(&state, meeting.id, "requested", &req.platform, &req.native_meeting_id, user.user_id, None).await;

    let meeting_token = mint_meeting_token(&state.config.admin_token, meeting.id, user.user_id, &req.platform, &req.native_meeting_id, 7200);
    let connection_id = uuid::Uuid::new_v4().to_string();
    let bot_name = req.bot_name.clone().unwrap_or_else(|| format!("VexaBot-{}", &uuid::Uuid::new_v4().simple().to_string()[..6]));

    // SYSTEM_DEFAULTS from meetings.py — per-request/per-user overrides (AutomaticLeave,
    // user.data.bot_config) are deliberately not resolved here, see module doc comment.
    let mut bot_config = json!({
        "platform": req.platform,
        "meetingUrl": meeting_url,
        "botName": bot_name,
        "token": meeting_token,
        "nativeMeetingId": req.native_meeting_id,
        "connectionId": connection_id,
        "language": req.language,
        "task": req.task,
        "transcriptionTier": req.transcription_tier.clone().unwrap_or_else(|| "realtime".to_string()),
        "redisUrl": state.config.bot_redis_url,
        "automaticLeave": {
            "waitingRoomTimeout": 900_000,
            "noOneJoinedTimeout": 120_000,
            "everyoneLeftTimeout": 900_000,
        },
        "meetingApiCallbackUrl": format!("{}/bots/internal/callback/exited", state.config.bot_meeting_api_url),
        "internalSecret": state.config.internal_api_secret,
        "recordingEnabled": recording_enabled,
        "transcribeEnabled": transcribe_enabled,
        "captureModes": capture_modes,
        "recordingUploadUrl": format!("{}/internal/recordings/upload", state.config.bot_meeting_api_url),
    });
    if req.authenticated.unwrap_or(false) {
        bot_config["authenticated"] = json!(true);
        if state.config.cookie_storage_backend == "http" {
            bot_config["cookieStorageBackend"] = json!("http");
            bot_config["cookieServiceUrl"] = json!(state.config.cookie_service_url);
            if !state.config.cookie_service_token.is_empty() {
                bot_config["cookieServiceToken"] = json!(state.config.cookie_service_token);
            }
            bot_config["userId"] = json!(user.user_id.to_string());
        } else {
            bot_config["userdataS3Path"] = json!(format!("users/{}/browser-userdata", user.user_id));
            bot_config["s3Endpoint"] = json!(format!("{}://{}", if state.config.minio_secure { "https" } else { "http" }, state.config.minio_endpoint));
            bot_config["s3Bucket"] = json!(state.config.minio_bucket);
            bot_config["s3AccessKey"] = json!(state.config.minio_access_key);
            bot_config["s3SecretKey"] = json!(state.config.minio_secret_key);
        }
    }

    let backend_url = runtime_backend::backend_url_for_name(&state.config, backend);
    let spawn_resp = state
        .http
        .post(format!("{backend_url}/containers"))
        .json(&json!({
            "profile": "meeting",
            "config": {
                "image": state.config.bot_image_name,
                "env": {
                    "BOT_CONFIG": bot_config.to_string(),
                },
            },
            "user_id": user.user_id.to_string(),
            "callback_url": format!("{}/bots/internal/callback/exited", state.config.meeting_api_url),
            "callback_headers": {"X-Internal-Secret": state.config.internal_api_secret},
            "metadata": {"meeting_id": meeting.id, "connection_id": connection_id},
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;

    let spawn_json = match spawn_resp {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.unwrap_or(json!({})),
        _ => {
            let _ = sqlx::query("UPDATE meetings SET status = 'failed' WHERE id = $1").bind(meeting.id).execute(&state.db).await;
            publish_meeting_status_change(&state, meeting.id, "failed", &req.platform, &req.native_meeting_id, user.user_id, None).await;
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to start bot container");
        }
    };
    let container_name = spawn_json.get("name").and_then(|v| v.as_str()).unwrap_or_default();

    // Real status transitions (joining/awaiting_admission/active) come from the bot's own
    // callbacks (callbacks.rs) once it actually reaches those states — status stays 'requested'
    // here, matching meetings.py (no synchronous "set active" shortcut).
    let updated: Meeting = match sqlx::query_as("UPDATE meetings SET bot_container_id = $1 WHERE id = $2 RETURNING *")
        .bind(container_name)
        .bind(meeting.id)
        .fetch_one(&state.db)
        .await
    {
        Ok(m) => m,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to update meeting: {e}")),
    };

    // Pre-register the session_uid → meeting_id mapping the bot will use for transcription
    // segments + recording chunk uploads (collector_pipeline's reactive session_start handler
    // also upserts this, but eager registration matches meetings.py and closes the race where a
    // recording/transcript chunk could arrive before any session_start stream event).
    let _ = sqlx::query(
        "INSERT INTO meeting_sessions (meeting_id, session_uid, session_start_time) VALUES ($1, $2, now()) \
         ON CONFLICT (meeting_id, session_uid) DO NOTHING",
    )
    .bind(meeting.id)
    .bind(&connection_id)
    .execute(&state.db)
    .await;

    (StatusCode::CREATED, Json(MeetingResponse::from(updated))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ListBotsQuery {
    #[serde(default = "default_list_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    search: Option<String>,
    status: Option<String>,
    platform: Option<String>,
    /// Backward-compat opt-in for the full `data` JSONB blob instead of the summary below.
    include: Option<String>,
}
fn default_list_limit() -> i64 {
    50
}

/// Slim per-meeting summary — mirrors Python's `_data_summary` (v0.10.5 Pack L): the list view
/// only ever renders name/title, completion_reason, a few participants, a notes preview, last
/// status transition, and whether a recording exists, so send that instead of the full blob
/// (which carries status_transition[]/recordings[]/webhook_deliveries[] and gets large).
fn data_summary(d: &Value) -> Value {
    let participants: Vec<Value> = d.get("participants").and_then(Value::as_array).cloned().unwrap_or_default();
    let notes_preview = d.get("notes").and_then(Value::as_str).map(|n| n.chars().take(120).collect::<String>());
    let last_transition = d.get("status_transition").and_then(Value::as_array).and_then(|t| t.last()).cloned();
    let has_recording = d.get("recordings").and_then(Value::as_array).map(|a| !a.is_empty()).unwrap_or(false);
    json!({
        "name": d.get("name").or_else(|| d.get("title")),
        "completion_reason": d.get("completion_reason"),
        "participants": participants.iter().take(3).cloned().collect::<Vec<_>>(),
        "participants_count": participants.len(),
        "notes_preview": notes_preview,
        "languages": d.get("languages"),
        "last_transition": last_transition,
        "has_recording": has_recording,
    })
}

/// `GET /bots` — list recent meetings/bots for the authenticated user, any status. Port of
/// Python's `list_user_bots`; this is the dashboard's primary Meetings-page data source
/// (`services/dashboard/src/app/api/vexa/[...path]/route.ts`), missing entirely from the initial
/// Rust cutover (#84).
pub async fn list_bots(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<ListBotsQuery>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };

    let limit = q.limit.max(1);
    let search_pattern = q.search.as_ref().map(|s| format!("%{s}%"));

    let meetings: Vec<Meeting> = sqlx::query_as(
        "SELECT * FROM meetings WHERE user_id = $1 \
         AND ($2::text IS NULL OR platform_specific_id ILIKE $2 OR data->>'name' ILIKE $2 OR data->>'title' ILIKE $2) \
         AND ($3::text IS NULL OR status = $3) \
         AND ($4::text IS NULL OR platform = $4) \
         ORDER BY created_at DESC OFFSET $5 LIMIT $6",
    )
    .bind(user.user_id)
    .bind(&search_pattern)
    .bind(&q.status)
    .bind(&q.platform)
    .bind(q.offset)
    .bind(limit + 1)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let has_more = meetings.len() as i64 > limit;
    let include_full_data = q.include.as_deref() == Some("data");

    let items: Vec<Value> = meetings
        .into_iter()
        .take(limit as usize)
        .map(|m| {
            json!({
                "id": m.id,
                "platform": m.platform,
                "native_meeting_id": m.platform_specific_id,
                "status": m.status,
                "bot_container_id": m.bot_container_id,
                "start_time": m.start_time,
                "end_time": m.end_time,
                "data": if include_full_data { m.data.clone() } else { data_summary(&m.data) },
                "created_at": m.created_at,
                "updated_at": m.updated_at,
            })
        })
        .collect();

    Json(json!({"meetings": items, "has_more": has_more})).into_response()
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

    for meeting in &non_terminal {
        // Mark stop_requested so the eventual exit-callback's Pack J classifier knows this
        // was user-initiated (never a failure), and so late bot status_change callbacks that
        // race the DELETE are ignored instead of failing an "invalid transition".
        let mut data = meeting.data.clone();
        data["stop_requested"] = json!(true);
        let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(&state.db).await;

        match &meeting.bot_container_id {
            Some(container_name) => {
                // Send leave-via-Redis (best-effort, bot may already be gone) and enqueue a
                // durable delayed stop through the outbox — the sweep consumer (not yet
                // wired into this binary's main loop, see task #20) is what actually fires
                // runtime-api DELETE with retry. Status stays STOPPING; the real terminal
                // transition happens in the exit_callback once the container is confirmed gone.
                let mut redis = state.redis.clone();
                let channel = format!("bot_commands:meeting:{}", meeting.id);
                let _: Result<(), _> = redis::AsyncCommands::publish(&mut redis, &channel, json!({"action": "leave", "meeting_id": meeting.id}).to_string()).await;

                let backend_url = runtime_backend::backend_url_for(&state.config, meeting).to_string();
                let stop_delay = if platform == "browser_session" { 0 } else { state.config.bot_stop_delay_seconds };
                enqueue_stop(&mut redis, container_name, meeting.id, stop_delay, &backend_url).await;

                let old_status = meeting.status.clone();
                if update_meeting_status(&state.db, meeting.id, MeetingStatus::Stopping, StatusUpdateOptions { transition_reason: Some("User requested stop"), ..Default::default() }).await.unwrap_or(false) {
                    publish_meeting_status_change(&state, meeting.id, "stopping", &meeting.platform, meeting.platform_specific_id.as_deref().unwrap_or(""), meeting.user_id, None).await;
                    crate::meeting_status::schedule_status_webhook_task(&state, (*meeting).clone(), old_status, "stopping".to_string(), Some("User requested stop (fast-path)".to_string()), "user_stop");
                }
            }
            None => {
                // No container on record — classify directly via Pack J rather than assuming
                // success or failure (mirrors the no-container branch of the Python handler).
                let (target_status, reason) = classify_stopped_exit(&state.db, meeting.id, &data, meeting.start_time, meeting.end_time, crate::schemas::MeetingCompletionReason::Stopped).await;
                let old_status = meeting.status.clone();
                if update_meeting_status(&state.db, meeting.id, target_status, StatusUpdateOptions { completion_reason: Some(reason), transition_reason: Some("User requested stop (no container)"), ..Default::default() }).await.unwrap_or(false) {
                    publish_meeting_status_change(&state, meeting.id, target_status.as_str(), &meeting.platform, meeting.platform_specific_id.as_deref().unwrap_or(""), meeting.user_id, None).await;
                    crate::meeting_status::schedule_status_webhook_task(&state, (*meeting).clone(), old_status, target_status.as_str().to_string(), Some("User requested stop".to_string()), "user_stop");
                }
            }
        }
    }

    (StatusCode::ACCEPTED, Json(json!({"message": "Stop request accepted and is being processed."}))).into_response()
}

/// Stop a container via runtime-api DELETE /containers/{name}. Idempotent (200/404 both count
/// as success). Shared by the outbox sweep consumer (main.rs's periodic sweep loop) — the only
/// thing that's actually allowed to call runtime-api DELETE, per container_stop_outbox.py's
/// "exactly one mechanism" design.
pub async fn stop_via_runtime_api(http: reqwest::Client, backend_url: String, container_name: String) -> bool {
    match http.delete(format!("{backend_url}/containers/{container_name}")).timeout(std::time::Duration::from_secs(30)).send().await {
        Ok(resp) => resp.status().as_u16() == 200 || resp.status().as_u16() == 404,
        Err(e) => {
            tracing::warn!(container_name, error = %e, "runtime-api stop failed");
            false
        }
    }
}
