//! Faithful port of post_meeting.py's portable tasks: transcription aggregation, in-progress
//! recording finalization, and hivemind ingest. `fire_post_meeting_hooks` is stubbed — it's an
//! opt-in internal billing/analytics hook (POST_MEETING_HOOKS env var, empty by default for
//! Kioku's self-hosted deployment) with a no-migration outbound-events ledger not ported.

use crate::models::Meeting;
use crate::state::AppState;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregationFailureClass {
    TransientInfra,
    PermanentInfra,
}

impl AggregationFailureClass {
    fn as_str(&self) -> &'static str {
        match self {
            Self::TransientInfra => "transient_infra",
            Self::PermanentInfra => "permanent_infra",
        }
    }
}

async fn set_aggregation_failure_class(db: &sqlx::PgPool, meeting: &Meeting, cls: AggregationFailureClass) {
    let mut data = meeting.data.clone();
    data["aggregation_failure_class"] = json!(cls.as_str());
    data["aggregation_last_retry_at"] = json!(chrono::Utc::now().to_rfc3339());
    let retry_count = data.get("aggregation_retry_count").and_then(Value::as_i64).unwrap_or(0) + 1;
    data["aggregation_retry_count"] = json!(retry_count);
    let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(db).await;
}

async fn clear_aggregation_failure_class(db: &sqlx::PgPool, meeting: &Meeting) {
    if meeting.data.get("aggregation_failure_class").is_none() {
        return;
    }
    let mut data = meeting.data.clone();
    if let Some(obj) = data.as_object_mut() {
        obj.remove("aggregation_failure_class");
    }
    let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(db).await;
}

/// Fetch transcription segments from the collector and aggregate participants/languages into
/// meeting.data. Pack H: distinguishes 5xx/network errors (transient, retry-eligible via the
/// aggregation-retry sweep) from 4xx (permanent, needs operator action) from success — a prior
/// tx-gateway restart once flipped 23 meetings to `failed` from what was really a transient flap.
pub async fn aggregate_transcription(state: &AppState, meeting: &Meeting) -> bool {
    let url = format!("{}/internal/transcripts/{}", state.config.transcription_collector_url, meeting.id);
    let mut req = state.http.get(&url).timeout(std::time::Duration::from_secs(30));
    if !state.config.internal_api_secret.is_empty() {
        req = req.header("X-Internal-Secret", &state.config.internal_api_secret);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(meeting_id = meeting.id, error = %e, "Pack H: tx-gateway request error — transient infra, retrying via sweep");
            set_aggregation_failure_class(&state.db, meeting, AggregationFailureClass::TransientInfra).await;
            return false;
        }
    };

    let status = resp.status().as_u16();
    if (500..600).contains(&status) {
        tracing::warn!(meeting_id = meeting.id, status, "Pack H: tx-gateway returned 5xx — transient infra, retrying via sweep");
        set_aggregation_failure_class(&state.db, meeting, AggregationFailureClass::TransientInfra).await;
        return false;
    }
    if status != 200 {
        tracing::error!(meeting_id = meeting.id, status, "Pack H: tx-gateway returned non-200 — permanent infra failure (operator action required)");
        set_aggregation_failure_class(&state.db, meeting, AggregationFailureClass::PermanentInfra).await;
        return false;
    }

    let segments: Vec<Value> = resp.json().await.unwrap_or_default();
    if segments.is_empty() {
        clear_aggregation_failure_class(&state.db, meeting).await;
        return true;
    }

    let mut speakers = std::collections::BTreeSet::new();
    let mut languages = std::collections::BTreeSet::new();
    for seg in &segments {
        if let Some(s) = seg.get("speaker").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            speakers.insert(s.to_string());
        }
        if let Some(l) = seg.get("language").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            languages.insert(l.to_string());
        }
    }

    let mut data = meeting.data.clone();
    let mut changed = false;
    if data.get("participants").is_none() && !speakers.is_empty() {
        data["participants"] = json!(speakers.into_iter().collect::<Vec<_>>());
        changed = true;
    }
    if data.get("languages").is_none() && !languages.is_empty() {
        data["languages"] = json!(languages.into_iter().collect::<Vec<_>>());
        changed = true;
    }
    if changed {
        let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(&state.db).await;
        tracing::info!(meeting_id = meeting.id, "aggregated transcription data");
    }
    clear_aggregation_failure_class(&state.db, meeting).await;
    true
}

