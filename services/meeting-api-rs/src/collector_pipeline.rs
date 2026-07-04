//! Real-time transcription ingestion pipeline. Faithful port of collector/{processors,
//! consumer,db_writer}.py.
//!
//! Architecture (confirmed by reading the actual call graph, not just the file list):
//! bot -> Redis Stream `transcription_segments` -> [this consumer] -> Redis Hash
//! `meeting:{id}:segments` (mutable, until a segment goes 30s+ without an update) ->
//! [db_writer background loop, every 10s] -> Postgres `transcriptions` table (UPSERT by
//! (meeting_id, segment_id)).
//!
//! ponytail: collector/filters.py (`TranscriptionFilter`) and collector/speaker_mapper.py are
//! NOT ported — confirmed dead code in the Python original. `TranscriptionFilter` is imported
//! in endpoints.py but never instantiated or called; speaker_mapper.py isn't imported by
//! anything at all. The live path uses producer-labeled speaker info straight from the bot, no
//! filtering step. Porting unreachable code would add complexity with no behavioral parity to
//! preserve.

use crate::state::AppState;
use base64::Engine;
use hmac::{Hmac, Mac};
use redis::AsyncCommands;
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::time::Duration;

fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s).ok()
}

fn b64url_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

#[derive(Debug, Clone)]
pub struct MeetingTokenClaims {
    pub meeting_id: i32,
    pub platform: Option<String>,
    pub native_meeting_id: Option<String>,
}

/// Verify a MeetingToken (HS256, header.payload.signature, base64url) minted by
/// meetings.rs's `mint_meeting_token`. Internal Redis stream messages without a token are
/// trusted (same-network producer); this only gates messages that *do* carry one.
pub fn verify_meeting_token(secret: &str, token: &str) -> Option<MeetingTokenClaims> {
    if token.is_empty() || secret.is_empty() {
        return None;
    }
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let [header_b64, payload_b64, signature_b64] = [parts[0], parts[1], parts[2]];

    let header: Value = serde_json::from_slice(&b64url_decode(header_b64)?).ok()?;
    let payload: Value = serde_json::from_slice(&b64url_decode(payload_b64)?).ok()?;
    if header.get("alg").and_then(Value::as_str) != Some("HS256") || header.get("typ").and_then(Value::as_str) != Some("JWT") {
        return None;
    }

    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(signing_input.as_bytes());
    let expected_sig = mac.finalize().into_bytes();
    let expected_b64 = b64url_encode(&expected_sig);
    if !constant_time_eq(&expected_b64, signature_b64) {
        return None;
    }

    if let Some(exp) = payload.get("exp").and_then(Value::as_i64) {
        if exp < chrono::Utc::now().timestamp() {
            return None;
        }
    }
    if payload.get("aud").and_then(Value::as_str) != Some("transcription-collector") || payload.get("iss").and_then(Value::as_str) != Some("meeting-api") {
        return None;
    }
    if payload.get("scope").and_then(Value::as_str) != Some("transcribe:write") {
        return None;
    }
    let meeting_id = payload.get("meeting_id").and_then(Value::as_i64)? as i32;

    Some(MeetingTokenClaims {
        meeting_id,
        platform: payload.get("platform").and_then(Value::as_str).map(str::to_string),
        native_meeting_id: payload.get("native_meeting_id").and_then(Value::as_str).map(str::to_string),
    })
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ---------------------------------------------------------------------------
// Message processing
// ---------------------------------------------------------------------------

async fn process_session_start_event(state: &AppState, stream_data: &Value, meeting_id: i32) -> bool {
    let (Some(session_uid), Some(start_ts_str)) = (stream_data.get("uid").and_then(Value::as_str), stream_data.get("start_timestamp").and_then(Value::as_str)) else {
        return true; // missing required fields — bad data, OK to ack
    };
    let cleaned = start_ts_str.trim_end_matches('Z');
    let Ok(start_timestamp) = chrono::NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%dT%H:%M:%S%.f").or_else(|_| chrono::NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%dT%H:%M:%S")) else {
        return true; // bad timestamp format — bad data, OK to ack
    };
    let start_timestamp = start_timestamp.and_utc();

    let existing: Option<(i32,)> = sqlx::query_as("SELECT id FROM meeting_sessions WHERE meeting_id = $1 AND session_uid = $2").bind(meeting_id).bind(session_uid).fetch_optional(&state.db).await.unwrap_or(None);
    let result = if existing.is_some() {
        sqlx::query("UPDATE meeting_sessions SET session_start_time = $1 WHERE meeting_id = $2 AND session_uid = $3").bind(start_timestamp).bind(meeting_id).bind(session_uid).execute(&state.db).await
    } else {
        sqlx::query("INSERT INTO meeting_sessions (meeting_id, session_uid, session_start_time) VALUES ($1, $2, $3)").bind(meeting_id).bind(session_uid).bind(start_timestamp).execute(&state.db).await
    };
    if let Err(e) = result {
        tracing::error!(meeting_id, session_uid, error = %e, "process_session_start_event: DB write failed");
        return false; // unexpected DB error — do not ack, allow retry
    }

    let mut redis = state.redis.clone();
    let cache_key = format!("meeting_session:{session_uid}:start");
    let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut redis, &cache_key, start_timestamp.to_rfc3339(), 7200).await;

    tracing::info!(meeting_id, session_uid, "processed session_start event");
    true
}

