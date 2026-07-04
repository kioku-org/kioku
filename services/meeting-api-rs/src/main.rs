mod auth;
mod config;
mod db;
mod handlers;
mod models;
mod runtime_backend;
mod state;

use axum::{
    routing::{delete, get, post},
    Json, Router,
};
use config::Config;
use state::AppState;
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

    let config = Arc::new(Config::from_env()?);
    let db = db::connect(&config).await?;
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let redis = redis::aio::ConnectionManager::new(redis_client).await?;
    let http = reqwest::Client::new();

    let state = AppState { db, redis, http, config: config.clone() };

    let app = Router::new()
        .route("/health", get(health))
        .route("/bots", post(handlers::meetings::request_bot))
        .route("/bots/status", get(handlers::meetings::get_bots_status))
        .route("/bots/id/:meeting_id", get(handlers::meetings::get_bot_by_id))
        .route("/bots/:platform/:native_meeting_id", delete(handlers::meetings::stop_bot))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", std::env::var("PORT").unwrap_or_else(|_| "8080".to_string()));
    tracing::info!("kioku-meeting-api listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