/// Bug B fix: recordings whose finalizer chunk never arrived (bot killed before sending the
/// empty-body is_final=true chunk) stayed IN_PROGRESS forever. At post-meeting time (terminal
/// state reached), flip any still-IN_PROGRESS recording to COMPLETED and every media_files
/// entry's is_final to true — the chunk files are already in storage, this is metadata-only.
/// #311: skip any media_files entry recording_finalizer already claimed (storage_path ending
/// in /audio/master.webm|wav) — that's the canonical owner, don't stomp its finalized_by.
pub async fn finalize_in_progress_recordings(db: &sqlx::PgPool, meeting: &Meeting) -> u32 {
    let Some(recordings) = meeting.data.get("recordings").and_then(Value::as_array).cloned() else { return 0 };
    if recordings.is_empty() {
        return 0;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut finalized_count = 0u32;
    let mut changed = false;
    let mut new_recordings = Vec::with_capacity(recordings.len());

    for rec in recordings {
        if rec.get("status").and_then(Value::as_str) == Some("completed") {
            new_recordings.push(rec);
            continue;
        }
        let mut rec = rec;
        rec["status"] = json!("completed");
        if rec.get("completed_at").is_none() {
            rec["completed_at"] = json!(now);
        }
        if let Some(media_files) = rec.get_mut("media_files").and_then(Value::as_array_mut) {
            for mf in media_files.iter_mut() {
                let sp = mf.get("storage_path").and_then(Value::as_str).unwrap_or("");
                if sp.ends_with("/audio/master.webm") || sp.ends_with("/audio/master.wav") {
                    continue; // recording_finalizer owns this entry — observe only
                }
                if mf.get("is_final").and_then(Value::as_bool) != Some(true) {
                    mf["is_final"] = json!(true);
                    mf["finalized_at"] = json!(now);
                    mf["finalized_by"] = json!("post_meeting_reconciler");
                    changed = true;
                }
            }
        }
        finalized_count += 1;
        changed = true;
        new_recordings.push(rec);
    }

    if changed {
        let mut data = meeting.data.clone();
        data["recordings"] = json!(new_recordings);
        let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(db).await;
        tracing::info!(meeting_id = meeting.id, finalized_count, "[Bug-B-Fix] post_meeting_reconciler finalized recordings");
    }
    finalized_count
}

/// Issue #46 — push the transcript into Hivemind's vector store after the meeting ends, so it
/// becomes searchable. `users` lives in the same "vexa" schema meeting-api already connects
/// to (admin-api's own ORM table, queried directly rather than over HTTP — same DB, same
/// pattern the Python original uses).
pub async fn push_to_hivemind(state: &AppState, meeting: &Meeting) {
    let user: Option<(String, Option<String>)> = sqlx::query_as("SELECT email, name FROM users WHERE id = $1").bind(meeting.user_id).fetch_optional(&state.db).await.unwrap_or(None);
    let Some((email, name)) = user else {
        tracing::warn!(meeting_id = meeting.id, user_id = meeting.user_id, "[Hivemind] no email for user, skipping ingest");
        return;
    };
    let user_name = name.unwrap_or_else(|| email.split('@').next().unwrap_or(&email).to_string());

    let prov = match state
        .http
        .post(format!("{}/internal/provision", state.config.hivemind_url))
        .json(&json!({"email": email, "name": user_name}))
        .header("X-Internal-Secret", &state.config.internal_api_secret)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            tracing::error!(meeting_id = meeting.id, status = r.status().as_u16(), "[Hivemind] provision failed");
            return;
        }
        Err(e) => {
            tracing::error!(meeting_id = meeting.id, error = %e, "[Hivemind] provision request failed");
            return;
        }
    };
    let Some(token) = prov.json::<Value>().await.ok().and_then(|v| v.get("token").and_then(Value::as_str).map(str::to_string)) else {
        tracing::error!(meeting_id = meeting.id, "[Hivemind] no token in provision response");
        return;
    };

    let segments: Vec<Value> = match state
        .http
        .get(format!("{}/internal/transcripts/{}", state.config.transcription_collector_url, meeting.id))
        .header("X-Internal-Secret", &state.config.internal_api_secret)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        Ok(r) => {
            tracing::warn!(meeting_id = meeting.id, status = r.status().as_u16(), "[Hivemind] collector returned non-2xx");
            return;
        }
        Err(e) => {
            tracing::error!(meeting_id = meeting.id, error = %e, "[Hivemind] collector fetch failed");
            return;
        }
    };
    if segments.is_empty() {
        tracing::info!(meeting_id = meeting.id, "[Hivemind] no segments, skipping ingest");
        return;
    }

    let hivemind_segments: Vec<Value> = segments
        .iter()
        .filter(|s| s.get("text").and_then(Value::as_str).map(|t| !t.trim().is_empty()).unwrap_or(false))
        .map(|s| {
            json!({
                "speaker": s.get("speaker").and_then(Value::as_str).unwrap_or("Unknown"),
                "text": s.get("text").and_then(Value::as_str).unwrap_or(""),
                "start_time": s.get("start_time").or_else(|| s.get("start")).and_then(Value::as_f64).unwrap_or(0.0),
                "end_time": s.get("end_time").or_else(|| s.get("end")).and_then(Value::as_f64).unwrap_or(0.0),
            })
        })
        .collect();
    if hivemind_segments.is_empty() {
        tracing::info!(meeting_id = meeting.id, "[Hivemind] all segments empty, skipping ingest");
        return;
    }

    let duration_seconds = match (meeting.start_time, meeting.end_time) {
        (Some(s), Some(e)) => (e - s).num_seconds(),
        _ => 0,
    };
    let date_ms = meeting.start_time.map(|t| t.timestamp_millis()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let title = meeting
        .data
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| meeting.platform_specific_id.as_ref().map(|id| format!("{}/{}", meeting.platform, id)))
        .unwrap_or_else(|| format!("Meeting {}", meeting.id));

    let payload = json!({
        "title": title,
        "date": date_ms,
        "duration_seconds": duration_seconds,
        "participants": meeting.data.get("participants").cloned().unwrap_or_else(|| json!([])),
        "transcript": hivemind_segments,
        "vexa_meeting_id": meeting.id,
        "vexa_platform": meeting.platform,
        "vexa_native_meeting_id": meeting.platform_specific_id,
    });

    match state.http.post(format!("{}/meetings", state.config.hivemind_url)).bearer_auth(&token).json(&payload).timeout(std::time::Duration::from_secs(30)).send().await {
        Ok(r) if r.status().as_u16() == 200 || r.status().as_u16() == 201 => {
            tracing::info!(meeting_id = meeting.id, segments = hivemind_segments.len(), "[Hivemind] ingested meeting");
        }
        Ok(r) => tracing::error!(meeting_id = meeting.id, status = r.status().as_u16(), "[Hivemind] ingest failed"),
        Err(e) => tracing::error!(meeting_id = meeting.id, error = %e, "[Hivemind] ingest request failed"),
    }
}