async fn process_transcript_bundle(state: &AppState, stream_data: &Value, meeting_id: i32) -> bool {
    let speaker = stream_data.get("speaker").and_then(Value::as_str).unwrap_or("");
    let empty = vec![];
    let confirmed_segs = stream_data.get("confirmed").and_then(Value::as_array).unwrap_or(&empty);
    let pending_segs = stream_data.get("pending").and_then(Value::as_array).unwrap_or(&empty);
    let session_uid = stream_data.get("uid").and_then(Value::as_str);
    let hash_key = format!("meeting:{meeting_id}:segments");
    let now_iso = chrono::Utc::now().to_rfc3339();

    let mut redis = state.redis.clone();
    if !confirmed_segs.is_empty() {
        let _: Result<(), _> = redis::AsyncCommands::sadd(&mut redis, "active_meetings", meeting_id.to_string()).await;
        let _: Result<(), _> = redis::AsyncCommands::expire(&mut redis, &hash_key, state.config.redis_segment_ttl).await;
        for seg in confirmed_segs {
            let Some(seg_id) = seg.get("segment_id").and_then(Value::as_str) else { continue };
            let text = seg.get("text").and_then(Value::as_str).unwrap_or("");
            if text.trim().is_empty() {
                continue;
            }
            let mut redis_data = json!({
                "text": text,
                "start_time": seg.get("start").cloned().unwrap_or(json!(0)),
                "end_time": seg.get("end").cloned().unwrap_or(json!(0)),
                "language": seg.get("language"),
                "completed": true,
                "updated_at": now_iso,
                "session_uid": session_uid,
                "speaker": seg.get("speaker").and_then(Value::as_str).unwrap_or(speaker),
                "speaker_mapping_status": "PRODUCER_LABELED",
                "segment_id": seg_id,
            });
            if let Some(v) = seg.get("absolute_start_time") {
                redis_data["absolute_start_time"] = v.clone();
            }
            if let Some(v) = seg.get("absolute_end_time") {
                redis_data["absolute_end_time"] = v.clone();
            }
            let _: Result<(), _> = redis::AsyncCommands::hset(&mut redis, &hash_key, seg_id, redis_data.to_string()).await;
        }
        tracing::info!(meeting_id, speaker, count = confirmed_segs.len(), "[Transcript] stored confirmed segments");
    }

    let pending_key = format!("meeting:{meeting_id}:pending:{speaker}");
    if !pending_segs.is_empty() {
        let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut redis, &pending_key, Value::Array(pending_segs.clone()).to_string(), 60).await;
    } else {
        let _: Result<(), _> = redis::AsyncCommands::del(&mut redis, &pending_key).await;
    }
    true
}

