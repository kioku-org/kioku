use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    routing::{get, post},
    Router,
};
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

pub fn mcp_routes(state: &AppState) -> Router<AppState> {
    let mcp_service = create_mcp_service(state);
    Router::new()
        .route("/mcp", post(mcp_handler))
        .route("/mcp", get(mcp_handler))
        .with_state(mcp_service)
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

    StreamableHttpService::new(
        move || {
            Ok(KiokuMcpService {
                db: db.clone(),
                vector_store: vector_store.clone(),
            })
        },
        session_manager,
        config,
    )
}

async fn mcp_handler(
    State(service): State<McpService>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let response = service
        .oneshot(req)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (parts, body) = response.into_parts();
    let body = Body::new(body);
    Ok(Response::from_parts(parts, body))
}
