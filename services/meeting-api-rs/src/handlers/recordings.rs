//! /recordings/* and /internal/recordings/upload — faithful port of recordings.py.
//!
//! Recordings are JSONB-only (meeting.data.recordings[]) — the separate `recordings`/
//! `media_files` SQL tables in models.rs mirror legacy tables the Python service also no
//! longer queries via ORM; the real read/write path scans meeting.data directly, same here.

use crate::collector_pipeline::{verify_meeting_token, MeetingTokenClaims};
use crate::models::Meeting;
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

fn json_error(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({"detail": detail}))).into_response()
}

fn media_content_type(media_type: &str, media_format: &str) -> &'static str {
    let fmt = media_format.to_lowercase();
    let typ = media_type.to_lowercase();
    if fmt == "webm" {
        return if typ == "audio" { "audio/webm" } else { "video/webm" };
    }
    match fmt.as_str() {
        "wav" => "audio/wav",
        "opus" => "audio/opus",
        "mp3" => "audio/mpeg",
        "jpg" => "image/jpeg",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

fn normalize_recording(rec: &Value, meeting_id: i32) -> Value {
    let mut rec = rec.clone();
    let obj = rec.as_object_mut().expect("recording entries are always objects");
    obj.entry("meeting_id").or_insert(json!(meeting_id));
    obj.entry("source").or_insert(json!("bot"));
    obj.entry("status").or_insert(json!("completed"));
    obj.entry("media_files").or_insert(json!([]));
    rec
}

fn new_recording_numeric_id() -> i64 {
    (Uuid::new_v4().as_u128() % 900_000_000_000) as i64 + 100_000_000_000
}

fn require_recording_upload_token(headers: &HeaderMap, secret: &str) -> Result<MeetingTokenClaims, &'static str> {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
    let Some(token) = auth.strip_prefix("Bearer ") else {
        return Err("Missing recording upload token");
    };
    verify_meeting_token(secret, token.trim()).ok_or("Invalid recording upload token")
}

#[derive(Debug, Default, Deserialize)]
struct UploadMetadata {
    session_uid: Option<String>,
    media_type: Option<String>,
    format: Option<String>,
    duration_seconds: Option<f64>,
    sample_rate: Option<i64>,
    is_final: Option<Value>,
    chunk_seq: Option<Value>,
}

fn to_bool(v: &Value, default: bool) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(default),
        Value::String(s) => matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        _ => default,
    }
}

