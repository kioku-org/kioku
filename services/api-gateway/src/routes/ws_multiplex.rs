use crate::auth::{resolve_token, user_scopes};
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{RawQuery, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(handler))
}

type SubKey = (String, String, String); // (platform, native_id, user_id)

async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            serde_urlencoded::from_str::<HashMap<String, String>>(&query.unwrap_or_default())
                .ok()
                .and_then(|q| q.get("api_key").cloned())
        });

    ws.on_upgrade(move |socket| async move { run(socket, state, api_key).await })
}

fn send_json(out: &UnboundedSender<Message>, value: Value) -> bool {
    out.send(Message::Text(value.to_string())).is_ok()
}

async fn run(socket: WebSocket, state: AppState, api_key: Option<String>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    // Single writer task: every producer (the main loop below, plus one per-meeting fan_in
    // task) sends into `out_tx` instead of touching the socket directly, since SplitSink
    // isn't cloneable and only one task may own it at a time.
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let Some(api_key) = api_key else {
        send_json(&out_tx, json!({"type": "error", "error": "missing_api_key"}));
        let _ = out_tx.send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code: 4401,
            reason: "".into(),
        })));
        drop(out_tx);
        let _ = writer.await;
        return;
    };

    let mut sub_tasks: HashMap<SubKey, JoinHandle<()>> = HashMap::new();
    let mut subscribed: HashSet<SubKey> = HashSet::new();

    while let Some(Ok(msg)) = ws_rx.next().await {
        let raw = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
            if !send_json(&out_tx, json!({"type": "error", "error": "invalid_json"})) {
                break;
            }
            continue;
        };

        let ok = match parsed.get("action").and_then(|v| v.as_str()) {
            Some("subscribe") => {
                handle_subscribe(&out_tx, &state, &api_key, &parsed, &mut sub_tasks, &mut subscribed).await
            }
            Some("unsubscribe") => handle_unsubscribe(&out_tx, &parsed, &mut sub_tasks, &mut subscribed),
            Some("ping") => send_json(&out_tx, json!({"type": "pong"})),
            _ => send_json(&out_tx, json!({"type": "error", "error": "unknown_action"})),
        };
        if !ok {
            break;
        }
    }

    for (_, task) in sub_tasks {
        task.abort();
    }
    drop(out_tx);
    let _ = writer.await;
}

async fn handle_subscribe(
    out: &UnboundedSender<Message>,
    state: &AppState,
    api_key: &str,
    msg: &Value,
    sub_tasks: &mut HashMap<SubKey, JoinHandle<()>>,
    subscribed: &mut HashSet<SubKey>,
) -> bool {
    let Some(meetings) = msg.get("meetings").and_then(|v| v.as_array()) else {
        return send_json(out, json!({"type": "error", "error": "invalid_subscribe_payload", "details": "'meetings' must be a non-empty list"}));
    };
    if meetings.is_empty() {
        return send_json(out, json!({"type": "error", "error": "invalid_subscribe_payload", "details": "'meetings' list cannot be empty"}));
    }

    let payload_meetings: Vec<Value> = meetings
        .iter()
        .filter_map(|m| {
            let plat = m.get("platform")?.as_str()?.trim().to_string();
            let nid = m.get("native_id")?.as_str()?.trim().to_string();
            if plat.is_empty() || nid.is_empty() {
                return None;
            }
            Some(json!({"platform": plat, "native_meeting_id": nid}))
        })
        .collect();
    if payload_meetings.is_empty() {
        return send_json(out, json!({"type": "error", "error": "invalid_subscribe_payload", "details": "no valid meeting objects"}));
    }

    let mut auth_headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = api_key.parse() {
        auth_headers.insert("X-API-Key", v);
    }
    if let Some(user_data) = resolve_token(state, api_key).await {
        if let Some(uid) = user_data.get("user_id") {
            if let Ok(v) = uid.to_string().trim_matches('"').parse() {
                auth_headers.insert("x-user-id", v);
            }
        }
        if let Ok(v) = user_scopes(&user_data).join(",").parse() {
            auth_headers.insert("x-user-scopes", v);
        }
        let limits = user_data.get("max_concurrent_bots").and_then(|v| v.as_i64()).unwrap_or(1);
        if let Ok(v) = limits.to_string().parse() {
            auth_headers.insert("x-user-limits", v);
        }
    }

    let url = format!("{}/ws/authorize-subscribe", state.config.transcription_collector_url);
    let resp = match state.http.post(&url).headers(auth_headers).json(&json!({"meetings": payload_meetings})).send().await {
        Ok(r) => r,
        Err(e) => return send_json(out, json!({"type": "error", "error": "authorization_call_failed", "details": e.to_string()})),
    };
    if resp.status() != reqwest::StatusCode::OK {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return send_json(out, json!({"type": "error", "error": "authorization_service_error", "status": status, "detail": detail}));
    }
    let data: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return send_json(out, json!({"type": "error", "error": "authorization_call_failed", "details": e.to_string()})),
    };

    let errors = data.get("errors").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if !errors.is_empty() && !send_json(out, json!({"type": "error", "error": "invalid_subscribe_payload", "details": errors})) {
        return false;
    }

    let authorized = data.get("authorized").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut confirmed = Vec::new();
    for item in authorized {
        let (Some(plat), Some(nid), Some(uid), Some(mid)) = (
            item.get("platform").and_then(|v| v.as_str()),
            item.get("native_id").and_then(|v| v.as_str()),
            value_as_id(item.get("user_id")),
            value_as_id(item.get("meeting_id")),
        ) else {
            continue;
        };
        let key: SubKey = (plat.to_string(), nid.to_string(), uid);
        if !subscribed.contains(&key) {
            subscribed.insert(key.clone());
            let redis_url = state.config.redis_url.clone();
            let out_clone = out.clone();
            sub_tasks.insert(key, tokio::spawn(async move { fan_in(redis_url, mid, out_clone).await }));
        }
        confirmed.push(json!({"platform": plat, "native_id": nid}));
    }

    send_json(out, json!({"type": "subscribed", "meetings": confirmed}))
}

