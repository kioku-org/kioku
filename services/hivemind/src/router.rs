use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};
use tower_http::cors::CorsLayer;

use crate::AppState;
use crate::handlers::{
    api_key, auth, company, company_api_key, invite, knowledge, member, meeting, message, session, trace, usage, vexa,
};
use crate::mcp::transport::mcp_routes;

/// Build the application router with all routes and middleware.
pub fn build(state: AppState) -> Router {
    Router::new()
        // Health
        .route("/health", get(|| async {
            axum::Json(serde_json::json!({ "status": "ok" }))
        }))
        // Auth
        .route("/auth/register/admin", post(auth::register_admin))
        .route("/auth/register/personal", post(auth::register_personal))
        .route("/auth/register/member", post(auth::register_member))
        .route("/auth/signin", post(auth::sign_in))
        .route("/auth/signout", post(auth::sign_out))
        .route("/auth/me", get(auth::auth_me))
        .route("/auth/token", post(company_api_key::exchange_api_key))
        // Company
        .route("/company/config", get(company::get_config))
        .route("/company/config", put(company::update_config))
        // Members
        .route("/company/members", get(member::list))
        .route("/company/members/:user_id", delete(member::remove))
        .route("/company/members/:user_id/:role", put(member::update_role))
        // Invites
        .route("/company/invites", get(invite::list))
        .route("/company/invites", post(invite::create))
        .route("/company/invites/:invite_id", delete(invite::remove))
        // API Keys (provider)
        .route("/company/apikeys/:user_id", get(api_key::list))
        .route("/company/apikeys", post(api_key::set))
        .route("/company/apikeys/key/:key_id", delete(api_key::delete))
        // Company API Keys (CLI auth)
        .route("/company/auth-keys", get(company_api_key::list_api_keys))
        .route("/company/auth-keys", post(company_api_key::create_api_key))
        .route("/company/auth-keys/:key_id", delete(company_api_key::delete_api_key))
        // Meetings
        .route("/meetings", get(meeting::list))
        .route("/meetings", post(meeting::ingest))
        // Knowledge
        .route("/knowledge/search", post(knowledge_search))
        .route("/knowledge/documents", get(knowledge::list_documents))
        .route("/knowledge/documents", post(knowledge::upload_pdf))
        .route("/knowledge/documents/:document_id", delete(knowledge::delete_document))
        // Sessions
        .route("/sessions", get(session::list))
        .route("/sessions", post(session::create))
        .route("/sessions/:session_id", get(session::get))
        .route("/sessions/:session_id", patch(session::update))
        .route("/sessions/:session_id", delete(session::delete))
        // Messages
        .route("/sessions/:session_id/messages", get(message::list))
        .route("/sessions/:session_id/messages", post(message::create))
        // Traces
        .route("/sessions/:session_id/traces", get(trace::list))
        .route("/sessions/:session_id/traces", post(trace::create))
        .route("/sessions/:session_id/traces/:trace_id", patch(trace::update))
        // Usage
        .route("/usage", post(usage::record))
        .route("/usage/summary", get(usage::summary))
        // Vexa
        .route("/vexa/bots", post(vexa::request_bot))
        .route("/vexa/meetings", get(vexa::get_meetings))
        // MCP
        .merge(mcp_routes(&state))
        .with_state(state)
        .layer(CorsLayer::permissive())
}

async fn knowledge_search(
    axum::extract::State(state): axum::extract::State<AppState>,
    auth: crate::middleware::AuthContext,
    axum::Json(req): axum::Json<crate::types::KnowledgeSearchRequest>,
) -> Result<axum::Json<Vec<crate::types::KnowledgeSearchResult>>, crate::errors::AppError> {
    if req.query.trim().is_empty() {
        return Ok(axum::Json(vec![]));
    }
    let limit = req.limit.max(1);
    let results = crate::services::knowledge::KnowledgeService::search(
        &state.db,
        &state.vector_store,
        auth.company_id,
        &req.query,
        limit,
    )
    .await?;
    Ok(axum::Json(results))
}