/// Internal chunk-upload endpoint for the bot — faithful port of recordings.py's
/// `internal_upload_recording`. Auth is a MeetingToken (same one collector_pipeline verifies
/// for the transcription stream), not the dashboard's user Bearer token.
pub async fn upload_recording(State(state): State<AppState>, headers: HeaderMap, mut multipart: Multipart) -> Response {
    let token_claims = match require_recording_upload_token(&headers, &state.config.admin_token) {
        Ok(c) => c,
        Err(msg) => return json_error(StatusCode::FORBIDDEN, msg),
    };

    let mut file_data: Option<Vec<u8>> = None;
    let mut metadata_raw: Option<String> = None;
    let mut session_uid: Option<String> = None;
    let mut media_type = "audio".to_string();
    let mut media_format = "wav".to_string();
    let mut duration_seconds: Option<f64> = None;
    let mut sample_rate: Option<i64> = None;
    let mut is_final = true;
    let mut chunk_seq: i64 = 0;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("Invalid multipart body: {e}")),
        };
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => match field.bytes().await {
                Ok(b) => file_data = Some(b.to_vec()),
                Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("Failed to read file field: {e}")),
            },
            "metadata" => metadata_raw = field.text().await.ok(),
            "session_uid" => session_uid = field.text().await.ok(),
            "media_type" => {
                if let Ok(v) = field.text().await {
                    media_type = v;
                }
            }
            "media_format" => {
                if let Ok(v) = field.text().await {
                    media_format = v;
                }
            }
            "duration_seconds" => {
                if let Ok(v) = field.text().await {
                    duration_seconds = v.parse().ok();
                }
            }
            "sample_rate" => {
                if let Ok(v) = field.text().await {
                    sample_rate = v.parse().ok();
                }
            }
            "is_final" => {
                if let Ok(v) = field.text().await {
                    is_final = to_bool(&Value::String(v), true);
                }
            }
            "chunk_seq" => {
                if let Ok(v) = field.text().await {
                    chunk_seq = v.parse().unwrap_or(0);
                }
            }
            _ => {}
        }
    }

    if let Some(raw) = &metadata_raw {
        let meta: UploadMetadata = match serde_json::from_str(raw) {
            Ok(m) => m,
            Err(_) => return json_error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid JSON in metadata"),
        };
        session_uid = session_uid.or(meta.session_uid);
        if let Some(v) = meta.media_type {
            media_type = v;
        }
        if let Some(v) = meta.format {
            media_format = v;
        }
        if meta.duration_seconds.is_some() {
            duration_seconds = meta.duration_seconds;
        }
        if meta.sample_rate.is_some() {
            sample_rate = meta.sample_rate;
        }
        if let Some(v) = meta.is_final {
            is_final = to_bool(&v, true);
        }
        if let Some(v) = meta.chunk_seq {
            if let Some(n) = v.as_i64() {
                chunk_seq = n;
            }
        }
    }

    let Some(session_uid) = session_uid else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "session_uid is required");
    };
    let file_data = file_data.unwrap_or_default();
    let file_size = file_data.len() as i64;

    let meeting_id: Option<i32> = sqlx::query_scalar("SELECT meeting_id FROM meeting_sessions WHERE session_uid = $1").bind(&session_uid).fetch_optional(&state.db).await.unwrap_or(None);
    let Some(meeting_id) = meeting_id else {
        if !is_final {
            return Json(json!({"status": "pending", "detail": format!("Meeting session not ready yet: {session_uid}")})).into_response();
        }
        return json_error(StatusCode::NOT_FOUND, &format!("Meeting session not found: {session_uid}"));
    };

    if token_claims.meeting_id != meeting_id {
        return json_error(StatusCode::FORBIDDEN, "Recording token does not match meeting");
    }

    let Some(meeting): Option<Meeting> = sqlx::query_as("SELECT * FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(&state.db).await.unwrap_or(None) else {
        return json_error(StatusCode::NOT_FOUND, &format!("Meeting not found for session: {session_uid}"));
    };
    let user_id = meeting.user_id;

    // Pre-lock recording_id lookup (best-effort; re-derived under the row lock below too).
    let recordings_list: Vec<Value> = meeting.data.get("recordings").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut recording_id = new_recording_numeric_id();
    for rec in &recordings_list {
        if rec.get("session_uid").and_then(Value::as_str) == Some(session_uid.as_str()) && rec.get("source").and_then(Value::as_str) == Some("bot") {
            recording_id = rec.get("id").and_then(Value::as_i64).unwrap_or(recording_id);
            break;
        }
    }

    // Path includes media_type: audio and video chunks can share format+chunk_seq, and
    // without the type segment the second upload would silently overwrite the first.
    let storage_path = format!("recordings/{user_id}/{recording_id}/{session_uid}/{media_type}/{chunk_seq:06}.{media_format}");
    let content_type = media_content_type(&media_type, &media_format);

    let storage = match crate::storage::create_storage_client(&state.config).await {
        Ok(s) => s,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Storage client init failed: {e}")),
    };
    if let Err(e) = storage.upload_file(&storage_path, file_data, content_type).await {
        tracing::error!(session_uid, error = %e, "storage upload failed for recording chunk");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to upload recording to storage");
    }

    // Row-locked JSONB update: SELECT ... FOR UPDATE held before the recording snapshot so
    // concurrent audio+video chunk uploads for the same meeting serialize instead of
    // clobbering each other's media_files entries.
    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("db error: {e}")),
    };
    let data: Option<Value> = sqlx::query_scalar("SELECT data FROM meetings WHERE id = $1 FOR UPDATE").bind(meeting_id).fetch_optional(&mut *tx).await.unwrap_or(None);
    let Some(mut data) = data else {
        let _ = tx.rollback().await;
        return json_error(StatusCode::NOT_FOUND, &format!("Meeting not found for session: {session_uid}"));
    };
    let mut recordings_list: Vec<Value> = data.get("recordings").and_then(Value::as_array).cloned().unwrap_or_default();

    let mut existing_idx: Option<usize> = None;
    for (idx, rec) in recordings_list.iter().enumerate() {
        if rec.get("session_uid").and_then(Value::as_str) == Some(session_uid.as_str()) && rec.get("source").and_then(Value::as_str) == Some("bot") {
            existing_idx = Some(idx);
            recording_id = rec.get("id").and_then(Value::as_i64).unwrap_or(recording_id);
            break;
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let (mut rec_payload, was_completed) = match existing_idx {
        Some(idx) => {
            let rec = recordings_list[idx].clone();
            let completed = rec.get("status").and_then(Value::as_str) == Some("completed");
            (rec, completed)
        }
        None => (
            json!({
                "id": recording_id,
                "meeting_id": meeting.id,
                "user_id": user_id,
                "session_uid": session_uid,
                "source": "bot",
                "status": if is_final { "completed" } else { "in_progress" },
                "created_at": now,
                "completed_at": if is_final { Some(now.clone()) } else { None },
                "media_files": [],
            }),
            false,
        ),
    };
    let existing_idx = existing_idx.unwrap_or(recordings_list.len());

    let prior_media_files: Vec<Value> = rec_payload.get("media_files").and_then(Value::as_array).cloned().unwrap_or_default();
    let prior_same_type = prior_media_files.iter().find(|mf| mf.get("type").and_then(Value::as_str) == Some(media_type.as_str())).cloned();
    let prior_same_type = prior_same_type.as_ref();
    let prior_bytes = prior_same_type.and_then(|mf| mf.get("file_size_bytes")).and_then(Value::as_i64).unwrap_or(0);
    let prior_chunk_count = prior_same_type.and_then(|mf| mf.get("chunk_count")).and_then(Value::as_i64).unwrap_or(if prior_same_type.is_some() { 1 } else { 0 });
    let first_chunk_at = prior_same_type.and_then(|mf| mf.get("first_chunk_at")).cloned().unwrap_or(json!(now));
    let cumulative_bytes = if prior_same_type.is_some() { prior_bytes + file_size } else { file_size };
    let cumulative_chunk_count = if prior_same_type.is_some() { prior_chunk_count + 1 } else { 1 };

    // Pack U.7 — preserve the finalized master path against a late chunk arriving after
    // recording_finalizer has already built master.{webm,wav} for this media_type.
    let prior_sp = prior_same_type.and_then(|mf| mf.get("storage_path")).and_then(Value::as_str).unwrap_or("").to_string();
    let prior_is_final = prior_same_type.and_then(|mf| mf.get("is_final")).and_then(Value::as_bool).unwrap_or(false);
    let master_finalized = prior_sp.ends_with("/audio/master.webm") || prior_sp.ends_with("/audio/master.wav") || prior_is_final;
    let new_storage_path = if master_finalized { prior_sp.clone() } else { storage_path.clone() };
    let new_is_final = master_finalized || is_final;
    if master_finalized && !is_final {
        tracing::warn!(meeting_id, recording_id, media_type, chunk_seq, storage_path = %prior_sp, "late chunk after finalize — preserving master storage_path");
    }

    let mut new_media_files: Vec<Value> = prior_media_files.into_iter().filter(|mf| mf.get("type").and_then(Value::as_str) != Some(media_type.as_str())).collect();
    new_media_files.push(json!({
        "id": prior_same_type.and_then(|mf| mf.get("id")).cloned().unwrap_or(json!(new_recording_numeric_id())),
        "type": media_type,
        "format": media_format,
        "storage_path": new_storage_path,
        "storage_backend": state.config.storage_backend,
        "file_size_bytes": cumulative_bytes,
        "last_chunk_size_bytes": file_size,
        "chunk_count": cumulative_chunk_count,
        "duration_seconds": duration_seconds,
        "chunk_seq": chunk_seq,
        "first_chunk_at": first_chunk_at,
        "metadata": sample_rate.map(|sr| json!({"sample_rate": sr})).unwrap_or(json!({})),
        "created_at": now,
        "is_final": new_is_final,
        "finalized_at": prior_same_type.and_then(|mf| mf.get("finalized_at")).cloned().unwrap_or(Value::Null),
        "finalized_by": prior_same_type.and_then(|mf| mf.get("finalized_by")).cloned().unwrap_or(Value::Null),
    }));
    rec_payload["media_files"] = json!(new_media_files);

    let mut status_transitioned_to_completed = false;
    if is_final {
        rec_payload["status"] = json!("completed");
        rec_payload["completed_at"] = json!(now);
        status_transitioned_to_completed = !was_completed;
    } else if !was_completed {
        // Terminal state is sticky — never downgrade completed -> in_progress on a stray
        // late chunk arriving after the reconciler already finalized the recording.
        rec_payload["status"] = json!("in_progress");
    }

    if existing_idx < recordings_list.len() {
        recordings_list[existing_idx] = rec_payload.clone();
    } else {
        recordings_list.push(rec_payload.clone());
    }
    data["recordings"] = json!(recordings_list);

    if let Err(e) = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting_id).execute(&mut *tx).await {
        let _ = tx.rollback().await;
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("db error: {e}"));
    }
    if let Err(e) = tx.commit().await {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("db error: {e}"));
    }

    if status_transitioned_to_completed {
        let state = state.clone();
        let payload = json!({"recording": rec_payload});
        tokio::spawn(async move {
            crate::webhooks::send_event_webhook(&state, meeting_id, "recording.completed", payload).await;
        });
    }

    let media_file_id = if is_final {
        rec_payload.get("media_files").and_then(Value::as_array).and_then(|files| files.last()).and_then(|mf| mf.get("id")).cloned()
    } else {
        None
    };

    Json(json!({
        "recording_id": rec_payload.get("id"),
        "media_file_id": media_file_id,
        "storage_path": storage_path,
        "status": rec_payload.get("status"),
        "chunk_seq": chunk_seq,
    }))
    .into_response()
}

