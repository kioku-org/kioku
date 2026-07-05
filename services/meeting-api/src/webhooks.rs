//! Faithful port of webhooks.py — builds the per-meeting webhook event payload and delivers
//! it via webhook_delivery. Reads webhook_url/webhook_secret/webhook_events straight out of
//! meeting.data (set at spawn time from the gateway's X-User-Webhook-* headers).

use crate::models::Meeting;
use crate::state::AppState;
use crate::webhook_delivery::{build_envelope, clean_meeting_data, deliver, DeliveryStatus};
use crate::webhook_url::validate_webhook_url;
use serde_json::{json, Value};
use sqlx::PgPool;

/// completed is intentionally absent — send_completion_webhook owns meeting.completed
/// separately; including it here would double-deliver on the terminal transition.
fn resolve_event_type(status: &str) -> &'static str {
    match status {
        "active" => "meeting.started",
        "failed" => "bot.failed",
        _ => "meeting.status_change",
    }
}

fn is_event_enabled(meeting_data: &Value, event_type: &str) -> bool {
    let default_enabled = event_type == "meeting.completed";
    match meeting_data.get("webhook_events").and_then(|v| v.as_object()) {
        None => default_enabled,
        Some(events) => match events.get(event_type).and_then(|v| v.as_bool()) {
            Some(enabled) => enabled,
            None => default_enabled,
        },
    }
}

fn webhook_config(meeting: &Meeting) -> (Option<String>, Option<String>) {
    (
        meeting.data.get("webhook_url").and_then(|v| v.as_str()).map(str::to_string),
        meeting.data.get("webhook_secret").and_then(|v| v.as_str()).map(str::to_string),
    )
}

fn build_meeting_event_data(meeting: &Meeting) -> Value {
    json!({
        "id": meeting.id,
        "user_id": meeting.user_id,
        "platform": meeting.platform,
        "native_meeting_id": meeting.platform_specific_id,
        "status": meeting.status,
        "completion_reason": meeting.data.get("completion_reason"),
        "failure_stage": meeting.data.get("failure_stage"),
        "start_time": meeting.start_time.map(|t| t.and_utc().to_rfc3339()),
        "end_time": meeting.end_time.map(|t| t.and_utc().to_rfc3339()),
        "data": clean_meeting_data(&meeting.data),
        "created_at": meeting.created_at.map(|t| t.and_utc().to_rfc3339()),
        "updated_at": meeting.updated_at.map(|t| t.and_utc().to_rfc3339()),
    })
}

/// #327 — webhook bookkeeping must not bump meetings.updated_at (that column is the "domain
/// progress" signal the stale-stopping sweep keys off; retry storms shouldn't reset its
/// staleness clock). Writes updated_at back to its own current value to defeat any
/// ON UPDATE-style trigger, matching the Python raw-UPDATE approach exactly.
async fn persist_data_preserving_updated_at(db: &PgPool, meeting_id: i32, data: &Value, updated_at: Option<chrono::NaiveDateTime>) {
    let _ = sqlx::query("UPDATE meetings SET data = $1, updated_at = COALESCE($2, updated_at) WHERE id = $3")
        .bind(data)
        .bind(updated_at)
        .bind(meeting_id)
        .execute(db)
        .await;
}

async fn append_delivery_log(db: &PgPool, meeting: &Meeting, entry: Value) {
    let mut data = meeting.data.clone();
    let mut log: Vec<Value> = data.get("webhook_deliveries").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    log.push(entry);
    if log.len() > 20 {
        log = log.split_off(log.len() - 20);
    }
    data["webhook_deliveries"] = json!(log);
    persist_data_preserving_updated_at(db, meeting.id, &data, meeting.updated_at).await;
}

async fn write_delivery_status(db: &PgPool, meeting: &Meeting, status: Value) {
    let mut data = meeting.data.clone();
    data["webhook_delivery"] = status;
    persist_data_preserving_updated_at(db, meeting.id, &data, meeting.updated_at).await;
}

/// Post-meeting webhook (meeting.completed) — called from post_meeting tasks.
pub async fn send_completion_webhook(state: &AppState, meeting: &Meeting) {
    let (Some(webhook_url), webhook_secret) = webhook_config(meeting) else { return };
    if validate_webhook_url(&webhook_url).await.is_err() {
        return;
    }

    let payload = build_envelope("meeting.completed", json!({"meeting": build_meeting_event_data(meeting)}));
    let now = chrono::Utc::now().to_rfc3339();
    let mut redis = state.redis.clone();
    let metadata = json!({"meeting_id": meeting.id});
    let label = format!("client-webhook meeting={} user={}", meeting.id, meeting.user_id);

    let status = deliver(&state.http, Some(&mut redis), &webhook_url, &payload, webhook_secret.as_deref(), &label, Some(&metadata)).await;
    let recorded = match status {
        DeliveryStatus::Delivered { status_code } => json!({"url": webhook_url, "status_code": status_code, "attempts": 1, "delivered_at": now, "status": "delivered"}),
        DeliveryStatus::Queued => json!({"url": webhook_url, "attempts": 0, "status": "queued", "queued_at": now}),
        DeliveryStatus::Failed { .. } => json!({"url": webhook_url, "attempts": 3, "status": "failed", "failed_at": now}),
    };
    write_delivery_status(&state.db, meeting, recorded).await;
}

