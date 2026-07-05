use crate::config::Config;
use crate::handler::KiokuMcpService;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

type McpService = StreamableHttpService<KiokuMcpService, LocalSessionManager>;

pub fn mcp_routes(http: reqwest::Client, config: Arc<Config>) -> Router {
    let session_manager = Arc::new(LocalSessionManager::default());
    let ct = CancellationToken::new();
    let server_config = StreamableHttpServerConfig::default()
        .with_sse_keep_alive(Some(Duration::from_secs(30)))
        .with_sse_retry(Some(Duration::from_secs(5)))
        .with_stateful_mode(true)
        .with_json_response(true)
        .with_cancellation_token(ct);

    let service: McpService = StreamableHttpService::new(
        move || Ok(KiokuMcpService { http: http.clone(), config: config.clone() }),
        session_manager,
        server_config,
    );

    Router::new().nest_service("/mcp", service)
}
