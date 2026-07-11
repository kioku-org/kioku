mod app;
mod auth;
mod clients;
mod config;
mod error;
mod tools;
mod transport;

use axum::{routing::get, Json, Router};
use config::Config;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Arc::new(Config::from_env());
    let http = reqwest::Client::new();
    let port = config.port;

    let app = Router::new()
        .route("/health", get(health))
        .merge(transport::mcp_routes(http, config))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("mcp service istening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