async fn process_speaker_event_message(state: &AppState, event_data: &HashMap<String, String>) -> bool {
    const REQUIRED: &[&str] = &["uid", "relative_client_timestamp_ms", "event_type", "participant_name"];
    if !REQUIRED.iter().all(|f| event_data.contains_key(*f)) {
        return true; // missing fields — bad data, OK to ack
    }
    let session_uid = &event_data["uid"];
    let Ok(relative_ts) = event_data["relative_client_timestamp_ms"].parse::<f64>() else {
        return true; // bad data, OK to ack
    };

    let event_json: Value = event_data.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>().into();
    let sorted_set_key = format!("{}:{session_uid}", state.config.redis_speaker_event_key_prefix);

    let mut redis = state.redis.clone();
    if redis::AsyncCommands::zadd::<_, _, _, ()>(&mut redis, &sorted_set_key, event_json.to_string(), relative_ts).await.is_err() {
        return false; // Redis error — retryable, do not ack
    }
    let _: Result<(), _> = redis::AsyncCommands::expire(&mut redis, &sorted_set_key, state.config.redis_speaker_event_ttl).await;
    true
}

/// Returns true if the message should be ACKed (processing complete, whether success or a
/// permanent/bad-data failure); false if a transient error occurred and the message should be
/// redelivered.
async fn process_stream_message(state: &AppState, fields: &HashMap<String, String>) -> bool {
    let Some(payload_json) = fields.get("payload") else {
        return true; // missing payload — bad data, OK to ack
    };
    let Ok(stream_data) = serde_json::from_str::<Value>(payload_json) else {
        tracing::error!("failed to parse JSON payload — acking to avoid loop");
        return true;
    };
    let message_type = stream_data.get("type").and_then(Value::as_str).unwrap_or("transcription");

    let token = stream_data.get("token").and_then(Value::as_str).unwrap_or("");
    let (meeting_id, _platform, _native_id) = if !token.is_empty() {
        match verify_meeting_token(&state.config.admin_token, token) {
            Some(claims) => (claims.meeting_id, claims.platform, claims.native_meeting_id),
            None => {
                tracing::warn!(message_type, "message failed MeetingToken verification — skipping");
                return true;
            }
        }
    } else {
        let Some(raw_mid) = stream_data.get("meeting_id").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))) else {
            tracing::warn!(message_type, "no token and no meeting_id — skipping");
            return true;
        };
        (raw_mid as i32, stream_data.get("platform").and_then(Value::as_str).map(str::to_string), stream_data.get("native_meeting_id").and_then(Value::as_str).map(str::to_string))
    };

    match message_type {
        "session_start" => {
            let exists: Option<(i32,)> = sqlx::query_as("SELECT id FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(&state.db).await.ok().flatten();
            if exists.is_none() {
                tracing::warn!(meeting_id, "session_start for unknown meeting — skipping");
                return true;
            }
            process_session_start_event(state, &stream_data, meeting_id).await
        }
        "transcript" => process_transcript_bundle(state, &stream_data, meeting_id).await,
        "session_end" => {
            let Some(session_uid) = stream_data.get("uid").and_then(Value::as_str) else {
                return true;
            };
            let mut redis = state.redis.clone();
            let speaker_event_key = format!("{}:{session_uid}", state.config.redis_speaker_event_key_prefix);
            let session_start_cache_key = format!("meeting_session:{session_uid}:start");
            match redis::AsyncCommands::del::<_, i64>(&mut redis, (speaker_event_key, session_start_cache_key)).await {
                Ok(_) => {
                    tracing::info!(session_uid, "processed session_end cleanup");
                    true
                }
                Err(e) => {
                    tracing::error!(session_uid, error = %e, "redis error on session_end cleanup");
                    false
                }
            }
        }
        "transcription" => process_legacy_transcription(state, &stream_data, meeting_id).await,
        other => {
            tracing::warn!(message_type = other, "unknown message type — skipping");
            true
        }
    }
}

