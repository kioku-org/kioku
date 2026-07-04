mod auth;
mod classifier;
mod collector_pipeline;
mod config;
mod container_stop_outbox;
mod db;
mod dispatch_check;
mod handlers;
mod internal_auth;
mod meeting_status;
mod models;
mod outbound_events;
mod post_meeting;
mod runtime_backend;
mod schemas;
mod state;
mod storage;
mod sweeps;
mod webhook_delivery;
mod webhook_url;
mod webhooks;

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

    // Collector: real-time transcription ingestion pipeline. Three supervised background
    // tasks, matching main.py's startup wiring exactly.
    tokio::spawn(collector_pipeline::process_redis_to_postgres(state.clone()));
    tokio::spawn(collector_pipeline::consume_redis_stream(state.clone()));
    tokio::spawn(collector_pipeline::consume_speaker_events_stream(state.clone()));

    // Sweep loop: container-stop outbox consumer (the only thing allowed to fire runtime-api
    // DELETE for a delayed stop) + stale-stopping reconciliation. 60s cadence matches sweeps.py.
    {
        let mut redis = state.redis.clone();
        let http = state.http.clone();
        let sweep_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let http = http.clone();
                let result = container_stop_outbox::consume_pending_stops(&mut redis, move |container_name, backend_url| {
                    let http = http.clone();
                    async move { handlers::meetings::stop_via_runtime_api(http, backend_url, container_name).await }
                })
                .await;
                if result.processed > 0 {
                    tracing::info!(processed = result.processed, succeeded = result.succeeded, retried = result.retried, dlq = result.dlq, "[stop-outbox] sweep pass complete");
                }
                sweeps::run_sweep_iteration(&sweep_state).await;
            }
        });
    }

    let internal_callback_routes = Router::new()
        .route("/bots/internal/callback/started", post(handlers::callbacks::bot_startup_callback))
        .route("/bots/internal/callback/joining", post(handlers::callbacks::bot_joining_callback))
        .route("/bots/internal/callback/awaiting_admission", post(handlers::callbacks::bot_awaiting_admission_callback))
        .route("/bots/internal/callback/exited", post(handlers::callbacks::bot_exit_callback))
        .route("/bots/internal/callback/status_change", post(handlers::callbacks::bot_status_change_callback))
        .route("/internal/transcripts/:meeting_id", get(handlers::collector::get_transcript_internal))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), internal_auth::require_internal_secret));

    let app = Router::new()
        .route("/health", get(health))
        .route("/bots", post(handlers::meetings::request_bot))
        .route("/bots/status", get(handlers::meetings::get_bots_status))
        .route("/bots/id/:meeting_id", get(handlers::meetings::get_bot_by_id))
        .route("/bots/:platform/:native_meeting_id", delete(handlers::meetings::stop_bot))
        .route("/recordings", get(handlers::recordings::list_recordings))
        .route("/recordings/:recording_id", get(handlers::recordings::get_recording).delete(handlers::recordings::delete_recording))
        .route("/recordings/:recording_id/media/:media_file_id/download", get(handlers::recordings::download_media_file))
        .route("/bots/:platform/:native_meeting_id/speak", post(handlers::voice_agent::bot_speak).delete(handlers::voice_agent::bot_speak_stop))
        .route("/bots/:platform/:native_meeting_id/chat", post(handlers::voice_agent::bot_chat_send).get(handlers::voice_agent::bot_chat_read))
        .route("/bots/:platform/:native_meeting_id/screen", post(handlers::voice_agent::bot_screen_show).delete(handlers::voice_agent::bot_screen_stop))
        .route("/bots/:platform/:native_meeting_id/avatar", axum::routing::put(handlers::voice_agent::bot_avatar_set).delete(handlers::voice_agent::bot_avatar_reset))
        .route("/bots/:platform/:native_meeting_id/events", get(handlers::voice_agent::bot_events))
        .route("/meetings", get(handlers::collector::get_meetings))
        .route(
            "/meetings/:platform/:native_meeting_id",
            axum::routing::patch(handlers::collector::update_meeting_data).delete(handlers::collector::delete_meeting),
        )
        .route("/transcripts/:platform/:native_meeting_id", get(handlers::collector::get_transcript_by_native_id))
        .merge(internal_callback_routes)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", std::env::var("PORT").unwrap_or_else(|_| "8080".to_string()));
    tracing::info!("kioku-meeting-api listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
