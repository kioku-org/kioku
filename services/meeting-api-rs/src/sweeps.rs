//! Periodic reconciliation sweeps — catches state-machine rows that escaped the canonical
//! durable mechanisms (Pack J's exit-callback, the stop outbox). Faithful port of the
//! stale-stopping sweep from sweeps.py; the other two sweeps (aggregation retry, unfinalized-
//! recordings repair) are stubbed pending post_meeting.rs / recording_finalizer.rs.

use crate::classifier::classify_stopped_exit;
use crate::meeting_status::{publish_meeting_status_change, update_meeting_status, StatusUpdateOptions};
use crate::models::Meeting;
use crate::schemas::MeetingCompletionReason;
use crate::state::AppState;
use serde_json::{json, Value};

const STALE_STOPPING_THRESHOLD_SECONDS: i64 = 300;

/// Force-finalizes meetings stuck in `stopping` for 5+ minutes — the canonical exit-callback
/// path is expected to resolve these in well under 90s, so a row this old means that path
/// genuinely failed somewhere and needs the same Pack J classification the callback would
/// have applied. #313: staleness is computed from the immutable `status_transition` history,
/// not `updated_at` (which webhook retries bump, masking genuinely-stuck rows).
pub async fn sweep_stale_stopping(state: &AppState) -> u32 {
    let threshold = chrono::Utc::now() - chrono::Duration::seconds(STALE_STOPPING_THRESHOLD_SECONDS);
    let candidates: Vec<Meeting> = match sqlx::query_as("SELECT * FROM meetings WHERE status = 'stopping' AND created_at < $1 LIMIT 200").bind(threshold).fetch_all(&state.db).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "[sweep] stale-stopping query failed");
            return 0;
        }
    };

    let mut swept = 0u32;
    for meeting in candidates {
        let last_progress_at = last_progress_timestamp(&meeting);
        if last_progress_at >= threshold {
            continue;
        }
        let stuck_for = (chrono::Utc::now() - last_progress_at).num_seconds();
        tracing::warn!(meeting_id = meeting.id, stuck_for, "[sweep] meeting stuck stopping — finalizing via stale-stopping sweep (canonical exit-callback path appears to have failed)");

        let (target_status, classified_reason) = classify_stopped_exit(&state.db, meeting.id, &meeting.data, meeting.start_time, meeting.end_time, MeetingCompletionReason::Stopped).await;
        let meta = json!({"sweep_source": "stale_stopping_sweep", "stuck_for_seconds": stuck_for, "pack_j_classification": classified_reason.as_str()});
        let success = update_meeting_status(
            &state.db,
            meeting.id,
            target_status,
            StatusUpdateOptions { completion_reason: Some(classified_reason), transition_reason: Some("stale_stopping_sweep"), transition_metadata: Some(meta), ..Default::default() },
        )
        .await
        .unwrap_or(false);

        if success {
            swept += 1;
            publish_meeting_status_change(state, meeting.id, target_status.as_str(), &meeting.platform, meeting.platform_specific_id.as_deref().unwrap_or(""), meeting.user_id, None).await;
        }
    }
    swept
}

/// Last time this meeting's status actually progressed, per its append-only transition
/// history — falls back to created_at for rows with no history yet.
fn last_progress_timestamp(meeting: &Meeting) -> chrono::DateTime<chrono::Utc> {
    let mut latest = meeting.created_at.unwrap_or_else(chrono::Utc::now);
    if let Some(transitions) = meeting.data.get("status_transition").and_then(Value::as_array) {
        for t in transitions {
            let Some(ts) = t.get("timestamp").and_then(Value::as_str) else { continue };
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) {
                let parsed = parsed.with_timezone(&chrono::Utc);
                if parsed > latest {
                    latest = parsed;
                }
            }
        }
    }
    latest
}

/// Runs all sweeps once. Called on a 60s loop from main.rs alongside the stop-outbox consumer.
pub async fn run_sweep_iteration(state: &AppState) {
    let swept = sweep_stale_stopping(state).await;
    if swept > 0 {
        tracing::warn!(swept, "[sweeps] stale-stopping sweep found stuck rows — operators should investigate why exit-callback path failed");
    }

    // ponytail: aggregation-retry (needs post_meeting::aggregate_transcription) and
    // unfinalized-recordings repair (needs recording_finalizer + storage S3 listing) aren't
    // ported yet — see meeting-api-rs task list. Both are recovery sweeps for already-durable
    // primary paths, not data-loss risks on their own.
    tracing::debug!("[sweeps] aggregation-retry and unfinalized-recordings sweeps not yet ported — skipping");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meeting_with_data(data: Value, created_at: chrono::DateTime<chrono::Utc>) -> Meeting {
        Meeting {
            id: 1,
            user_id: 1,
            platform: "google_meet".to_string(),
            platform_specific_id: Some("abc".to_string()),
            status: "stopping".to_string(),
            bot_container_id: None,
            start_time: None,
            end_time: None,
            data,
            created_at: Some(created_at),
            updated_at: None,
        }
    }

    #[test]
    fn falls_back_to_created_at_with_no_transition_history() {
        let created = chrono::Utc::now() - chrono::Duration::minutes(10);
        let meeting = meeting_with_data(json!({}), created);
        assert_eq!(last_progress_timestamp(&meeting), created);
    }

    #[test]
    fn uses_latest_transition_timestamp_when_present() {
        let created = chrono::Utc::now() - chrono::Duration::minutes(10);
        let recent = chrono::Utc::now() - chrono::Duration::minutes(1);
        let data = json!({"status_transition": [
            {"to": "joining", "timestamp": created.to_rfc3339()},
            {"to": "stopping", "timestamp": recent.to_rfc3339()},
        ]});
        let meeting = meeting_with_data(data, created);
        let result = last_progress_timestamp(&meeting);
        // Within a second of `recent` — round-trip through RFC3339 string parsing.
        assert!((result - recent).num_seconds().abs() <= 1);
    }
}
