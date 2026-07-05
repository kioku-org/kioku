use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// meetings.{start_time,end_time,created_at,updated_at} are `TIMESTAMP` (no timezone) in Postgres
// — always UTC by convention, never stored with an offset — so these are NaiveDateTime, not
// DateTime<Utc>. sqlx's Postgres decoder is strict about this: TIMESTAMP only decodes into
// NaiveDateTime, TIMESTAMPTZ only into DateTime<Tz>. Getting this wrong doesn't fail to compile
// (both are just chrono types) — it fails at runtime, at decode time, on every row that isn't
// empty. Call sites needing a real DateTime<Utc> (rfc3339 formatting, Unix timestamps, passing to
// code that takes DateTime<Utc>) do `.and_utc()`, the zero-cost "this naive value is UTC" cast.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Meeting {
    pub id: i32,
    pub user_id: i32,
    pub platform: String,
    pub platform_specific_id: Option<String>,
    pub status: String,
    pub bot_container_id: Option<String>,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
    pub data: Value,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

impl Meeting {
    pub fn native_meeting_id(&self) -> Option<&str> {
        self.platform_specific_id.as_deref()
    }

    /// Which runtime backend ("local" | "runpod") this meeting's container was spawned on.
    /// Replaces the old `router` service's in-memory `_bot_backends` map — this survives
    /// restarts since it's the same JSONB column every other piece of meeting metadata lives in.
    pub fn runtime_backend(&self) -> Option<&str> {
        self.data.get("runtime_backend").and_then(|v| v.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transcription {
    pub id: i32,
    pub meeting_id: i32,
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
    pub speaker: Option<String>,
    pub language: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub session_uid: Option<String>,
    pub segment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeetingSession {
    pub id: i32,
    pub meeting_id: i32,
    pub session_uid: String,
    pub session_start_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Recording {
    pub id: i32,
    pub meeting_id: Option<i32>,
    pub user_id: i32,
    pub session_uid: Option<String>,
    pub source: String,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MediaFile {
    pub id: i32,
    pub recording_id: i32,
    #[sqlx(rename = "type")]
    pub media_type: String,
    pub format: String,
    pub storage_path: String,
    pub storage_backend: String,
    pub file_size_bytes: Option<i32>,
    pub duration_seconds: Option<f64>,
    pub metadata: Value,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CalendarEvent {
    pub id: i32,
    pub user_id: i32,
    pub external_event_id: String,
    pub title: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub meeting_url: Option<String>,
    pub platform: Option<String>,
    pub status: String,
    pub meeting_id: Option<i32>,
    pub sync_token: Option<String>,
    pub created_at: Option<NaiveDateTime>,
}