fn value_as_id(v: Option<&Value>) -> Option<String> {
    let v = v?;
    v.as_str().map(String::from).or_else(|| v.as_i64().map(|n| n.to_string()))
}

fn handle_unsubscribe(
    out: &UnboundedSender<Message>,
    msg: &Value,
    sub_tasks: &mut HashMap<SubKey, JoinHandle<()>>,
    subscribed: &mut HashSet<SubKey>,
) -> bool {
    let Some(meetings) = msg.get("meetings").and_then(|v| v.as_array()) else {
        return send_json(out, json!({"type": "error", "error": "invalid_unsubscribe_payload", "details": "'meetings' must be a list"}));
    };

    let mut unsubscribed = Vec::new();
    let mut errors = Vec::new();
    for (idx, m) in meetings.iter().enumerate() {
        let plat = m.get("platform").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let nid = m.get("native_id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if plat.is_empty() || nid.is_empty() {
            errors.push(format!("meetings[{idx}] missing 'platform' or 'native_id'"));
            continue;
        }
        let matching = subscribed.iter().find(|k| k.0 == plat && k.1 == nid).cloned();
        match matching {
            Some(key) => {
                if let Some(task) = sub_tasks.remove(&key) {
                    task.abort();
                }
                subscribed.remove(&key);
                unsubscribed.push(json!({"platform": plat, "native_id": nid}));
            }
            None => errors.push(format!("meetings[{idx}] not currently subscribed")),
        }
    }

    if !errors.is_empty() && unsubscribed.is_empty() {
        return send_json(out, json!({"type": "error", "error": "invalid_unsubscribe_payload", "details": errors}));
    }
    send_json(out, json!({"type": "unsubscribed", "meetings": unsubscribed}))
}

/// Fans Redis pub/sub messages for one meeting straight through to the client socket.
/// Mirrors Python's `fan_in`: subscribes to the mutable-transcript/status/chat channels and
/// forwards every message verbatim until the subscription is cancelled or the socket drops.
async fn fan_in(redis_url: String, meeting_id: String, out: UnboundedSender<Message>) {
    let channels = [
        format!("tc:meeting:{meeting_id}:mutable"),
        format!("bm:meeting:{meeting_id}:status"),
        format!("va:meeting:{meeting_id}:chat"),
    ];
    let client = match redis::Client::open(redis_url) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("fan_in: bad redis url: {e}");
            return;
        }
    };
    let mut pubsub = match client.get_async_pubsub().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("fan_in: pubsub connect failed: {e}");
            return;
        }
    };
    for ch in &channels {
        if let Err(e) = pubsub.subscribe(ch).await {
            tracing::warn!("fan_in: subscribe {ch} failed: {e}");
            return;
        }
    }

    let mut stream = pubsub.on_message();
    while let Some(msg) = stream.next().await {
        let Ok(payload) = msg.get_payload::<String>() else { continue };
        if out.send(Message::Text(payload)).is_err() {
            break;
        }
    }
}
