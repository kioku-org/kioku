//! Internal outbound-event ledger stored in meeting.data. Faithful port of outbound_events.py —
//! a no-migration outbox for internal post-meeting hooks (billing/analytics). One ledger entry
//! per (channel, event_type, meeting, destination); lets callers distinguish delivered/queued/
//! pending/failed so retries don't double-deliver.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const OUTBOUND_EVENTS_KEY: &str = "outbound_events";
const DEFAULT_PENDING_MAX_AGE_SECONDS: i64 = 300;

pub fn destination_hash(destination: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(destination.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

pub fn event_key(channel: &str, event_type: &str, meeting_id: i32, destination: &str) -> String {
    format!("{channel}:{event_type}:{meeting_id}:{}", destination_hash(destination))
}

/// Claim an outbound event under a row lock. Returns (key, event, should_deliver) — delivered/
/// queued/pending events are not re-delivered; failed events can be reclaimed since no durable
/// owner currently has the work.
pub async fn claim_outbound_event(db: &PgPool, meeting_id: i32, channel: &str, event_type: &str, destination: &str, payload: &Value) -> (String, Value, bool) {
    let key = event_key(channel, event_type, meeting_id, destination);
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = match db.begin().await {
        Ok(t) => t,
        Err(_) => return (key, json!({"status": "failed", "error": "db error"}), false),
    };
    let data: Option<Value> = sqlx::query_scalar("SELECT data FROM meetings WHERE id = $1 FOR UPDATE").bind(meeting_id).fetch_optional(&mut *tx).await.unwrap_or(None);
    let Some(mut data) = data else {
        let _ = tx.rollback().await;
        return (key, json!({"status": "failed", "error": "meeting not found"}), false);
    };

    let mut events = data.get(OUTBOUND_EVENTS_KEY).and_then(Value::as_object).cloned().unwrap_or_default();
    if let Some(existing) = events.get(&key) {
        if matches!(existing.get("status").and_then(Value::as_str), Some("delivered") | Some("queued") | Some("pending")) {
            let _ = tx.rollback().await;
            return (key, existing.clone(), false);
        }
    }

    let mut event = events.get(&key).and_then(Value::as_object).cloned().unwrap_or_default();
    let attempts = event.get("attempts").and_then(Value::as_i64).unwrap_or(0);
    let first_claimed_at = event.get("first_claimed_at").or_else(|| event.get("claimed_at")).cloned().unwrap_or(json!(now));
    event.insert("key".to_string(), json!(key));
    event.insert("channel".to_string(), json!(channel));
    event.insert("event_type".to_string(), json!(event_type));
    event.insert("destination".to_string(), json!(destination));
    event.insert("destination_hash".to_string(), json!(destination_hash(destination)));
    event.insert("payload".to_string(), payload.clone());
    event.insert("status".to_string(), json!("pending"));
    event.insert("first_claimed_at".to_string(), first_claimed_at);
    event.insert("claimed_at".to_string(), json!(now));
    event.insert("updated_at".to_string(), json!(now));
    event.insert("attempts".to_string(), json!(attempts));

    let event_value = Value::Object(event);
    events.insert(key.clone(), event_value.clone());
    data[OUTBOUND_EVENTS_KEY] = Value::Object(events);

    if sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting_id).execute(&mut *tx).await.is_err() {
        let _ = tx.rollback().await;
        return (key, json!({"status": "failed", "error": "commit failed"}), false);
    }
    let _ = tx.commit().await;
    (key, event_value, true)
}

/// Update one ledger event under row lock (delivery outcome).
pub async fn mark_outbound_event(db: &PgPool, meeting_id: i32, key: &str, status: &str, attempts: Option<i64>, error: Option<&str>, status_code: Option<u16>) {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = match db.begin().await {
        Ok(t) => t,
        Err(_) => return,
    };
    let data: Option<Value> = sqlx::query_scalar("SELECT data FROM meetings WHERE id = $1 FOR UPDATE").bind(meeting_id).fetch_optional(&mut *tx).await.unwrap_or(None);
    let Some(mut data) = data else {
        let _ = tx.rollback().await;
        return;
    };

    let mut events = data.get(OUTBOUND_EVENTS_KEY).and_then(Value::as_object).cloned().unwrap_or_default();
    let mut event = events.get(key).and_then(Value::as_object).cloned().unwrap_or_default();
    event.insert("key".to_string(), json!(key));
    event.insert("status".to_string(), json!(status));
    event.insert("updated_at".to_string(), json!(now));
    if let Some(a) = attempts {
        event.insert("attempts".to_string(), json!(a));
    }
    if let Some(e) = error {
        event.insert("error".to_string(), json!(&e[..e.len().min(500)]));
    } else if matches!(status, "delivered" | "queued" | "pending") {
        event.remove("error");
    }
    if let Some(sc) = status_code {
        event.insert("status_code".to_string(), json!(sc));
    }
    match status {
        "delivered" => {
            event.insert("delivered_at".to_string(), json!(now));
        }
        "queued" => {
            event.insert("queued_at".to_string(), json!(now));
        }
        "failed" => {
            event.insert("failed_at".to_string(), json!(now));
        }
        _ => {}
    }

    events.insert(key.to_string(), Value::Object(event));
    data[OUTBOUND_EVENTS_KEY] = Value::Object(events);
    let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting_id).execute(&mut *tx).await;
    let _ = tx.commit().await;
}

pub fn is_stale_pending_event(event: &Value, now: chrono::DateTime<chrono::Utc>, max_age_seconds: i64) -> bool {
    if event.get("status").and_then(Value::as_str) != Some("pending") {
        return false;
    }
    let claimed_at = event.get("claimed_at").or_else(|| event.get("updated_at")).and_then(Value::as_str).and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
    match claimed_at {
        Some(t) => (now - t.with_timezone(&chrono::Utc)).num_seconds() > max_age_seconds,
        None => true,
    }
}

pub const DEFAULT_MAX_AGE_SECONDS: i64 = DEFAULT_PENDING_MAX_AGE_SECONDS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_hash_is_stable_and_16_hex_chars() {
        let h1 = destination_hash("https://example.com/hook");
        let h2 = destination_hash("https://example.com/hook");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn event_key_incorporates_all_components() {
        let key = event_key("post_meeting_hooks", "meeting.completed", 42, "https://a.example/hook");
        assert!(key.starts_with("post_meeting_hooks:meeting.completed:42:"));
    }

    #[test]
    fn stale_pending_detection() {
        let old = chrono::Utc::now() - chrono::Duration::seconds(400);
        let event = json!({"status": "pending", "claimed_at": old.to_rfc3339()});
        assert!(is_stale_pending_event(&event, chrono::Utc::now(), DEFAULT_MAX_AGE_SECONDS));

        let recent = chrono::Utc::now() - chrono::Duration::seconds(10);
        let event2 = json!({"status": "pending", "claimed_at": recent.to_rfc3339()});
        assert!(!is_stale_pending_event(&event2, chrono::Utc::now(), DEFAULT_MAX_AGE_SECONDS));

        let delivered = json!({"status": "delivered", "claimed_at": old.to_rfc3339()});
        assert!(!is_stale_pending_event(&delivered, chrono::Utc::now(), DEFAULT_MAX_AGE_SECONDS));
    }
}