/// Fire-and-forget webhook for recording/transcription events — faithful port of
/// webhooks.py's `send_event_webhook`. Unlike send_completion_webhook/send_status_webhook,
/// this is not gated by `is_event_enabled` (Python doesn't gate it either) and doesn't
/// persist a delivery-status record onto meeting.data.
pub async fn send_event_webhook(state: &AppState, meeting_id: i32, event_type: &str, data: Value) {
    let meeting: Option<Meeting> = sqlx::query_as("SELECT * FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(&state.db).await.unwrap_or(None);
    let Some(meeting) = meeting else { return };
    let (Some(webhook_url), webhook_secret) = webhook_config(&meeting) else { return };
    if validate_webhook_url(&webhook_url).await.is_err() {
        return;
    }

    let payload = build_envelope(event_type, data);
    let mut redis = state.redis.clone();
    let label = format!("event-webhook {event_type} meeting={meeting_id}");
    deliver(&state.http, Some(&mut redis), &webhook_url, &payload, webhook_secret.as_deref(), &label, None).await;
}

pub struct StatusChangeInfo<'a> {
    pub old_status: &'a str,
    pub new_status: &'a str,
    pub reason: Option<&'a str>,
    pub transition_source: &'a str,
}

/// Status-change webhook — called on every transition.
pub async fn send_status_webhook(state: &AppState, meeting: &Meeting, status_change_info: Option<StatusChangeInfo<'_>>) {
    let (Some(webhook_url), webhook_secret) = webhook_config(meeting) else { return };

    let resolution_status = status_change_info.as_ref().map(|s| s.new_status).unwrap_or(&meeting.status);
    let event_type = resolve_event_type(resolution_status);
    if !is_event_enabled(&meeting.data, event_type) {
        return;
    }
    if validate_webhook_url(&webhook_url).await.is_err() {
        return;
    }

    let mut event_data = json!({"meeting": build_meeting_event_data(meeting)});
    if let Some(info) = &status_change_info {
        event_data["status_change"] = json!({
            "from": info.old_status,
            "to": info.new_status,
            "reason": info.reason,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "transition_source": info.transition_source,
        });
    }

    let payload = build_envelope(event_type, event_data);
    let now = chrono::Utc::now().to_rfc3339();
    let mut redis = state.redis.clone();
    let metadata = json!({"meeting_id": meeting.id});
    let label = format!("status-webhook meeting={} status={}", meeting.id, meeting.status);

    let status = deliver(&state.http, Some(&mut redis), &webhook_url, &payload, webhook_secret.as_deref(), &label, Some(&metadata)).await;
    let mut entry = json!({"event_type": event_type, "url": webhook_url, "timestamp": now});
    match status {
        DeliveryStatus::Delivered { status_code } => {
            entry["status"] = json!("delivered");
            entry["status_code"] = json!(status_code);
        }
        DeliveryStatus::Queued => entry["status"] = json!("queued"),
        DeliveryStatus::Failed { .. } => entry["status"] = json!("failed"),
    }
    append_delivery_log(&state.db, meeting, entry).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_event_type_maps_active_and_failed() {
        assert_eq!(resolve_event_type("active"), "meeting.started");
        assert_eq!(resolve_event_type("failed"), "bot.failed");
        assert_eq!(resolve_event_type("joining"), "meeting.status_change");
        // completed is deliberately not special-cased here — send_completion_webhook owns it.
        assert_eq!(resolve_event_type("completed"), "meeting.status_change");
    }

    #[test]
    fn is_event_enabled_defaults_to_completed_only() {
        let empty = json!({});
        assert!(is_event_enabled(&empty, "meeting.completed"));
        assert!(!is_event_enabled(&empty, "meeting.started"));
    }

    #[test]
    fn is_event_enabled_respects_explicit_config() {
        let data = json!({"webhook_events": {"meeting.started": true, "meeting.completed": false}});
        assert!(is_event_enabled(&data, "meeting.started"));
        assert!(!is_event_enabled(&data, "meeting.completed"));
    }
}
