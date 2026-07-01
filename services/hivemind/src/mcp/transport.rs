use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    routing::{get, post},
    Router,
};
use http_body_util::BodyExt;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use crate::mcp::handler::KiokuMcpService;
use crate::AppState;

type McpService = StreamableHttpService<KiokuMcpService, LocalSessionManager>;

struct McpState {
    service: McpService,
    jwt_secret: String,
    db: sqlx::PgPool,
}

pub fn mcp_routes(state: &AppState) -> Router<AppState> {
    let mcp_state = Arc::new(McpState {
        service: create_mcp_service(state),
        jwt_secret: state.settings.jwt_secret.clone(),
        db: state.db.clone(),
    });
    Router::new()
        .route("/mcp", post(mcp_handler))
        .route("/mcp", get(mcp_handler))
        .with_state(mcp_state)
}

fn create_mcp_service(state: &AppState) -> McpService {
    let db = state.db.clone();
    let vector_store = state.vector_store.clone();
    let session_manager = Arc::new(LocalSessionManager::default());
    let ct = CancellationToken::new();

    let config = StreamableHttpServerConfig::default()
        .with_sse_keep_alive(Some(Duration::from_secs(30)))
        .with_sse_retry(Some(Duration::from_secs(5)))
        .with_stateful_mode(true)
        .with_json_response(true)
        .with_cancellation_token(ct)
        .with_allowed_hosts(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "0.0.0.0".to_string(),
            "api.coolcmyk.dev".to_string(),
            "api.kioku.chat".to_string(),
        ])
        .with_allowed_origins(vec![
            "https://api.coolcmyk.dev".to_string(),
            "https://api.kioku.chat".to_string(),
            "http://localhost:9100".to_string(),
        ]);

    let jwt_secret_for_factory = state.settings.jwt_secret.clone();
    StreamableHttpService::new(
        move || {
            Ok(KiokuMcpService {
                db: db.clone(),
                vector_store: vector_store.clone(),
                jwt_secret: jwt_secret_for_factory.clone(),
            })
        },
        session_manager,
        config,
    )
}

/// Extract Bearer token from Authorization header.
fn bearer_token(req: &Request<Body>) -> Option<String> {
    let auth = req.headers().get("authorization")?.to_str().ok()?;
    Some(auth.strip_prefix("Bearer ")?.to_string())
}

/// Resolve company_id + user_id from a Bearer token.
/// Tries JWT decode first (fast); falls back to auth_tokens DB lookup for API keys.
async fn resolve_auth_ids(
    jwt_secret: &str,
    db: &sqlx::PgPool,
    token: &str,
) -> Option<(String, String)> {
    use crate::repos::auth::validate_token;

    if let Ok(claims) = validate_token(jwt_secret, token) {
        return Some((claims.company_id.to_string(), claims.user_id.to_string()));
    }

    // Non-JWT token (e.g. an exchanged kioku_xxx API key session) — look up in auth_tokens.
    let row: Option<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
        "SELECT company_id, user_id FROM auth_tokens WHERE token = $1 LIMIT 1",
    )
    .bind(token)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    row.map(|(company_id, user_id)| (company_id.to_string(), user_id.to_string()))
}

/// For MCP initialize requests, inject company_id + user_id into params._meta
/// so handlers can read them via context.peer.peer_info().meta.
async fn inject_auth_into_init(
    jwt_secret: &str,
    db: &sqlx::PgPool,
    req: Request<Body>,
) -> Result<Request<Body>, StatusCode> {
    let token = bearer_token(&req);
    let auth_ids = match token {
        Some(ref t) => resolve_auth_ids(jwt_secret, db, t).await,
        None => None,
    };

    // Only body-rewrite POST requests; GET (SSE) passes through unchanged.
    if req.method() != axum::http::Method::POST {
        return Ok(req);
    }

    let (parts, body) = req.into_parts();
    let bytes = body
        .collect()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_bytes();

    let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        // Not JSON — reconstruct and pass through.
        let new_body = Body::from(bytes);
        return Ok(Request::from_parts(parts, new_body));
    };

    // Only rewrite initialize requests.
    if json.get("method").and_then(|m| m.as_str()) == Some("initialize") {
        if let Some((company_id, user_id)) = auth_ids {
            // peer_info() returns InitializeRequestParams whose _meta is at params._meta
            let existing_meta = json
                .pointer_mut("/params/_meta")
                .and_then(|v| v.as_object_mut());

            if let Some(meta_obj) = existing_meta {
                meta_obj
                    .entry("company_id")
                    .or_insert(serde_json::Value::String(company_id.clone()));
                meta_obj
                    .entry("user_id")
                    .or_insert(serde_json::Value::String(user_id.clone()));
            } else if let Some(params) = json.pointer_mut("/params") {
                if let Some(obj) = params.as_object_mut() {
                    obj.insert(
                        "_meta".to_string(),
                        serde_json::json!({
                            "company_id": company_id,
                            "user_id": user_id,
                        }),
                    );
                }
            }
        }
    }

    let new_bytes = serde_json::to_vec(&json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut new_req = Request::from_parts(parts, Body::from(new_bytes));
    // Update Content-Length to match rewritten body.
    new_req.headers_mut().remove("content-length");
    Ok(new_req)
}

async fn mcp_handler(
    State(state): State<Arc<McpState>>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let req = inject_auth_into_init(&state.jwt_secret, &state.db, req).await?;

    let response = state
        .service
        .clone()
        .oneshot(req)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}
