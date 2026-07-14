use crate::state::AppState;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        FromRequestParts, Path, Request, State,
    },
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{any, delete, get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

const TOUCH_DEBOUNCE: Duration = Duration::from_secs(30);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/b/:token", get(dashboard))
        .route("/b/:token/vnc/*path", any(vnc_http_proxy))
        .route("/b/:token/vnc/websockify", get(vnc_ws_proxy))
        .route("/b/:token/cdp/*path", get(cdp_http_proxy))
        // Bare `/cdp` serves both the CDP HTTP handshake (GET /json/version) and the CDP
        // WebSocket tunnel at the *same path* — mirrors Python's `@app.websocket` vs
        // `@app.api_route`, which dispatch on ASGI scope type rather than method+path. axum
        // has no separate route type for that, so both live in one handler that branches on
        // the `Upgrade` header instead of being registered twice (which would panic at
        // startup: axum forbids two handlers for the identical method+path).
        .route("/b/:token/cdp", get(cdp_root))
        .route("/b/:token/cdp-ws", get(cdp_ws_proxy))
        .route("/b/:token/save", post(save_storage))
        .route("/b/:token/storage", delete(delete_storage))
}

fn guessable_token_rejected(token: &str) -> Option<Response> {
    if token.chars().all(|c| c.is_ascii_digit()) {
        return Some(
            (
                StatusCode::FORBIDDEN,
                "This route requires the unguessable session token, not the guessable meeting id",
            )
                .into_response(),
        );
    }
    None
}

async fn fire_touch(state: &AppState, container_name: &str, backend_url: &str) {
    if !container_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        tracing::warn!("Suspicious container_name in touch: {}", &container_name[..container_name.len().min(20)]);
        return;
    }
    {
        let mut debounce = state.touch_debounce.lock().unwrap();
        if let Some(last) = debounce.get(container_name) {
            if last.elapsed() < TOUCH_DEBOUNCE {
                return;
            }
        }
        debounce.insert(container_name.to_string(), Instant::now());
        // ponytail: unbounded growth guard — stale entries evicted on insert
        debounce.retain(|_, t| t.elapsed() < TOUCH_DEBOUNCE * 10);
    }
    let url = format!("{backend_url}/containers/{container_name}/touch");
    if let Err(e) = state.http.post(&url).timeout(Duration::from_secs(5)).send().await {
        tracing::debug!("touch failed for {container_name}: {e}");
    }
}

/// Resolve a `/b/{token}` session from Redis, firing a keep-alive touch and resolving/caching
/// the container IP if missing (containers may have no DNS name, e.g. under Kubernetes).
async fn resolve_session(state: &AppState, token: &str) -> Option<Value> {
    let raw: Option<String> = {
        let mut redis = state.redis.clone();
        redis::AsyncCommands::get(&mut redis, format!("browser_session:{token}")).await.ok()?
    };
    let mut session: Value = serde_json::from_str(&raw?).ok()?;

    // Which runtime-api instance this session's container actually lives on (set by
    // meeting-api at spawn time). Absent for sessions created before this field existed —
    // falls back to the local backend, same as meeting-api's own resolver does.
    let backend_url = state
        .config
        .runtime_backend_url(session.get("runtime_backend").and_then(|v| v.as_str()))
        .to_string();

    let container_name = session.get("container_name").and_then(|v| v.as_str()).map(str::to_string);
    if let Some(name) = &container_name {
        let state = state.clone();
        let name = name.clone();
        let backend_url = backend_url.clone();
        tokio::spawn(async move { fire_touch(&state, &name, &backend_url).await });
    }

    if session.get("container_ip").and_then(|v| v.as_str()).is_none() {
        if let Some(name) = &container_name {
            let url = format!("{backend_url}/containers/{name}");
            if let Ok(resp) = state.http.get(&url).timeout(Duration::from_secs(5)).send().await {
                if resp.status() == StatusCode::OK {
                    if let Ok(info) = resp.json::<Value>().await {
                        if let Some(ip) = info.get("ip").and_then(|v| v.as_str()) {
                            session["container_ip"] = Value::String(ip.to_string());
                            let mut redis = state.redis.clone();
                            let _: Result<(), _> = redis::AsyncCommands::set_ex(
                                &mut redis,
                                format!("browser_session:{token}"),
                                session.to_string(),
                                86400,
                            )
                            .await;
                        }
                    }
                }
            }
        }
    }

    Some(session)
}

fn session_container(session: &Value) -> String {
    session
        .get("container_ip")
        .and_then(|v| v.as_str())
        .or_else(|| session.get("container_name").and_then(|v| v.as_str()))
        .unwrap_or_default()
        .to_string()
}

