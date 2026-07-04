//! Durable DELETE container-stop outbox. Faithful port of container_stop_outbox.py.
//!
//! A Redis Stream is the single durable record of "we promised to stop this container" —
//! producer (meetings.rs stop_bot) XADDs with a future fire_at; consumer (sweeps.rs, every
//! 60s) XRANGEs for due entries, calls runtime-api DELETE (idempotent — 200 no-op if already
//! stopped), XDELs on success, and on failure re-XADDs with exponential backoff up to
//! MAX_RETRIES before moving the entry to a DLQ Redis SET for operator inspection.

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const STREAM_KEY: &str = "meeting-api:container-stops";
const DLQ_KEY: &str = "meeting-api:container-stop-dlq";
const MAX_RETRIES: u32 = 5;
const MAX_STREAM_LENGTH: usize = 10_000;

fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

/// Push a delayed container-stop intent onto the outbox stream. Returns the stream entry id
/// on success. Callers may enqueue twice for the same container (e.g. a retried stop request)
/// — the consumer handles that safely since runtime-api DELETE is itself idempotent.
pub async fn enqueue_stop(redis: &mut ConnectionManager, container_name: &str, meeting_id: i32, delay_seconds: i64, backend_url: &str) -> Option<String> {
    let fire_at = now_secs() + delay_seconds.max(0) as f64;
    let fields: Vec<(&str, String)> = vec![
        ("container_name", container_name.to_string()),
        ("meeting_id", meeting_id.to_string()),
        ("backend_url", backend_url.to_string()),
        ("fire_at", format!("{fire_at:.0}")),
        ("enqueued_at", format!("{:.0}", now_secs())),
        ("attempts", "0".to_string()),
    ];
    match xadd_capped(redis, &fields).await {
        Ok(entry_id) => {
            tracing::info!(container_name, meeting_id, fire_at, entry_id, "[stop-outbox] enqueued stop");
            Some(entry_id)
        }
        Err(e) => {
            tracing::error!(container_name, error = %e, "[stop-outbox] enqueue FAILED");
            None
        }
    }
}

async fn xadd_capped(redis: &mut ConnectionManager, fields: &[(&str, String)]) -> redis::RedisResult<String> {
    redis::cmd("XADD")
        .arg(STREAM_KEY)
        .arg("MAXLEN")
        .arg("~")
        .arg(MAX_STREAM_LENGTH)
        .arg("*")
        .arg(fields)
        .query_async(redis)
        .await
}

async fn move_to_dlq(redis: &mut ConnectionManager, entry_id: &str, payload: &HashMap<String, String>, reason: &str) {
    let mut record: serde_json::Map<String, Value> = payload.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
    record.insert("dlq_reason".to_string(), json!(reason));
    record.insert("dlq_at".to_string(), json!(format!("{:.0}", now_secs())));
    record.insert("original_entry_id".to_string(), json!(entry_id));
    let record_json = Value::Object(record).to_string();

    if let Err(e) = redis.sadd::<_, _, ()>(DLQ_KEY, &record_json).await {
        tracing::error!(entry_id, error = %e, "[stop-outbox] DLQ write FAILED");
        return;
    }
    tracing::error!(
        container_name = payload.get("container_name").map(String::as_str).unwrap_or(""),
        meeting_id = payload.get("meeting_id").map(String::as_str).unwrap_or(""),
        attempts = payload.get("attempts").map(String::as_str).unwrap_or(""),
        reason,
        "[stop-outbox] DLQ: operator must investigate (orphan pod possible)"
    );
}

#[derive(Debug, Default)]
pub struct SweepResult {
    pub processed: u32,
    pub succeeded: u32,
    pub retried: u32,
    pub dlq: u32,
    pub deferred: u32,
}

/// One sweep pass: process all stream entries due (fire_at <= now). `stop_fn` is injected
/// (normally `handlers::meetings::stop_via_runtime_api`) so this module stays decoupled from
/// meeting-lifecycle code and independently testable.
pub async fn consume_pending_stops<F, Fut>(redis: &mut ConnectionManager, stop_fn: F) -> SweepResult
where
    F: Fn(String, String) -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let mut result = SweepResult::default();
    let now = now_secs();

    let entries: Vec<(String, HashMap<String, String>)> = match redis.xrange_all(STREAM_KEY).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(error = %e, "[stop-outbox] XRANGE failed");
            return result;
        }
    };

    for (entry_id, payload) in entries {
        let fire_at: f64 = payload.get("fire_at").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        if fire_at > now {
            result.deferred += 1;
            continue;
        }
        result.processed += 1;

        let container_name = payload.get("container_name").cloned().unwrap_or_default();
        let meeting_id = payload.get("meeting_id").cloned().unwrap_or_else(|| "?".to_string());
        let backend_url = payload.get("backend_url").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "http://localhost:8091".to_string());
        let attempts: u32 = payload.get("attempts").and_then(|v| v.parse().ok()).unwrap_or(0);

        if container_name.is_empty() {
            move_to_dlq(redis, &entry_id, &payload, "malformed_no_container_name").await;
            let _: Result<(), _> = redis.xdel(STREAM_KEY, &[&entry_id]).await;
            result.dlq += 1;
            continue;
        }

        tracing::info!(container_name, meeting_id, attempt = attempts + 1, max = MAX_RETRIES, "[stop-outbox] firing stop");
        let success = stop_fn(container_name.clone(), backend_url).await;

        if success {
            result.succeeded += 1;
            let _: Result<(), _> = redis.xdel(STREAM_KEY, &[&entry_id]).await;
            tracing::info!(container_name, meeting_id, entry_id, "[stop-outbox] stop OK; entry acked");
            continue;
        }

        let new_attempts = attempts + 1;
        if new_attempts >= MAX_RETRIES {
            let mut bumped = payload.clone();
            bumped.insert("attempts".to_string(), new_attempts.to_string());
            move_to_dlq(redis, &entry_id, &bumped, &format!("max_retries_exceeded ({MAX_RETRIES})")).await;
            let _: Result<(), _> = redis.xdel(STREAM_KEY, &[&entry_id]).await;
            result.dlq += 1;
            continue;
        }

        let backoff = 60.0 * 2f64.powi(new_attempts as i32);
        let next_fire_at = now + backoff;
        let mut new_fields: Vec<(&str, String)> = payload.iter().filter(|(k, _)| k.as_str() != "attempts" && k.as_str() != "fire_at").map(|(k, v)| (k.as_str(), v.clone())).collect();
        new_fields.push(("attempts", new_attempts.to_string()));
        new_fields.push(("fire_at", format!("{next_fire_at:.0}")));
        new_fields.push(("last_failure_at", format!("{now:.0}")));

        match xadd_capped(redis, &new_fields).await {
            Ok(_) => {
                let _: Result<(), _> = redis.xdel(STREAM_KEY, &[&entry_id]).await;
                result.retried += 1;
                tracing::warn!(container_name, new_attempts, max = MAX_RETRIES, backoff, "[stop-outbox] stop FAILED; retry scheduled");
            }
            Err(e) => {
                tracing::error!(container_name, error = %e, "[stop-outbox] re-enqueue FAILED; original entry preserved for next sweep");
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_secs_is_a_real_unix_timestamp() {
        // Sanity check, not a mock — any time after this session started.
        assert!(now_secs() > 1_700_000_000.0);
    }
}