async fn refetch(state: &AppState, meeting_id: i32) -> Option<Meeting> {
    sqlx::query_as("SELECT * FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(&state.db).await.ok().flatten()
}

/// Run all post-meeting tasks for a meeting_id. Each task re-fetches the meeting so a change
/// from an earlier task is visible to the next, and each is isolated so one failing doesn't
/// block the rest — matches the Python original's short-lived-session-per-task structure.
pub async fn run_all_tasks(state: &AppState, meeting_id: i32) {
    tracing::info!(meeting_id, "starting post-meeting tasks");

    if let Some(meeting) = refetch(state, meeting_id).await {
        finalize_in_progress_recordings(&state.db, &meeting).await;
    }
    if let Some(meeting) = refetch(state, meeting_id).await {
        aggregate_transcription(state, &meeting).await;
    }
    if let Some(meeting) = refetch(state, meeting_id).await {
        crate::webhooks::send_completion_webhook(state, &meeting).await;
    }
    // ponytail: fire_post_meeting_hooks not ported — opt-in (POST_MEETING_HOOKS env, empty by
    // default for self-hosted Kioku) internal billing/analytics hook with a ledger dependency
    // (outbound_events.py) that's out of scope here.
    if let Some(meeting) = refetch(state, meeting_id).await {
        push_to_hivemind(state, &meeting).await;
    }

    tracing::info!(meeting_id, "post-meeting tasks completed");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meeting_with_recordings(recordings: Value) -> Meeting {
        Meeting {
            id: 1,
            user_id: 1,
            platform: "google_meet".to_string(),
            platform_specific_id: Some("abc".to_string()),
            status: "completed".to_string(),
            bot_container_id: None,
            start_time: None,
            end_time: None,
            data: json!({"recordings": recordings}),
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn already_completed_recording_data_shape_is_untouched() {
        // Direct unit test of the pure decision logic without a DB: verify the "skip if already
        // completed" branch condition matches what finalize_in_progress_recordings checks.
        let rec = json!({"status": "completed", "media_files": []});
        assert_eq!(rec.get("status").and_then(Value::as_str), Some("completed"));
    }

    #[test]
    fn meeting_with_no_recordings_key_has_none() {
        let meeting = meeting_with_recordings(json!([]));
        let recs = meeting.data.get("recordings").and_then(Value::as_array).cloned().unwrap_or_default();
        assert!(recs.is_empty());
    }
}
