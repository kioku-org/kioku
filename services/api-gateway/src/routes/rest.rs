use crate::proxy::{self, build_response};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Json, Router,
};

async fn root() -> impl IntoResponse {
    Json(serde_json::json!({ "message": "Welcome to the Vexa API Gateway" }))
}

/// Plain liveness check for the container healthcheck (`docker-compose.stateful.yml` curls
/// this path) — the Python version never had one, so the healthcheck has been silently
/// failing in production; this just matches what `cookie`/`router` already do.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

fn not_configured(name: &str) -> Response {
    (StatusCode::NOT_IMPLEMENTED, format!("{name} not configured")).into_response()
}

/// Generic `/admin/{path}` forwarder — admin-api validates X-Admin-API-Key itself.
async fn forward_admin(State(state): State<AppState>, Path(path): Path<String>, req: Request<Body>) -> Response {
    let url = format!("{}/admin/{}", state.config.admin_api_url, path);
    proxy::forward(&state, req, url, false).await
}

/// GET /auth/me — identity of the caller based on their API key.
async fn auth_me(State(state): State<AppState>, req: Request<Body>) -> Response {
    let api_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let Some(api_key) = api_key else {
        return (StatusCode::UNAUTHORIZED, "Missing API key").into_response();
    };
    let Some(user_data) = crate::auth::resolve_token(&state, &api_key).await else {
        return (StatusCode::UNAUTHORIZED, "Invalid API key").into_response();
    };
    Json(serde_json::json!({
        "user_id": user_data.get("user_id"),
        "email": user_data.get("email").and_then(|v| v.as_str()).unwrap_or(""),
        "scopes": user_data.get("scopes").cloned().unwrap_or(serde_json::json!([])),
        "max_concurrent": user_data.get("max_concurrent").and_then(|v| v.as_i64()).unwrap_or(1),
    }))
    .into_response()
}

/// MCP transport header shaping shared by `/mcp` and `/mcp/{path}`.
async fn forward_mcp(state: &AppState, req: Request<Body>, target_path: &str) -> Response {
    let method = req.method().clone();
    let orig_headers = req.headers().clone();
    let query = req.uri().query().map(str::to_string);
    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Failed to read body: {e}")).into_response(),
    };

    let mut headers = reqwest::header::HeaderMap::new();
    let auth = orig_headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| orig_headers.get("x-api-key").and_then(|v| v.to_str().ok()).map(str::to_string));
    if let Some(auth) = &auth {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(auth) {
            headers.insert(reqwest::header::AUTHORIZATION, v);
        }
    }
    if method == axum::http::Method::GET {
        headers.insert(reqwest::header::ACCEPT, reqwest::header::HeaderValue::from_static("text/event-stream"));
    } else {
        headers.insert(reqwest::header::ACCEPT, reqwest::header::HeaderValue::from_static("application/json"));
        if matches!(method, axum::http::Method::POST | axum::http::Method::PUT | axum::http::Method::PATCH) {
            headers.insert(reqwest::header::CONTENT_TYPE, reqwest::header::HeaderValue::from_static("application/json"));
        }
    }
    let excluded = ["host", "content-length", "transfer-encoding", "accept", "authorization", "x-api-key"];
    for (name, value) in orig_headers.iter() {
        if !excluded.contains(&name.as_str()) {
            headers.insert(name.clone(), value.clone());
        }
    }

    let mut url = format!("{}{}", state.config.mcp_url, target_path);
    if let Some(q) = query {
        url.push('?');
        url.push_str(&q);
    }

    let resp = match state.http.request(method.clone(), &url).headers(headers).body(body).send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, format!("MCP service unavailable: {e}")).into_response(),
    };

    // Some MCP servers answer the initial GET handshake with a 400 JSON-RPC error while still
    // providing a valid mcp-session-id header; many clients treat non-2xx as fatal, so mask it.
    let has_session = resp.headers().contains_key("mcp-session-id");
    let status = resp.status();
    let is_handshake_quirk = method == axum::http::Method::GET && status == StatusCode::BAD_REQUEST && has_session;
    if is_handshake_quirk {
        let body = resp.bytes().await.unwrap_or_default();
        if body.windows(b"Missing session ID".len()).any(|w| w == b"Missing session ID") {
            return (StatusCode::OK, body).into_response();
        }
        return (status, body).into_response();
    }
    build_response(resp).await
}

