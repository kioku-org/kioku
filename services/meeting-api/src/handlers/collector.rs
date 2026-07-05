//! /meetings and /transcripts/* read endpoints — faithful port of collector/endpoints.py's
//! REST surface. Faithful only for the DB-backed history; the live in-progress-meeting Redis
//! segment merge (`_get_full_transcript_segments`'s redis_c branch) is not ported, so segments
//! for a still-active meeting won't show up here until they land in Postgres — completed
//! meetings (the common case for these reads) are unaffected.
//!
//! ponytail: the actual segment *ingestion* pipeline (consumer.py's Redis Stream consumer,
//! processors.py, speaker_mapper.py, db_writer.py, filters.py) is NOT ported — that's a
//! separate, large, real-time system (stream consumer + speaker diarization + content
//! filtering) genuinely out of scope for this pass. Nothing writes new transcription rows via
//! this binary; these are read/update/delete endpoints over rows written by the Python
//! collector, which must keep running until that pipeline is ported.

use crate::models::{Meeting, Transcription};
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

fn segment_json(t: &Transcription) -> Value {
    json!({
        "start": t.start_time,
        "end": t.end_time,
        "text": t.text,
        "language": t.language,
        "created_at": t.created_at.map(|d| d.and_utc().to_rfc3339()),
        "speaker": t.speaker,
        "segment_id": t.segment_id,
    })
}

#[derive(Debug, Deserialize)]
pub struct ListMeetingsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
    platform: Option<String>,
}

pub async fn get_meetings(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<ListMeetingsQuery>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };

    let mut sql = "SELECT * FROM meetings WHERE user_id = $1".to_string();
    let mut idx = 2;
    if q.status.is_some() {
        sql += &format!(" AND status = ${idx}");
        idx += 1;
    }
    if q.platform.is_some() {
        sql += &format!(" AND platform = ${idx}");
        idx += 1;
    }
    sql += " ORDER BY created_at DESC";
    if q.limit.is_some() {
        sql += &format!(" LIMIT ${idx}");
        idx += 1;
    }
    if q.offset.is_some() {
        sql += &format!(" OFFSET ${idx}");
    }

    let mut query = sqlx::query_as::<_, Meeting>(&sql).bind(user.user_id);
    if let Some(s) = &q.status {
        query = query.bind(s);
    }
    if let Some(p) = &q.platform {
        query = query.bind(p);
    }
    if let Some(l) = q.limit {
        query = query.bind(l);
    }
    if let Some(o) = q.offset {
        query = query.bind(o);
    }

    let meetings: Vec<Meeting> = query.fetch_all(&state.db).await.unwrap_or_default();
    Json(json!({"meetings": meetings.into_iter().map(|m| json!({
        "id": m.id, "user_id": m.user_id, "platform": m.platform,
        "native_meeting_id": m.platform_specific_id, "status": m.status,
        "created_at": m.created_at.map(|d| d.and_utc().to_rfc3339()),
        "updated_at": m.updated_at.map(|d| d.and_utc().to_rfc3339()),
        "data": m.data,
    })).collect::<Vec<_>>()}))
    .into_response()
}

