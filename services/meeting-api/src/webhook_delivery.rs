//! Webhook delivery with exponential backoff and HMAC signing. Faithful port of
//! webhook_delivery.py + retry.py. Frozen wire contract (envelope shape, header names,
//! HMAC-over-`{ts}.{body}`) — must match what customers' existing webhook verifiers expect.

use hmac::{Hmac, Mac};
use rand::Rng;
use serde_json::{json, Value};
use sha2::Sha256;
use std::time::Duration;
use uuid::Uuid;

pub const WEBHOOK_API_VERSION: &str = "2026-03-01";
const RETRY_QUEUE_KEY: &str = "webhook:retry_queue";
const RETRYABLE_STATUS: &[u16] = &[429, 500, 502, 503, 504];

const INTERNAL_DATA_KEYS: &[&str] = &[
    "webhook_delivery", "webhook_deliveries", "webhook_secret", "webhook_secrets",
    "webhook_events", "webhook_url", "outbound_events", "bot_container_id", "container_name",
];

pub fn build_envelope(event_type: &str, data: Value) -> Value {
    json!({
        "event_id": format!("evt_{}", Uuid::new_v4().simple()),
        "event_type": event_type,
        "api_version": WEBHOOK_API_VERSION,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "data": data,
    })
}

/// Strip internal bookkeeping keys from meeting.data before it goes out over a webhook.
pub fn clean_meeting_data(data: &Value) -> Value {
    match data.as_object() {
        None => json!({}),
        Some(obj) => Value::Object(obj.iter().filter(|(k, _)| !INTERNAL_DATA_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()),
    }
}

fn hmac_hex(secret: &str, message: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(message);
    hex::encode(mac.finalize().into_bytes())
}

pub fn sign_payload(payload_bytes: &[u8], secret: &str) -> String {
    format!("sha256={}", hmac_hex(secret, payload_bytes))
}

pub fn build_headers(webhook_secret: Option<&str>, payload_bytes: &[u8]) -> Vec<(String, String)> {
    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    if let Some(secret) = webhook_secret.map(str::trim).filter(|s| !s.is_empty()) {
        headers.push(("Authorization".to_string(), format!("Bearer {secret}")));
        let ts = chrono::Utc::now().timestamp().to_string();
        let mut signed_content = format!("{ts}.").into_bytes();
        signed_content.extend_from_slice(payload_bytes);
        let sig = hmac_hex(secret, &signed_content);
        headers.push(("X-Webhook-Signature".to_string(), format!("sha256={sig}")));
        headers.push(("X-Webhook-Timestamp".to_string(), ts));
    }
    headers
}

pub enum DeliveryStatus {
    Delivered { status_code: u16 },
    Queued,
    Failed { error: String },
}

async fn send_once(http: &reqwest::Client, url: &str, payload_bytes: &[u8], headers: &[(String, String)], timeout: Duration) -> Result<reqwest::Response, reqwest::Error> {
    let mut req = http.post(url).body(payload_bytes.to_vec()).timeout(timeout);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    req.send().await
}

/// Exponential backoff over transient failures (timeouts, connection errors, and the
/// retryable HTTP status set), matching retry.py's with_retry exactly (base 1s, cap 10s,
/// 3 retries, jitter up to 0.5s).
async fn deliver_with_retry(http: &reqwest::Client, url: &str, payload_bytes: &[u8], headers: &[(String, String)], timeout: Duration, max_retries: u32) -> Result<reqwest::Response, String> {
    let mut attempt = 0u32;
    loop {
        match send_once(http, url, payload_bytes, headers, timeout).await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if RETRYABLE_STATUS.contains(&status) && attempt < max_retries {
                    attempt += 1;
                    let delay = backoff_delay(attempt - 1);
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                let retryable = e.is_timeout() || e.is_connect();
                if retryable && attempt < max_retries {
                    attempt += 1;
                    let delay = backoff_delay(attempt - 1);
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(e.to_string());
            }
        }
    }
}

fn backoff_delay(attempt: u32) -> Duration {
    let base = 1.0_f64 * 2f64.powi(attempt as i32);
    let jitter: f64 = rand::thread_rng().gen_range(0.0..0.5);
    Duration::from_secs_f64((base + jitter).min(10.0))
}

async fn enqueue_failed_webhook(redis: &mut redis::aio::ConnectionManager, url: &str, payload: &Value, headers: &[(String, String)], webhook_secret: Option<&str>, label: &str, metadata: Option<&Value>) -> bool {
    let now = chrono::Utc::now().timestamp() as f64;
    let mut entry = json!({
        "url": url,
        "payload": payload,
        "headers": headers.iter().cloned().collect::<std::collections::HashMap<_, _>>(),
        "webhook_secret": webhook_secret,
        "label": label,
        "attempt": 0,
        "next_retry_at": now + 60.0,
        "created_at": now,
    });
    if let Some(m) = metadata {
        entry["metadata"] = m.clone();
    }
    redis::AsyncCommands::rpush::<_, _, ()>(redis, RETRY_QUEUE_KEY, entry.to_string()).await.is_ok()
}

/// Deliver a webhook: HMAC-sign, POST with retry, and — on exhausted retries — persist to
/// the Redis durable-retry queue for the (not-yet-ported) background retry worker to pick up.
pub async fn deliver(
    http: &reqwest::Client,
    redis: Option<&mut redis::aio::ConnectionManager>,
    url: &str,
    payload: &Value,
    webhook_secret: Option<&str>,
    label: &str,
    metadata: Option<&Value>,
) -> DeliveryStatus {
    let payload_bytes = payload.to_string().into_bytes();
    let headers = build_headers(webhook_secret, &payload_bytes);

    match deliver_with_retry(http, url, &payload_bytes, &headers, Duration::from_secs(30), 3).await {
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            if status_code < 300 {
                tracing::info!(url, status_code, "webhook delivered");
            } else {
                tracing::warn!(url, status_code, "webhook returned non-2xx");
            }
            DeliveryStatus::Delivered { status_code }
        }
        Err(e) => {
            tracing::error!(url, error = %e, "webhook delivery failed after retries");
            if let Some(redis) = redis {
                if enqueue_failed_webhook(redis, url, payload, &headers, webhook_secret, label, metadata).await {
                    return DeliveryStatus::Queued;
                }
            }
            DeliveryStatus::Failed { error: e }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_payload_matches_expected_hmac_sha256_format() {
        let sig = sign_payload(b"hello", "secret");
        assert!(sig.starts_with("sha256="));
        assert_eq!(sig.len(), "sha256=".len() + 64); // hex-encoded SHA-256 digest
    }

    #[test]
    fn build_headers_without_secret_is_just_content_type() {
        let headers = build_headers(None, b"{}");
        assert_eq!(headers, vec![("Content-Type".to_string(), "application/json".to_string())]);
    }

    #[test]
    fn build_headers_with_secret_includes_signature_and_timestamp() {
        let headers = build_headers(Some("shh"), b"{}");
        let names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"Authorization"));
        assert!(names.contains(&"X-Webhook-Signature"));
        assert!(names.contains(&"X-Webhook-Timestamp"));
    }

    #[test]
    fn clean_meeting_data_strips_internal_keys() {
        let data = json!({"webhook_url": "http://x", "webhook_secret": "s", "notes": "keep me"});
        let cleaned = clean_meeting_data(&data);
        assert_eq!(cleaned, json!({"notes": "keep me"}));
    }
}