async fn mcp_root(State(state): State<AppState>, req: Request<Body>) -> Response {
    forward_mcp(&state, req, "/mcp").await
}

async fn mcp_path(State(state): State<AppState>, Path(path): Path<String>, req: Request<Body>) -> Response {
    let target = format!("/mcp/{path}");
    forward_mcp(&state, req, &target).await
}

async fn synthetic_test(State(state): State<AppState>, Path(path): Path<String>, req: Request<Body>) -> Response {
    if state.config.is_production {
        return StatusCode::NOT_FOUND.into_response();
    }
    let url = format!("{}/bots/internal/test/{}", state.config.meeting_api_url, path);
    proxy::forward(&state, req, url, false).await
}

async fn synthetic_callback(State(state): State<AppState>, Path(path): Path<String>, req: Request<Body>) -> Response {
    if state.config.is_production {
        return StatusCode::NOT_FOUND.into_response();
    }
    let url = format!("{}/bots/internal/callback/{}", state.config.meeting_api_url, path);
    proxy::forward(&state, req, url, false).await
}

/// Registers every mechanical "forward this request to a fixed downstream path" route.
/// Path params are substituted into the target URL; everything else (auth, scopes, body,
/// headers) is handled uniformly by `proxy::forward`.
pub fn router(state: &AppState) -> Router<AppState> {
    let meeting = state.config.meeting_api_url.clone();
    let admin = state.config.admin_api_url.clone();
    let collector = state.config.transcription_collector_url.clone();
    let calendar = state.config.calendar_service_url.clone();
    let agent = state.config.agent_api_url.clone();

    macro_rules! fwd {
        ($base:expr, $path_tpl:expr) => {{
            let base = $base.clone();
            move |State(state): State<AppState>, req: Request<Body>| {
                let url = format!("{}{}", base, $path_tpl);
                async move { proxy::forward(&state, req, url, true).await }
            }
        }};
    }

    macro_rules! fwd1 {
        ($base:expr, $fmt:expr) => {{
            let base = $base.clone();
            move |State(state): State<AppState>, Path(a): Path<String>, req: Request<Body>| {
                let url = format!("{}{}", base, format!($fmt, a));
                async move { proxy::forward(&state, req, url, true).await }
            }
        }};
    }

    macro_rules! fwd2 {
        ($base:expr, $fmt:expr) => {{
            let base = $base.clone();
            move |State(state): State<AppState>, Path((a, b)): Path<(String, String)>, req: Request<Body>| {
                let url = format!("{}{}", base, format!($fmt, a, b));
                async move { proxy::forward(&state, req, url, true).await }
            }
        }};
    }

    let mut router = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/extension/sessions", post(fwd!(meeting, "/extension/sessions")))
        .route("/extension/sessions/end", post(fwd!(meeting, "/extension/sessions/end")))
        .route("/bots", get(fwd!(meeting, "/bots")).post(fwd!(meeting, "/bots")))
        .route("/bots/status", get(fwd!(meeting, "/bots/status")))
        .route("/bots/id/:meeting_id", get(fwd1!(meeting, "/bots/id/{}")))
        .route("/bots/:platform/:native_meeting_id", delete(fwd2!(meeting, "/bots/{}/{}")))
        .route("/bots/:platform/:native_meeting_id/config", put(fwd2!(meeting, "/bots/{}/{}/config")))
        .route(
            "/bots/:platform/:native_meeting_id/speak",
            post(fwd2!(meeting, "/bots/{}/{}/speak")).delete(fwd2!(meeting, "/bots/{}/{}/speak")),
        )
        .route(
            "/bots/:platform/:native_meeting_id/chat",
            post(fwd2!(meeting, "/bots/{}/{}/chat")).get(fwd2!(meeting, "/bots/{}/{}/chat")),
        )
        .route(
            "/bots/:platform/:native_meeting_id/screen",
            post(fwd2!(meeting, "/bots/{}/{}/screen")).delete(fwd2!(meeting, "/bots/{}/{}/screen")),
        )
        .route(
            "/bots/:platform/:native_meeting_id/avatar",
            put(fwd2!(meeting, "/bots/{}/{}/avatar")).delete(fwd2!(meeting, "/bots/{}/{}/avatar")),
        )
        .route("/recordings", get(fwd!(meeting, "/recordings")))
        .route(
            "/recordings/:id",
            get(fwd1!(meeting, "/recordings/{}")).delete(fwd1!(meeting, "/recordings/{}")),
        )
        .route("/recordings/:id/master", get(fwd1!(meeting, "/recordings/{}/master")))
        .route("/recordings/:id/media/:media_id/download", get(fwd2!(meeting, "/recordings/{}/media/{}/download")))
        .route("/recordings/:id/media/:media_id/raw", get(fwd2!(meeting, "/recordings/{}/media/{}/raw")))
        .route(
            "/recording-config",
            get(fwd!(meeting, "/recording-config")).put(fwd!(meeting, "/recording-config")),
        )
        // NB: segment named `:platform` here even though it's really a meeting_id — axum's
        // router requires sibling routes at the same path position to share one param name
        // (it conflicts with `/meetings/:platform/:id` below otherwise). The name is unused;
        // only position matters, see `fwd1!`/`fwd2!`.
        .route("/meetings/:platform/transcribe", post(fwd1!(meeting, "/meetings/{}/transcribe")))
        .route("/meetings/:platform", get(fwd1!(meeting, "/bots/id/{}")))
        .route("/meetings", get(fwd!(collector, "/meetings")))
        .route("/transcripts/:platform/:id", get(fwd2!(collector, "/transcripts/{}/{}")))
        .route(
            "/meetings/:platform/:id",
            patch(fwd2!(collector, "/meetings/{}/{}")).delete(fwd2!(collector, "/meetings/{}/{}")),
        )
        .route("/user/webhook", put(fwd!(admin, "/user/webhook")))
        .route(
            "/user/workspace-git",
            put(fwd!(admin, "/user/workspace-git")).delete(fwd!(admin, "/user/workspace-git")),
        )
        .route(
            "/admin/*path",
            get(forward_admin)
                .post(forward_admin)
                .put(forward_admin)
                .delete(forward_admin)
                .patch(forward_admin),
        )
        .route("/mcp", get(mcp_root).post(mcp_root).put(mcp_root).delete(mcp_root).patch(mcp_root))
        .route("/mcp/*path", get(mcp_path).post(mcp_path).put(mcp_path).delete(mcp_path).patch(mcp_path))
        .route("/auth/me", get(auth_me))
        .route(
            "/bots/internal/test/*path",
            get(synthetic_test).post(synthetic_test).put(synthetic_test).delete(synthetic_test),
        )
        .route("/bots/internal/callback/*path", post(synthetic_callback));

    if let Some(calendar) = calendar {
        router = router
            .route("/calendar/connect", post(fwd!(calendar, "/calendar/connect")))
            .route("/calendar/status", get(fwd!(calendar, "/calendar/status")))
            .route("/calendar/disconnect", delete(fwd!(calendar, "/calendar/disconnect")))
            .route("/calendar/events", get(fwd!(calendar, "/calendar/events")))
            .route("/calendar/preferences", put(fwd!(calendar, "/calendar/preferences")));
    } else {
        router = router
            .route("/calendar/connect", post(|| async { not_configured("Calendar service") }))
            .route("/calendar/status", get(|| async { not_configured("Calendar service") }))
            .route("/calendar/disconnect", delete(|| async { not_configured("Calendar service") }))
            .route("/calendar/events", get(|| async { not_configured("Calendar service") }))
            .route("/calendar/preferences", put(|| async { not_configured("Calendar service") }));
    }

    if let Some(agent) = agent {
        router = router
            .route("/api/chat", delete(fwd!(agent, "/api/chat")))
            .route("/api/chat/reset", post(fwd!(agent, "/api/chat/reset")))
            .route("/api/sessions", get(fwd!(agent, "/api/sessions")).post(fwd!(agent, "/api/sessions")))
            .route(
                "/api/sessions/:id",
                put(fwd1!(agent, "/api/sessions/{}")).delete(fwd1!(agent, "/api/sessions/{}")),
            );
    }

    router
}