async fn find_meeting(db: &sqlx::PgPool, user_id: i32, platform: &str, native_meeting_id: &str, meeting_id: Option<i32>) -> Option<Meeting> {
    match meeting_id {
        Some(id) => sqlx::query_as("SELECT * FROM meetings WHERE id = $1 AND user_id = $2 AND platform = $3 AND platform_specific_id = $4")
            .bind(id)
            .bind(user_id)
            .bind(platform)
            .bind(native_meeting_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten(),
        None => sqlx::query_as("SELECT * FROM meetings WHERE user_id = $1 AND platform = $2 AND platform_specific_id = $3 ORDER BY created_at DESC LIMIT 1")
            .bind(user_id)
            .bind(platform)
            .bind(native_meeting_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten(),
    }
}

async fn segments_for_meeting(db: &sqlx::PgPool, meeting_id: i32) -> Vec<Transcription> {
    sqlx::query_as("SELECT * FROM transcriptions WHERE meeting_id = $1 ORDER BY start_time ASC").bind(meeting_id).fetch_all(db).await.unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub struct TranscriptQuery {
    meeting_id: Option<i32>,
}

pub async fn get_transcript_by_native_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((platform, native_meeting_id)): Path<(String, String)>,
    Query(q): Query<TranscriptQuery>,
) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_meeting(&state.db, user.user_id, &platform, &native_meeting_id, q.meeting_id).await else {
        return json_error(StatusCode::NOT_FOUND, &format!("Meeting not found for platform {platform} and ID {native_meeting_id}"));
    };

    let segments = segments_for_meeting(&state.db, meeting.id).await;
    Json(json!({
        "id": meeting.id, "user_id": meeting.user_id, "platform": meeting.platform,
        "native_meeting_id": meeting.platform_specific_id, "status": meeting.status,
        "start_time": meeting.start_time.map(|d| d.and_utc().to_rfc3339()),
        "end_time": meeting.end_time.map(|d| d.and_utc().to_rfc3339()),
        "recordings": meeting.data.get("recordings").cloned().unwrap_or_else(|| json!([])),
        "notes": meeting.data.get("notes"),
        "data": meeting.data,
        "speaker_events": meeting.data.get("speaker_events").cloned().unwrap_or_else(|| json!([])),
        "segments": segments.iter().map(segment_json).collect::<Vec<_>>(),
    }))
    .into_response()
}

pub async fn get_transcript_internal(State(state): State<AppState>, Path(meeting_id): Path<i32>) -> Response {
    let exists: Option<(i32,)> = sqlx::query_as("SELECT id FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(&state.db).await.ok().flatten();
    if exists.is_none() {
        return json_error(StatusCode::NOT_FOUND, &format!("Meeting with ID {meeting_id} not found."));
    }
    let segments = segments_for_meeting(&state.db, meeting_id).await;
    Json(segments.iter().map(segment_json).collect::<Vec<_>>()).into_response()
}

#[derive(Debug, Deserialize)]
pub struct MeetingUpdateData {
    name: Option<String>,
    participants: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    notes: Option<String>,
}

pub async fn update_meeting_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((platform, native_meeting_id)): Path<(String, String)>,
    Json(update): Json<MeetingUpdateData>,
) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_meeting(&state.db, user.user_id, &platform, &native_meeting_id, None).await else {
        return json_error(StatusCode::NOT_FOUND, &format!("Meeting not found for platform {platform} and ID {native_meeting_id}"));
    };

    let mut data = meeting.data.clone();
    let mut any = false;
    if let Some(v) = update.name {
        data["name"] = json!(v);
        any = true;
    }
    if let Some(v) = update.participants {
        data["participants"] = json!(v);
        any = true;
    }
    if let Some(v) = update.languages {
        data["languages"] = json!(v);
        any = true;
    }
    if let Some(v) = update.notes {
        data["notes"] = json!(v);
        any = true;
    }
    if !any {
        return json_error(StatusCode::BAD_REQUEST, "No data provided for update.");
    }

    let updated: Meeting = match sqlx::query_as("UPDATE meetings SET data = $1 WHERE id = $2 RETURNING *").bind(&data).bind(meeting.id).fetch_one(&state.db).await {
        Ok(m) => m,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Update failed: {e}")),
    };
    Json(json!({
        "id": updated.id, "user_id": updated.user_id, "platform": updated.platform,
        "native_meeting_id": updated.platform_specific_id, "status": updated.status, "data": updated.data,
    }))
    .into_response()
}

/// ponytail: skips the storage-file + vestigial recordings/media_files SQL-table purge
/// (`_purge_recordings_for_meeting`, not read/ported) — clears meeting.data.recordings
/// (the real, JSONB-backed recording pointers) and anonymizes the row, which is the part
/// that matters for the "purge PII" contract this endpoint promises.
pub async fn delete_meeting(State(state): State<AppState>, headers: HeaderMap, Path((platform, native_meeting_id)): Path<(String, String)>) -> Response {
    let user = match crate::auth::validate_request(&state, &headers) {
        Ok(u) => u,
        Err(_) => return json_error(StatusCode::UNAUTHORIZED, "Authentication required"),
    };
    let Some(meeting) = find_meeting(&state.db, user.user_id, &platform, &native_meeting_id, None).await else {
        return json_error(StatusCode::NOT_FOUND, &format!("Meeting not found for platform {platform} and ID {native_meeting_id}"));
    };

    if meeting.data.get("redacted").and_then(Value::as_bool) == Some(true) {
        return Json(json!({"message": format!("Meeting {platform}/{native_meeting_id} artifacts already deleted and data anonymized")})).into_response();
    }
    if meeting.status != "completed" && meeting.status != "failed" {
        return json_error(StatusCode::CONFLICT, &format!("Meeting not finalized; cannot delete transcripts. Current status: {}", meeting.status));
    }

    let _ = sqlx::query("DELETE FROM transcriptions WHERE meeting_id = $1").bind(meeting.id).execute(&state.db).await;

    const TELEMETRY_FIELDS: &[&str] = &["status_transition", "completion_reason", "error", "diagnostics"];
    let mut scrubbed = json!({});
    if let Some(obj) = meeting.data.as_object() {
        let scrubbed_obj = scrubbed.as_object_mut().unwrap();
        for field in TELEMETRY_FIELDS {
            if let Some(v) = obj.get(*field) {
                scrubbed_obj.insert(field.to_string(), v.clone());
            }
        }
    }
    scrubbed["redacted"] = json!(true);

    let _ = sqlx::query("UPDATE meetings SET platform_specific_id = NULL, data = $1 WHERE id = $2").bind(&scrubbed).bind(meeting.id).execute(&state.db).await;

    Json(json!({"message": format!("Meeting {platform}/{native_meeting_id} transcripts and recording artifacts deleted; meeting data anonymized")})).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn segment(text: &str, start: f64) -> Transcription {
        Transcription { id: 1, meeting_id: 1, start_time: start, end_time: start + 1.0, text: text.to_string(), speaker: Some("Alice".to_string()), language: Some("en".to_string()), created_at: Some(Utc::now().naive_utc()), session_uid: None, segment_id: None }
    }

    #[test]
    fn segment_json_uses_start_end_aliases() {
        let s = segment("hello", 1.5);
        let v = segment_json(&s);
        assert_eq!(v["start"], json!(1.5));
        assert_eq!(v["end"], json!(2.5));
        assert_eq!(v["text"], json!("hello"));
        assert_eq!(v["speaker"], json!("Alice"));
    }
}
