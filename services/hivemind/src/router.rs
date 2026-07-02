use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};
use tower_http::cors::CorsLayer;

use crate::handlers::{
    auth, internal, invite, knowledge, meeting, member, message, session, trace, vexa, workspace,
    workspace_api_key, workspace_config,
};
use crate::mcp::transport::mcp_routes;
use crate::AppState;

/// Build the application router with all routes and middleware.
pub fn build(state: AppState) -> Router {
    Router::new()
        // Health
        .route(
            "/health",
            get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }),
        )
        // Internal (dashboard → hivemind, protected by X-Internal-Secret)
        .route("/internal/provision", post(internal::provision))
        // Auth
        .route("/auth/register/admin", post(auth::register_admin))
        .route("/auth/register/personal", post(auth::register_personal))
        .route("/auth/register/member", post(auth::register_member))
        .route("/auth/signin", post(auth::sign_in))
        .route("/auth/signout", post(auth::sign_out))
        .route("/auth/me", get(auth::auth_me))
        .route("/auth/token", post(workspace_api_key::exchange_api_key))
        // Workspace config (the caller's active workspace)
        .route("/workspace/config", get(workspace_config::get_config))
        .route("/workspace/config", put(workspace_config::update_config))
        // Members
        .route("/workspace/members", get(member::list))
        .route("/workspace/members/:user_id", delete(member::remove))
        .route(
            "/workspace/members/:user_id/:role",
            put(member::update_role),
        )
        // Invites (targets the caller's active workspace)
        .route("/workspace/invites", get(invite::list))
        .route("/workspace/invites", post(invite::create))
        .route("/workspace/invites/:invite_id", delete(invite::remove))
        // Workspaces (multi-workspace-per-user: list/create/switch, and
        // inviting into a specific non-active workspace by id or slug)
        .route("/workspaces", get(workspace::list))
        .route("/workspaces", post(workspace::create))
        .route(
            "/workspaces/:workspace_id/invites",
            post(workspace::create_invite),
        )
        // Workspace API Keys (CLI auth)
        .route(
            "/workspace/auth-keys",
            get(workspace_api_key::list_api_keys),
        )
        .route(
            "/workspace/auth-keys",
            post(workspace_api_key::create_api_key),
        )
        .route(
            "/workspace/auth-keys/:key_id",
            delete(workspace_api_key::delete_api_key),
        )
        // Meetings
        .route("/meetings", get(meeting::list))
        .route("/meetings", post(meeting::ingest))
        .route("/meetings/:meeting_id", get(meeting::get))
        .route("/meetings/:meeting_id/transcript", get(meeting::transcript))
        // Knowledge
        .route("/knowledge/search", post(knowledge_search))
        .route("/knowledge/documents", get(knowledge::list_documents))
        .route("/knowledge/documents", post(knowledge::upload_pdf))
        .route(
            "/knowledge/documents/:document_id",
            delete(knowledge::delete_document),
        )
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
        .route(
            "/sessions/:session_id/traces/:trace_id",
            patch(trace::update),
        )
        // Vexa
        .route("/vexa/bots", post(vexa::request_bot))
        .route("/vexa/bots/status", get(vexa::get_bots_status))
        .route(
            "/vexa/bots/:platform/:native_meeting_id",
            delete(vexa::stop_bot),
        )
        .route("/vexa/meetings", get(vexa::get_meetings))
        .route("/vexa/token", get(vexa::get_vexa_token))
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
        auth.workspace_id,
        &req.query,
        limit,
    )
    .await?;
    Ok(axum::Json(results))
}
