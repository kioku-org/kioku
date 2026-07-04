//! /recordings/* — faithful port of the read/delete side of recordings.py.
//!
//! Recordings are JSONB-only (meeting.data.recordings[]) — the separate `recordings`/
//! `media_files` SQL tables in models.rs mirror legacy tables the Python service also no
//! longer queries via ORM; the real read/write path scans meeting.data directly, same here.
//!
//! ponytail: `internal_upload_recording` (the bot's chunk-upload endpoint, with its row-locked
//! media_files bookkeeping) and `recording_finalizer`'s master-file assembly aren't ported yet
//! — see meeting-api-rs task list. Without them nothing populates real recordings, but the
//! read/delete surface below is real and independently testable against seeded data.

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