async fn process_legacy_transcription(state: &AppState, stream_data: &Value, meeting_id: i32) -> bool {
    let Some(segments) = stream_data.get("segments").and_then(Value::as_array) else {
        return true; // missing 'segments' — bad data, OK to ack
    };
    let session_uid = stream_data.get("uid").and_then(Value::as_str);
    let hash_key = format!("meeting:{meeting_id}:segments");
    let mut to_store: Vec<(String, String)> = vec![];

    for segment in segments {
        let (Some(start), Some(end)) = (segment.get("start").and_then(Value::as_f64), segment.get("end").and_then(Value::as_f64)) else {
            continue;
        };
        let (mut start, mut end) = (start, end);
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        if end - start < 1e-3 {
            continue;
        }
        let Some(segment_id) = segment.get("segment_id").and_then(Value::as_str) else {
            tracing::error!(meeting_id, "segment missing segment_id — skipping");
            continue;
        };

        let mut redis_data = json!({
            "text": segment.get("text").and_then(Value::as_str).unwrap_or(""),
            "start_time": start,
            "end_time": end,
            "language": segment.get("language"),
            "completed": segment.get("completed").and_then(Value::as_bool).unwrap_or(false),
            "updated_at": chrono::Utc::now().to_rfc3339(),
            "session_uid": session_uid,
            "speaker": segment.get("speaker"),
            "speaker_mapping_status": "PRODUCER_LABELED",
            "segment_id": segment_id,
        });
        if let Some(v) = segment.get("absolute_start_time") {
            redis_data["absolute_start_time"] = v.clone();
        }
        if let Some(v) = segment.get("absolute_end_time") {
            redis_data["absolute_end_time"] = v.clone();
        }
        to_store.push((segment_id.to_string(), redis_data.to_string()));
    }

    if to_store.is_empty() {
        return true;
    }

    let mut redis = state.redis.clone();
    let mut pipe = redis::pipe();
    pipe.atomic();
    pipe.cmd("SADD").arg("active_meetings").arg(meeting_id.to_string()).ignore();
    pipe.cmd("EXPIRE").arg(&hash_key).arg(state.config.redis_segment_ttl).ignore();
    let mut hset_cmd = redis::cmd("HSET");
    hset_cmd.arg(&hash_key);
    for (k, v) in &to_store {
        hset_cmd.arg(k).arg(v);
    }
    pipe.add_command(hset_cmd).ignore();

    match pipe.query_async::<()>(&mut redis).await {
        Ok(_) => {
            tracing::info!(meeting_id, count = to_store.len(), "stored segments in Redis");
            true
        }
        Err(e) => {
            tracing::error!(meeting_id, error = %e, "redis pipeline error storing segments");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Stream consumers
// ---------------------------------------------------------------------------

fn decode_stream_fields(raw: HashMap<String, redis::Value>) -> HashMap<String, String> {
    raw.into_iter()
        .filter_map(|(k, v)| match v {
            redis::Value::BulkString(bytes) => Some((k, String::from_utf8_lossy(&bytes).to_string())),
            redis::Value::SimpleString(s) => Some((k, s)),
            _ => None,
        })
        .collect()
}

async fn ensure_group(redis: &mut redis::aio::ConnectionManager, stream: &str, group: &str) {
    let result: redis::RedisResult<()> = redis::cmd("XGROUP").arg("CREATE").arg(stream).arg(group).arg("0").arg("MKSTREAM").query_async(redis).await;
    match result {
        Ok(_) => tracing::info!(stream, group, "consumer group ensured"),
        Err(e) if e.to_string().contains("BUSYGROUP") => tracing::debug!(stream, group, "consumer group already exists"),
        Err(e) => tracing::error!(stream, group, error = %e, "failed to create consumer group"),
    }
}

/// Claims and reprocesses messages that have been pending (unacked) longer than
/// PENDING_MSG_TIMEOUT_MS — recovers work from a crashed/restarted consumer.
async fn claim_stale_messages(state: &AppState) {
    let mut redis = state.redis.clone();
    let stream = state.config.redis_stream_name.clone();
    let group = state.config.redis_consumer_group.clone();

    loop {
        let pending: redis::streams::StreamPendingCountReply = match redis::cmd("XPENDING")
            .arg(&stream)
            .arg(&group)
            .arg("-")
            .arg("+")
            .arg(100)
            .query_async(&mut redis)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, "XPENDING failed during stale message check");
                return;
            }
        };
        if pending.ids.is_empty() {
            return;
        }

        let stale_ids: Vec<String> = pending.ids.iter().filter(|m| m.last_delivered_ms >= state.config.pending_msg_timeout_ms as usize).map(|m| m.id.clone()).collect();
        let total_checked = pending.ids.len();
        if stale_ids.is_empty() {
            return;
        }

        let mut claim_cmd = redis::cmd("XCLAIM");
        claim_cmd.arg(&stream).arg(&group).arg(&state.config.consumer_name).arg(state.config.pending_msg_timeout_ms);
        for id in &stale_ids {
            claim_cmd.arg(id);
        }
        let claimed: Vec<(String, Option<HashMap<String, redis::Value>>)> = claim_cmd.query_async(&mut redis).await.unwrap_or_default();

        for (message_id, fields) in claimed {
            let Some(raw_fields) = fields else { continue };
            let decoded = decode_stream_fields(raw_fields);
            let ack = process_stream_message(state, &decoded).await;
            if ack {
                let _: Result<(), _> = redis::AsyncCommands::xack(&mut redis, &stream, &group, &[message_id]).await;
            }
        }

        if total_checked < 100 {
            return;
        }
    }
}

