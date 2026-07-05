//! Faithful port of meetings.py's `update_meeting_status` + `publish_meeting_status_change`.
//! This is the core state-machine writer every callback goes through — row-locked to avoid
//! the TOCTOU race between concurrent callbacks, same as the Python original.

use crate::models::Meeting;
use crate::schemas::{
    get_status_source, is_valid_status_transition, MeetingCompletionReason, MeetingFailureStage,
    MeetingStatus,
};
use crate::state::AppState;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Debug, Default)]
pub struct StatusUpdateOptions<'a> {
    pub completion_reason: Option<MeetingCompletionReason>,
    pub failure_stage: Option<MeetingFailureStage>,
    pub error_details: Option<&'a str>,
    pub transition_reason: Option<&'a str>,
    pub transition_metadata: Option<Value>,
}

/// Returns Ok(true) if the transition was applied (or was a benign idempotent terminal
/// re-fire), Ok(false) if the transition was illegal, Err on a DB failure.
pub async fn update_meeting_status(
    db: &PgPool,
    meeting_id: i32,
    new_status: MeetingStatus,
    opts: StatusUpdateOptions<'_>,
) -> anyhow::Result<bool> {
    let mut tx = db.begin().await?;

    // SELECT ... FOR UPDATE — prevents the concurrent-callback TOCTOU race the Python
    // version guards against with the same lock.
    let meeting: Meeting = sqlx::query_as("SELECT * FROM meetings WHERE id = $1 FOR UPDATE")
        .bind(meeting_id)
        .fetch_one(&mut *tx)
        .await?;

    let current_status = MeetingStatus::parse(&meeting.status).unwrap_or(MeetingStatus::Failed);

    if !is_valid_status_transition(current_status, new_status) {
        // Idempotent re-fire of an already-terminal status is benign (documented race
        // across chat-persistence / status-update / post-meeting-task paths), not a real
        // rejection.
        if current_status == new_status && current_status.is_terminal() {
            tx.commit().await?;
            return Ok(true);
        }
        tx.rollback().await?;
        return Ok(false);
    }

    let old_status = meeting.status.clone();
    let mut data = match &meeting.data {
        Value::Object(_) => meeting.data.clone(),
        _ => json!({}),
    };
    let obj = data.as_object_mut().expect("data is always an object here");

    if new_status == MeetingStatus::Failed {
        // Pack R: failure_stage tracks the stage the meeting WAS IN when failure
        // happened. Only overwrite when current_status maps to an actual lifecycle
        // stage (not a transitional/terminal status like "stopping").
        if let Some(stage) = match current_status {
            MeetingStatus::Requested => Some(MeetingFailureStage::Requested),
            MeetingStatus::Joining => Some(MeetingFailureStage::Joining),
            MeetingStatus::AwaitingAdmission => Some(MeetingFailureStage::AwaitingAdmission),
            MeetingStatus::Active => Some(MeetingFailureStage::Active),
            _ => None,
        } {
            obj.insert("failure_stage".to_string(), json!(stage.as_str()));
        }
    }

    let mut end_time_now = false;
    if new_status == MeetingStatus::Completed {
        if let Some(r) = opts.completion_reason {
            obj.insert("completion_reason".to_string(), json!(r.as_str()));
        }
        end_time_now = true;
    } else if new_status == MeetingStatus::Failed {
        if let Some(r) = opts.completion_reason {
            obj.insert("completion_reason".to_string(), json!(r.as_str()));
        }
        if let Some(s) = opts.failure_stage {
            obj.insert("failure_stage".to_string(), json!(s.as_str()));
        }
        if let Some(e) = opts.error_details {
            obj.insert("error_details".to_string(), json!(e));
        }
        end_time_now = true;
    }

    let mut transition_entry = json!({
        "from": old_status,
        "to": new_status.as_str(),
        "timestamp": Utc::now().to_rfc3339(),
        "source": get_status_source(current_status, new_status),
    });
    let te = transition_entry.as_object_mut().unwrap();
    if let Some(r) = opts.transition_reason {
        te.insert("reason".to_string(), json!(r));
    }
    if let Some(r) = opts.completion_reason {
        te.insert("completion_reason".to_string(), json!(r.as_str()));
    }
    if let Some(s) = opts.failure_stage {
        te.insert("failure_stage".to_string(), json!(s.as_str()));
    }
    if let Some(e) = opts.error_details {
        te.insert("error_details".to_string(), json!(e));
    }
    if let Some(Value::Object(meta)) = &opts.transition_metadata {
        for (k, v) in meta {
            te.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    let mut transitions: Vec<Value> = match obj.get("status_transition") {
        Some(Value::Array(a)) => a.clone(),
        Some(v @ Value::Object(_)) => vec![v.clone()],
        _ => vec![],
    };
    transitions.push(transition_entry);
    obj.insert("status_transition".to_string(), Value::Array(transitions));
    obj.remove("status_transitions");

    if end_time_now {
        sqlx::query("UPDATE meetings SET status = $1, data = $2, end_time = now() WHERE id = $3")
            .bind(new_status.as_str())
            .bind(&data)
            .bind(meeting_id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE meetings SET status = $1, data = $2 WHERE id = $3")
            .bind(new_status.as_str())
            .bind(&data)
            .bind(meeting_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(true)
}

/// Publish to bm:meeting:{id}:status — frozen channel prefix, matches Python exactly.
pub async fn publish_meeting_status_change(
    state: &AppState,
    meeting_id: i32,
    new_status: &str,
    platform: &str,
    native_meeting_id: &str,
    user_id: i32,
    extra_data: Option<Value>,
) {
    let mut status_payload = json!({"status": new_status});
    if let Some(extra) = extra_data {
        status_payload["data"] = extra;
    }
    let payload = json!({
        "type": "meeting.status",
        "meeting": {"id": meeting_id, "platform": platform, "native_id": native_meeting_id},
        "payload": status_payload,
        "user_id": user_id,
        "ts": Utc::now().to_rfc3339(),
    });
    let channel = format!("bm:meeting:{meeting_id}:status");
    let mut redis = state.redis.clone();
    if let Err(e) = redis::AsyncCommands::publish::<_, _, ()>(&mut redis, &channel, payload.to_string()).await {
        tracing::error!(meeting_id, %channel, error = %e, "failed to publish status change");
    }
}

/// Fire-and-forget status-change webhook delivery — matches Python's
/// `background_tasks.add_task(...)` semantics (doesn't block the callback response).
pub fn schedule_status_webhook_task(
    state: &AppState,
    meeting: Meeting,
    old_status: String,
    new_status: String,
    reason: Option<String>,
    transition_source: &'static str,
) {
    let state = state.clone();
    tokio::spawn(async move {
        crate::webhooks::send_status_webhook(
            &state,
            &meeting,
            Some(crate::webhooks::StatusChangeInfo { old_status: &old_status, new_status: &new_status, reason: reason.as_deref(), transition_source }),
        )
        .await;
    });
}