async fn list_user_meetings_with_recordings(db: &sqlx::PgPool, user_id: i32, meeting_id: Option<i32>) -> Vec<Meeting> {
    match meeting_id {
        Some(id) => sqlx::query_as("SELECT * FROM meetings WHERE user_id = $1 AND id = $2").bind(user_id).bind(id).fetch_all(db).await.unwrap_or_default(),
        None => sqlx::query_as("SELECT * FROM meetings WHERE user_id = $1").bind(user_id).fetch_all(db).await.unwrap_or_default(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ListRecordingsQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
    meeting_id: Option<i32>,
}
fn default_limit() -> usize {
    50
}

pub async fn list_recordings(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<ListRecordingsQuery>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };

    let meetings = list_user_meetings_with_recordings(&state.db, user.user_id, q.meeting_id).await;
    let mut recordings: Vec<Value> = vec![];
    for m in &meetings {
        if let Some(recs) = m.data.get("recordings").and_then(Value::as_array) {
            for rec in recs {
                recordings.push(normalize_recording(rec, m.id));
            }
        }
    }
    recordings.sort_by(|a, b| {
        let ca = a.get("created_at").and_then(Value::as_str).unwrap_or("");
        let cb = b.get("created_at").and_then(Value::as_str).unwrap_or("");
        cb.cmp(ca)
    });
    let page: Vec<Value> = recordings.into_iter().skip(q.offset).take(q.limit).collect();
    Json(json!({"recordings": page})).into_response()
}

async fn find_recording(db: &sqlx::PgPool, user_id: i32, recording_id: i64) -> Option<(Meeting, Value)> {
    let meetings = list_user_meetings_with_recordings(db, user_id, None).await;
    for m in meetings {
        if let Some(recs) = m.data.get("recordings").and_then(Value::as_array) {
            for rec in recs {
                if rec.get("id").and_then(Value::as_i64) == Some(recording_id) {
                    let normalized = normalize_recording(rec, m.id);
                    return Some((m, normalized));
                }
            }
        }
    }
    None
}

pub async fn get_recording(State(state): State<AppState>, headers: HeaderMap, Path(recording_id): Path<i64>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    match find_recording(&state.db, user.user_id, recording_id).await {
        Some((_, rec)) => Json(rec).into_response(),
        None => json_error(StatusCode::NOT_FOUND, "Recording not found"),
    }
}

pub async fn download_media_file(State(state): State<AppState>, headers: HeaderMap, Path((recording_id, media_file_id)): Path<(i64, i64)>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some((_, rec)) = find_recording(&state.db, user.user_id, recording_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Recording not found");
    };
    let Some(mf) = rec.get("media_files").and_then(Value::as_array).and_then(|files| files.iter().find(|f| f.get("id").and_then(Value::as_i64) == Some(media_file_id))) else {
        return json_error(StatusCode::NOT_FOUND, "Media file not found");
    };
    let fmt = mf.get("format").and_then(Value::as_str).unwrap_or("bin").to_lowercase();
    let media_type = mf.get("type").and_then(Value::as_str).unwrap_or("audio");
    let content_type = media_content_type(media_type, &fmt);
    let Some(storage_path) = mf.get("storage_path").and_then(Value::as_str) else {
        return json_error(StatusCode::NOT_FOUND, "Media file storage path not set");
    };
    let storage_backend = mf.get("storage_backend").and_then(Value::as_str).unwrap_or("");
    let file_size = mf.get("file_size_bytes").cloned().unwrap_or(Value::Null);

    let storage = match crate::storage::create_storage_client(&state.config).await {
        Ok(s) => s,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Storage client init failed: {e}")),
    };
    // Master may not exist yet (still recording, or finalizer hasn't run) — 404 so the
    // caller can fall back, same contract as the Python /download endpoint.
    match storage.file_exists(storage_path).await {
        Ok(true) => {}
        _ => return json_error(StatusCode::NOT_FOUND, "Media file content not found in storage"),
    }

    let url = if storage_backend == "local" {
        format!("/recordings/{recording_id}/media/{media_file_id}/raw")
    } else {
        match storage.get_presigned_url(storage_path, 3600).await {
            Ok(u) => u,
            Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to sign URL: {e}")),
        }
    };

    Json(json!({
        "url": url,
        "download_url": url,
        "filename": format!("{recording_id}_{media_type}.{fmt}"),
        "content_type": content_type,
        "file_size_bytes": file_size,
        "expires_in": 3600,
    }))
    .into_response()
}

pub async fn delete_recording(State(state): State<AppState>, headers: HeaderMap, Path(recording_id): Path<i64>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some((meeting, rec)) = find_recording(&state.db, user.user_id, recording_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Recording not found");
    };

    if let Ok(storage) = crate::storage::create_storage_client(&state.config).await {
        if let Some(files) = rec.get("media_files").and_then(Value::as_array) {
            for mf in files {
                if let Some(path) = mf.get("storage_path").and_then(Value::as_str) {
                    if let Err(e) = storage.delete_file(path).await {
                        tracing::warn!(path, error = %e, "failed to delete recording media file");
                    }
                }
            }
        }
    }

    let mut data = meeting.data.clone();
    let remaining: Vec<Value> = data
        .get("recordings")
        .and_then(Value::as_array)
        .map(|recs| recs.iter().filter(|r| r.get("id").and_then(Value::as_i64) != Some(recording_id)).cloned().collect())
        .unwrap_or_default();
    data["recordings"] = json!(remaining);
    let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(&state.db).await;

    Json(json!({"status": "deleted", "recording_id": recording_id})).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_content_type_prefers_audio_video_split_for_webm() {
        assert_eq!(media_content_type("audio", "webm"), "audio/webm");
        assert_eq!(media_content_type("video", "webm"), "video/webm");
    }

    #[test]
    fn media_content_type_known_formats() {
        assert_eq!(media_content_type("audio", "wav"), "audio/wav");
        assert_eq!(media_content_type("audio", "mp3"), "audio/mpeg");
        assert_eq!(media_content_type("image", "png"), "image/png");
    }

    #[test]
    fn media_content_type_unknown_format_falls_back() {
        assert_eq!(media_content_type("audio", "xyz"), "application/octet-stream");
    }

    #[test]
    fn normalize_recording_fills_in_defaults() {
        let rec = json!({"id": 1});
        let normalized = normalize_recording(&rec, 42);
        assert_eq!(normalized["meeting_id"], json!(42));
        assert_eq!(normalized["source"], json!("bot"));
        assert_eq!(normalized["status"], json!("completed"));
        assert_eq!(normalized["media_files"], json!([]));
    }
}
