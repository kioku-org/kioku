//! Pack J — data-driven exit classification (#255 silent-class fix). Faithful port of
//! callbacks.py's `_classify_stopped_exit`. [PLATFORM] data showed 47% of `completed`
//! meetings in a 30-day window were actually misclassified (432 stopped-pre-admission +
//! 125 substantive-silent-failures); this closes that gap by inspecting the same signals
//! the original data analysis used, rather than trusting the bot's self-reported reason.

use crate::schemas::{MeetingCompletionReason, MeetingStatus};
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;

pub async fn classify_stopped_exit(
    db: &PgPool,
    meeting_id: i32,
    meeting_data: &Value,
    start_time: Option<chrono::DateTime<Utc>>,
    end_time: Option<chrono::DateTime<Utc>>,
    requested_reason: MeetingCompletionReason,
) -> (MeetingStatus, MeetingCompletionReason) {
    // User-initiated stop is NEVER a failure, regardless of lifecycle stage at DELETE time.
    let user_initiated_stop = meeting_data.get("stop_requested").and_then(|v| v.as_bool()).unwrap_or(false);
    if user_initiated_stop {
        return (MeetingStatus::Completed, requested_reason);
    }

    if requested_reason.is_explicit_failure() {
        return (MeetingStatus::Failed, requested_reason);
    }
    if requested_reason == MeetingCompletionReason::LeftAlone {
        return (MeetingStatus::Completed, requested_reason);
    }
    if requested_reason != MeetingCompletionReason::Stopped {
        // Defensive: shouldn't happen (caller passes only STOPPED through here for the
        // deeper checks), but never silently mark completed for an unrecognized reason.
        return (MeetingStatus::Failed, requested_reason);
    }

    let transitions = meeting_data.get("status_transition").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let reached_active = transitions.iter().any(|t| t.get("to").and_then(|v| v.as_str()) == Some("active"));
    if !reached_active {
        return (MeetingStatus::Failed, MeetingCompletionReason::StoppedBeforeAdmission);
    }

    let duration_s = match start_time {
        Some(start) => (end_time.unwrap_or_else(Utc::now) - start).num_milliseconds() as f64 / 1000.0,
        None => 0.0,
    };
    let transcribe_enabled = meeting_data.get("transcribe_enabled").and_then(|v| v.as_bool()).unwrap_or(true);

    if duration_s < 30.0 || !transcribe_enabled {
        return (MeetingStatus::Completed, requested_reason);
    }

    let segment_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transcriptions WHERE meeting_id = $1")
        .bind(meeting_id)
        .fetch_one(db)
        .await
        .unwrap_or(0);

    if segment_count == 0 {
        // Recording delivered (even with zero speech) is a successful capture, not a
        // failure — don't bury a downloadable recording behind a `failed` status.
        let recording_delivered = meeting_data
            .get("recordings")
            .and_then(|v| v.as_array())
            .map(|recs| {
                recs.iter().any(|rec| {
                    rec.get("media_files")
                        .and_then(|v| v.as_array())
                        .map(|mfs| mfs.iter().any(|mf| mf.get("file_size_bytes").and_then(|v| v.as_i64()).unwrap_or(0) > 0))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if recording_delivered {
            return (MeetingStatus::Completed, requested_reason);
        }
        return (MeetingStatus::Failed, MeetingCompletionReason::StoppedWithNoAudio);
    }

    (MeetingStatus::Completed, requested_reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_explicit_failure_matches_python_set() {
        assert!(MeetingCompletionReason::AwaitingAdmissionTimeout.is_explicit_failure());
        assert!(MeetingCompletionReason::StoppedBeforeAdmission.is_explicit_failure());
        assert!(!MeetingCompletionReason::Stopped.is_explicit_failure());
        assert!(!MeetingCompletionReason::LeftAlone.is_explicit_failure());
    }
}