/// Main consumer loop: reads new transcription-segment messages, processes them, ACKs success.
pub async fn consume_redis_stream(state: AppState) {
    ensure_group(&mut state.redis.clone(), &state.config.redis_stream_name, &state.config.redis_consumer_group).await;
    claim_stale_messages(&state).await;

    let mut redis = state.redis.clone();
    let opts = redis::streams::StreamReadOptions::default().group(&state.config.redis_consumer_group, &state.config.consumer_name).count(state.config.redis_stream_read_count).block(state.config.redis_stream_block_ms);

    loop {
        let reply: redis::RedisResult<redis::streams::StreamReadReply> = redis.xread_options(&[&state.config.redis_stream_name], &[">"], &opts).await;
        let reply = match reply {
            Ok(r) => r,
            Err(e) if e.to_string().contains("NOGROUP") => {
                tracing::warn!("NOGROUP — recreating consumer group");
                ensure_group(&mut redis, &state.config.redis_stream_name, &state.config.redis_consumer_group).await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            Err(e) => {
                tracing::error!(error = %e, "redis stream consumer error — retrying after delay");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        for stream_key in &reply.keys {
            let mut to_ack = vec![];
            for id in &stream_key.ids {
                let fields = decode_stream_fields(id.map.clone());
                if process_stream_message(&state, &fields).await {
                    to_ack.push(id.id.clone());
                }
            }
            if !to_ack.is_empty() {
                let _: Result<(), _> = redis::AsyncCommands::xack(&mut redis, &state.config.redis_stream_name, &state.config.redis_consumer_group, &to_ack).await;
            }
        }
    }
}

/// Speaker-events consumer loop — same pattern, separate stream/group.
pub async fn consume_speaker_events_stream(state: AppState) {
    ensure_group(&mut state.redis.clone(), &state.config.redis_speaker_events_stream_name, &state.config.redis_speaker_events_consumer_group).await;

    let mut redis = state.redis.clone();
    let consumer_name = format!("{}-speaker", state.config.consumer_name);
    let opts = redis::streams::StreamReadOptions::default().group(&state.config.redis_speaker_events_consumer_group, &consumer_name).count(state.config.redis_stream_read_count).block(state.config.redis_stream_block_ms);

    loop {
        let reply: redis::RedisResult<redis::streams::StreamReadReply> = redis.xread_options(&[&state.config.redis_speaker_events_stream_name], &[">"], &opts).await;
        let reply = match reply {
            Ok(r) => r,
            Err(e) if e.to_string().contains("NOGROUP") => {
                tracing::warn!("[SpeakerConsumer] NOGROUP — recreating consumer group");
                ensure_group(&mut redis, &state.config.redis_speaker_events_stream_name, &state.config.redis_speaker_events_consumer_group).await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            Err(e) => {
                tracing::error!(error = %e, "[SpeakerConsumer] error — retrying after delay");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        for stream_key in &reply.keys {
            let mut to_ack = vec![];
            for id in &stream_key.ids {
                let fields = decode_stream_fields(id.map.clone());
                if process_speaker_event_message(&state, &fields).await {
                    to_ack.push(id.id.clone());
                }
            }
            if !to_ack.is_empty() {
                let _: Result<(), _> = redis::AsyncCommands::xack(&mut redis, &state.config.redis_speaker_events_stream_name, &state.config.redis_speaker_events_consumer_group, &to_ack).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// db_writer: periodic Redis-hash -> Postgres flush
// ---------------------------------------------------------------------------

/// Background loop: every BACKGROUND_TASK_INTERVAL seconds, move segments that haven't been
/// updated in IMMUTABILITY_THRESHOLD seconds (the bot considers them final) from the per-meeting
/// Redis hash into Postgres, UPSERTing by (meeting_id, segment_id).
pub async fn process_redis_to_postgres(state: AppState) {
    tracing::info!("background Redis-to-PostgreSQL processor started");
    loop {
        tokio::time::sleep(Duration::from_secs(state.config.background_task_interval_secs)).await;

        let mut redis = state.redis.clone();
        let meeting_ids: Vec<String> = match redis::AsyncCommands::smembers(&mut redis, "active_meetings").await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "failed to read active_meetings");
                continue;
            }
        };
        if meeting_ids.is_empty() {
            continue;
        }

        let immutability_cutoff = chrono::Utc::now() - chrono::Duration::seconds(state.config.immutability_threshold_secs);
        let mut stored_count = 0usize;

        for meeting_id_str in meeting_ids {
            let Ok(meeting_id) = meeting_id_str.parse::<i32>() else { continue };
            let hash_key = format!("meeting:{meeting_id}:segments");
            let segments: HashMap<String, String> = redis::AsyncCommands::hgetall(&mut redis, &hash_key).await.unwrap_or_default();

            if segments.is_empty() {
                let _: Result<(), _> = redis::AsyncCommands::srem(&mut redis, "active_meetings", &meeting_id_str).await;
                continue;
            }

            let mut to_delete: Vec<String> = vec![];
            for (seg_key, segment_json) in &segments {
                let Ok(segment_data) = serde_json::from_str::<Value>(segment_json) else {
                    to_delete.push(seg_key.clone());
                    continue;
                };
                let Some(updated_at_str) = segment_data.get("updated_at").and_then(Value::as_str) else {
                    continue; // no updated_at yet — leave in place
                };
                let Ok(updated_at) = chrono::DateTime::parse_from_rfc3339(updated_at_str) else {
                    to_delete.push(seg_key.clone());
                    continue;
                };
                if updated_at.with_timezone(&chrono::Utc) >= immutability_cutoff {
                    continue; // still mutable — bot may update it again
                }

                let mut start = segment_data.get("start_time").and_then(Value::as_f64).unwrap_or(0.0);
                let mut end = segment_data.get("end_time").and_then(Value::as_f64).unwrap_or(0.0);
                if end < start {
                    std::mem::swap(&mut start, &mut end);
                }
                let text = segment_data.get("text").and_then(Value::as_str).unwrap_or("");
                if text.trim().is_empty() {
                    to_delete.push(seg_key.clone());
                    continue;
                }

                let segment_id = segment_data.get("segment_id").and_then(Value::as_str);
                let language = segment_data.get("language").and_then(Value::as_str);
                let session_uid = segment_data.get("session_uid").and_then(Value::as_str);
                let speaker = segment_data.get("speaker").and_then(Value::as_str);

                let write_result = if let Some(segid) = segment_id {
                    sqlx::query(
                        "INSERT INTO transcriptions (meeting_id, start_time, end_time, text, speaker, language, session_uid, segment_id, created_at) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now()) \
                         ON CONFLICT (meeting_id, segment_id) WHERE segment_id IS NOT NULL \
                         DO UPDATE SET text = $4, speaker = $5, end_time = $3, created_at = now()",
                    )
                    .bind(meeting_id)
                    .bind(start)
                    .bind(end)
                    .bind(text)
                    .bind(speaker)
                    .bind(language)
                    .bind(session_uid)
                    .bind(segid)
                    .execute(&state.db)
                    .await
                } else {
                    // Legacy segments without segment_id — plain insert, no dedup key.
                    sqlx::query("INSERT INTO transcriptions (meeting_id, start_time, end_time, text, speaker, language, session_uid, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, now())")
                        .bind(meeting_id)
                        .bind(start)
                        .bind(end)
                        .bind(text)
                        .bind(speaker)
                        .bind(language)
                        .bind(session_uid)
                        .execute(&state.db)
                        .await
                };

                match write_result {
                    Ok(_) => {
                        stored_count += 1;
                        to_delete.push(seg_key.clone());
                    }
                    Err(e) => tracing::error!(meeting_id, seg_key, error = %e, "failed to write transcription segment"),
                }
            }

            if !to_delete.is_empty() {
                let _: Result<(), _> = redis::AsyncCommands::hdel(&mut redis, &hash_key, to_delete).await;
            }
        }

        if stored_count > 0 {
            tracing::info!(stored_count, "stored segments to PostgreSQL");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mint_test_token(secret: &str, claims: &Value) -> String {
        let header = json!({"alg": "HS256", "typ": "JWT"});
        let header_b64 = b64url_encode(&serde_json::to_vec(&header).unwrap());
        let payload_b64 = b64url_encode(&serde_json::to_vec(claims).unwrap());
        let signing_input = format!("{header_b64}.{payload_b64}");
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signing_input.as_bytes());
        let sig_b64 = b64url_encode(&mac.finalize().into_bytes());
        format!("{header_b64}.{payload_b64}.{sig_b64}")
    }

    #[test]
    fn valid_token_round_trips() {
        let secret = "test-secret";
        let exp = chrono::Utc::now().timestamp() + 3600;
        let claims = json!({"meeting_id": 42, "aud": "transcription-collector", "iss": "meeting-api", "scope": "transcribe:write", "exp": exp, "platform": "google_meet"});
        let token = mint_test_token(secret, &claims);
        let result = verify_meeting_token(secret, &token).expect("should verify");
        assert_eq!(result.meeting_id, 42);
        assert_eq!(result.platform.as_deref(), Some("google_meet"));
    }

    #[test]
    fn expired_token_is_rejected() {
        let secret = "test-secret";
        let exp = chrono::Utc::now().timestamp() - 10;
        let claims = json!({"meeting_id": 1, "aud": "transcription-collector", "iss": "meeting-api", "scope": "transcribe:write", "exp": exp});
        let token = mint_test_token(secret, &claims);
        assert!(verify_meeting_token(secret, &token).is_none());
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let exp = chrono::Utc::now().timestamp() + 3600;
        let claims = json!({"meeting_id": 1, "aud": "transcription-collector", "iss": "meeting-api", "scope": "transcribe:write", "exp": exp});
        let token = mint_test_token("real-secret", &claims);
        assert!(verify_meeting_token("wrong-secret", &token).is_none());
    }

    #[test]
    fn wrong_audience_is_rejected() {
        let secret = "test-secret";
        let exp = chrono::Utc::now().timestamp() + 3600;
        let claims = json!({"meeting_id": 1, "aud": "someone-else", "iss": "meeting-api", "scope": "transcribe:write", "exp": exp});
        let token = mint_test_token(secret, &claims);
        assert!(verify_meeting_token(secret, &token).is_none());
    }

    #[test]
    fn malformed_token_is_rejected() {
        assert!(verify_meeting_token("secret", "not.a.validtoken").is_none());
        assert!(verify_meeting_token("secret", "only-one-part").is_none());
        assert!(verify_meeting_token("secret", "").is_none());
    }
}