/// SECURITY: `{token}` alone is not sufficient — `meeting_id` is registered as a guessable
/// alias for the session, so a valid X-API-Key for the *owning* user is required to attach.
async fn authorize_control(state: &AppState, session: &Value, api_key: &str) -> bool {
    if api_key.is_empty() {
        return false;
    }
    let Some(user) = crate::auth::resolve_token(state, api_key).await else {
        return false;
    };
    let sess_uid = session.get("user_id").map(|v| v.to_string().trim_matches('"').to_string()).unwrap_or_default();
    let uid = user
        .get("id")
        .or_else(|| user.get("user_id"))
        .map(|v| v.to_string().trim_matches('"').to_string())
        .unwrap_or_default();
    !sess_uid.is_empty() && sess_uid == uid
}

fn dashboard_html(token: &str, session: &Value) -> String {
    let meeting_id = session.get("meeting_id").map(|v| v.to_string().trim_matches('"').to_string()).unwrap_or_default();
    let vnc_iframe_url = format!("/b/{token}/vnc/vnc.html?autoconnect=true&resize=scale&reconnect=true&path=b/{token}/vnc/websockify");
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Remote Browser</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    html, body {{ height: 100%; background: #1a1a2e; color: #eee; font-family: system-ui, -apple-system, sans-serif; }}
    .toolbar {{ display: flex; align-items: center; gap: 10px; padding: 8px 16px; background: #0f3460; height: 48px; border-bottom: 1px solid rgba(255,255,255,0.08); }}
    .toolbar h1 {{ font-size: 16px; font-weight: 600; margin-right: auto; white-space: nowrap; }}
    .btn {{ border: none; padding: 7px 16px; border-radius: 4px; cursor: pointer; font-size: 13px; font-weight: 600; color: #fff; transition: background 0.15s; }}
    .btn:disabled {{ opacity: 0.5; cursor: wait; }}
    .btn-green {{ background: #27ae60; }}
    .btn-green:hover:not(:disabled) {{ background: #219a52; }}
    .btn-purple {{ background: #8e44ad; }}
    .btn-purple:hover:not(:disabled) {{ background: #7d3c98; }}
    .btn-blue {{ background: #2980b9; }}
    .btn-blue:hover:not(:disabled) {{ background: #2471a3; }}
    .vnc-frame {{ width: 100%; border: none; display: block; height: calc(100vh - 48px); }}
    .toast {{ position: fixed; top: 60px; right: 20px; background: #16213e; border: 1px solid #0f3460; padding: 12px 20px; border-radius: 6px; max-width: 400px; z-index: 999; font-size: 13px; transition: opacity 0.3s; white-space: pre-line; }}
    .toast.hidden {{ opacity: 0; pointer-events: none; }}
    #storage-panel {{ display: none; position: fixed; top: 48px; right: 0; width: 480px; bottom: 0; background: #16213e; border-left: 1px solid #0f3460; z-index: 100; overflow-y: auto; font-size: 13px; }}
    #storage-panel.open {{ display: block; }}
    .panel-header {{ padding: 12px 16px; background: #0f3460; display: flex; justify-content: space-between; align-items: center; }}
    .panel-header h2 {{ font-size: 15px; }}
    .panel-body {{ padding: 16px; color: #8899aa; }}
  </style>
</head>
<body>
  <div class="toolbar">
    <h1>Remote Browser</h1>
    <button class="btn btn-green" onclick="saveStorage()" id="save-btn">Save Storage</button>
    <button class="btn btn-purple" onclick="toggleAudit()">Storage Audit</button>
    <button class="btn btn-blue" onclick="window.open('/b/{token}/vnc/vnc.html?autoconnect=true&resize=scale&reconnect=true&path=b/{token}/vnc/websockify', '_blank')">Fullscreen</button>
  </div>
  <div class="toast hidden" id="toast"></div>
  <div id="storage-panel">
    <div class="panel-header">
      <h2>Storage Audit</h2>
      <button class="btn" style="background:#555;padding:5px 12px;font-size:12px" onclick="toggleAudit()">Close</button>
    </div>
    <div class="panel-body">
      <p>Storage audit coming soon.</p>
      <p style="margin-top:12px;font-size:12px;color:#556">This panel will show cookies, localStorage, and IndexedDB from the browser session for inspection and debugging.</p>
    </div>
  </div>
  <iframe class="vnc-frame" src="{vnc_iframe_url}" id="vnc-iframe"></iframe>
  <script>
    const TOKEN = "{token}";
    const MEETING_ID = "{meeting_id}";
    const toast = document.getElementById('toast');
    let toastTimer;
    function showToast(msg, ms) {{
      toast.textContent = msg;
      toast.classList.remove('hidden');
      clearTimeout(toastTimer);
      toastTimer = setTimeout(() => toast.classList.add('hidden'), ms || 4000);
    }}
    async function saveStorage() {{
      const btn = document.getElementById('save-btn');
      btn.disabled = true;
      btn.textContent = 'Saving...';
      showToast('Saving browser storage to MinIO...');
      try {{
        const res = await fetch('/b/' + TOKEN + '/save', {{ method: 'POST' }});
        const data = await res.json();
        if (res.ok) {{ showToast(data.message || 'Storage saved!', 5000); }}
        else {{ showToast('Error: ' + (data.detail || res.statusText), 6000); }}
      }} catch (e) {{ showToast('Error: ' + e.message, 6000); }}
      finally {{ btn.disabled = false; btn.textContent = 'Save Storage'; }}
    }}
    function toggleAudit() {{ document.getElementById('storage-panel').classList.toggle('open'); }}
  </script>
</body>
</html>"#
    )
}

async fn dashboard(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    if let Some(rejected) = guessable_token_rejected(&token) {
        return rejected;
    }
    let Some(session) = resolve_session(&state, &token).await else {
        return (StatusCode::NOT_FOUND, "Browser session not found or expired").into_response();
    };
    Html(dashboard_html(&token, &session)).into_response()
}

async fn vnc_http_proxy(
    State(state): State<AppState>,
    Path((token, path)): Path<(String, String)>,
    req: Request<Body>,
) -> Response {
    if let Some(rejected) = guessable_token_rejected(&token) {
        return rejected;
    }
    let Some(session) = resolve_session(&state, &token).await else {
        return (StatusCode::NOT_FOUND, "Browser session not found or expired").into_response();
    };
    let vnc_host = state.config.vnc_host_override.clone().unwrap_or_else(|| session_container(&session));
    let mut target = format!("http://{vnc_host}:6080/{path}");
    if let Some(q) = req.uri().query() {
        target.push('?');
        target.push_str(q);
    }

    let method = req.method().clone();
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in req.headers().iter() {
        let lname = name.as_str().to_lowercase();
        if !["host", "content-length", "transfer-encoding"].contains(&lname.as_str()) {
            headers.insert(name.clone(), value.clone());
        }
    }
    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Failed to read body: {e}")).into_response(),
    };

    match state.http.request(method, &target).headers(headers).body(body).timeout(Duration::from_secs(30)).send().await {
        Ok(resp) => crate::proxy::build_response(resp).await,
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to reach browser container: {e}")).into_response(),
    }
}

async fn vnc_ws_proxy(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    if token.chars().all(|c| c.is_ascii_digit()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(session) = resolve_session(&state, &token).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let vnc_host = state.config.vnc_host_override.clone().unwrap_or_else(|| session_container(&session));
    let upstream_url = format!("ws://{vnc_host}:6080/websockify");
    let backend_url = state
        .config
        .runtime_backend_url(session.get("runtime_backend").and_then(|v| v.as_str()))
        .to_string();

    ws.protocols(["binary"]).on_upgrade(move |socket| async move {
        relay_binary_websocket(&state, socket, &upstream_url, &vnc_host, &backend_url).await;
    })
}

/// Bidirectional byte-proxy shared by VNC and (conceptually) similar raw WS tunnels: relays
/// client<->upstream frames verbatim and fires a keep-alive touch every 60s while connected.
async fn relay_binary_websocket(state: &AppState, socket: WebSocket, upstream_url: &str, container: &str, backend_url: &str) {
    let mut req = match upstream_url.into_client_request() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("bad upstream url {upstream_url}: {e}");
            return;
        }
    };
    req.headers_mut().insert("Sec-WebSocket-Protocol", "binary".parse().unwrap());
    let upstream = match tokio_tungstenite::connect_async(req).await {
        Ok((stream, _)) => stream,
        Err(e) => {
            tracing::warn!("VNC websocket connect failed for {container}: {e}");
            return;
        }
    };
    let (mut up_tx, mut up_rx) = upstream.split();
    let (mut down_tx, mut down_rx) = socket.split();

    let client_to_upstream = async {
        while let Some(Ok(msg)) = down_rx.next().await {
            let forwarded = match msg {
                Message::Binary(data) => tungstenite::Message::Binary(data),
                Message::Text(text) => tungstenite::Message::Text(text),
                Message::Close(_) => break,
                _ => continue,
            };
            if up_tx.send(forwarded).await.is_err() {
                break;
            }
        }
    };
    let upstream_to_client = async {
        while let Some(Ok(msg)) = up_rx.next().await {
            let forwarded = match msg {
                tungstenite::Message::Binary(data) => Message::Binary(data),
                tungstenite::Message::Text(text) => Message::Text(text),
                tungstenite::Message::Close(_) => break,
                _ => continue,
            };
            if down_tx.send(forwarded).await.is_err() {
                break;
            }
        }
    };
    let periodic_touch = async {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            fire_touch(state, container, backend_url).await;
        }
    };

    tokio::select! {
        _ = client_to_upstream => {},
        _ = upstream_to_client => {},
        _ = periodic_touch => {},
    }
}

#[derive(Deserialize)]
struct ApiKeyQuery {
    api_key: Option<String>,
}

async fn proxy_cdp_http(state: &AppState, token: &str, path: &str, headers: &HeaderMap, query: &str) -> Response {
    let Some(session) = resolve_session(state, token).await else {
        return (StatusCode::NOT_FOUND, "Browser session not found").into_response();
    };
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            serde_urlencoded::from_str::<ApiKeyQuery>(query).ok().and_then(|q| q.api_key)
        })
        .unwrap_or_default();
    if !authorize_control(state, &session, &api_key).await {
        return (
            StatusCode::FORBIDDEN,
            "Forbidden: a valid X-API-Key for the owning user is required to attach to this CDP session",
        )
            .into_response();
    }
    let container = session_container(&session);
    let upstream_path = if path.is_empty() { "json/version" } else { path };
    let qs = if query.is_empty() { String::new() } else { format!("?{query}") };
    let resp = match state
        .http
        .get(format!("http://{container}:9223/{upstream_path}{qs}"))
        .header("Host", "localhost")
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("CDP HTTP proxy error: {e}")).into_response(),
    };
    let status = resp.status();
    let body = match resp.text().await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("CDP HTTP proxy error: {e}")).into_response(),
    };

    let host = headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or("localhost:8056");
    let forwarded_proto = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).unwrap_or("");
    let scheme = forwarded_proto.split(',').next().unwrap_or("").trim().to_lowercase();
    let ws_scheme = if scheme == "https" { "wss" } else { "ws" };
    let proxy_ws_url = format!("{ws_scheme}://{host}/b/{token}/cdp");
    let re = Regex::new(r#""webSocketDebuggerUrl":\s*"[^"]*""#).unwrap();
    let rewritten = re.replace(&body, format!(r#""webSocketDebuggerUrl": "{proxy_ws_url}""#).as_str());

    (status, [("content-type", "application/json")], rewritten.into_owned()).into_response()
}

async fn cdp_http_proxy(
    State(state): State<AppState>,
    Path((token, path)): Path<(String, String)>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    proxy_cdp_http(&state, &token, &path, &headers, req.uri().query().unwrap_or("")).await
}

/// Dual-purpose `/b/{token}/cdp`: a plain GET returns the CDP `/json/version` handshake
/// (rewritten to point at our WS proxy); a WebSocket-upgrade GET opens the CDP tunnel.
async fn cdp_root(State(state): State<AppState>, Path(token): Path<String>, req: Request<Body>) -> Response {
    let is_ws_upgrade = req
        .headers()
        .get(axum::http::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if !is_ws_upgrade {
        let headers = req.headers().clone();
        let query = req.uri().query().unwrap_or("").to_string();
        return proxy_cdp_http(&state, &token, "", &headers, &query).await;
    }

    let (mut parts, _body) = req.into_parts();
    let query = parts.uri.query().unwrap_or("").to_string();
    let headers = parts.headers.clone();
    let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(ws) => ws,
        Err(rejection) => return rejection.into_response(),
    };
    cdp_upgrade(ws, state, token, headers, query).await
}

async fn cdp_ws_proxy(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
) -> Response {
    cdp_upgrade(ws, state, token, headers, query.unwrap_or_default()).await
}

/// Shared by `/b/{token}/cdp-ws` and the WS branch of the merged `/b/{token}/cdp` handler:
/// authorizes, discovers the container's live CDP WebSocket URL, and opens the tunnel.
async fn cdp_upgrade(ws: WebSocketUpgrade, state: AppState, token: String, headers: HeaderMap, query: String) -> Response {
    let Some(session) = resolve_session(&state, &token).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| serde_urlencoded::from_str::<ApiKeyQuery>(&query).ok().and_then(|q| q.api_key))
        .unwrap_or_default();
    if !authorize_control(&state, &session, &api_key).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let container = session_container(&session);
    let version_resp = match state
        .http
        .get(format!("http://{container}:9223/json/version"))
        .header("Host", "localhost")
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to discover CDP URL for {container}: {e}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let version_info: Value = match version_resp.json().await {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };
    let raw_ws_url = version_info.get("webSocketDebuggerUrl").and_then(|v| v.as_str()).unwrap_or("");
    if raw_ws_url.is_empty() {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    let re = Regex::new(r"ws://(localhost|127\.0\.0\.1)(:\d+)?/").unwrap();
    let cdp_ws_url = re.replace(raw_ws_url, format!("ws://{container}:9223/")).into_owned();
    let backend_url = state
        .config
        .runtime_backend_url(session.get("runtime_backend").and_then(|v| v.as_str()))
        .to_string();

    ws.on_upgrade(move |socket| async move {
        relay_cdp_websocket(&state, socket, &cdp_ws_url, &container, &backend_url).await;
    })
}

async fn relay_cdp_websocket(state: &AppState, socket: WebSocket, upstream_url: &str, container: &str, backend_url: &str) {
    let mut req = match upstream_url.into_client_request() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("bad CDP url {upstream_url}: {e}");
            return;
        }
    };
    req.headers_mut().insert("Host", "localhost".parse().unwrap());
    let upstream = match tokio_tungstenite::connect_async(req).await {
        Ok((stream, _)) => stream,
        Err(e) => {
            tracing::warn!("CDP websocket connect failed for {container}: {e}");
            return;
        }
    };
    let (mut up_tx, mut up_rx) = upstream.split();
    let (mut down_tx, mut down_rx) = socket.split();

    let client_to_upstream = async {
        while let Some(Ok(msg)) = down_rx.next().await {
            let forwarded = match msg {
                Message::Text(text) => tungstenite::Message::Text(text),
                Message::Close(_) => break,
                _ => continue,
            };
            if up_tx.send(forwarded).await.is_err() {
                break;
            }
        }
    };
    let upstream_to_client = async {
        while let Some(Ok(msg)) = up_rx.next().await {
            let forwarded = match msg {
                tungstenite::Message::Text(text) => Message::Text(text),
                tungstenite::Message::Binary(data) => Message::Binary(data),
                tungstenite::Message::Close(_) => break,
                _ => continue,
            };
            if down_tx.send(forwarded).await.is_err() {
                break;
            }
        }
    };
    let periodic_touch = async {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            fire_touch(state, container, backend_url).await;
        }
    };

    tokio::select! {
        _ = client_to_upstream => {},
        _ = upstream_to_client => {},
        _ = periodic_touch => {},
    }
}

async fn save_storage(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    if let Some(rejected) = guessable_token_rejected(&token) {
        return rejected;
    }
    let Some(session) = resolve_session(&state, &token).await else {
        return (StatusCode::NOT_FOUND, "Browser session not found or expired").into_response();
    };
    let Some(meeting_id) = session.get("meeting_id") else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Session missing meeting_id").into_response();
    };
    let url = format!(
        "{}/internal/browser-sessions/{}/save",
        state.config.meeting_api_url, meeting_id
    );
    match state.http.post(&url).timeout(Duration::from_secs(60)).send().await {
        Ok(resp) if resp.status().is_client_error() || resp.status().is_server_error() => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            (status, text).into_response()
        }
        Ok(resp) => match resp.json::<Value>().await {
            Ok(v) => Json(v).into_response(),
            Err(e) => (StatusCode::BAD_GATEWAY, format!("Invalid response from meeting-api: {e}")).into_response(),
        },
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to reach meeting-api: {e}")).into_response(),
    }
}

async fn delete_storage(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    if let Some(rejected) = guessable_token_rejected(&token) {
        return rejected;
    }
    let Some(session) = resolve_session(&state, &token).await else {
        return (StatusCode::NOT_FOUND, "Browser session not found or expired").into_response();
    };
    let Some(user_id) = session.get("user_id") else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Session missing user_id").into_response();
    };
    let url = format!(
        "{}/internal/browser-sessions/{}/storage",
        state.config.meeting_api_url, user_id
    );
    match state.http.delete(&url).timeout(Duration::from_secs(60)).send().await {
        Ok(resp) if resp.status().is_client_error() || resp.status().is_server_error() => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            (status, text).into_response()
        }
        Ok(resp) => match resp.json::<Value>().await {
            Ok(v) => Json(v).into_response(),
            Err(e) => (StatusCode::BAD_GATEWAY, format!("Invalid response from meeting-api: {e}")).into_response(),
        },
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to reach meeting-api: {e}")).into_response(),
    }
}
