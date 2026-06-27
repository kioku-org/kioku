use axum::extract::State;
use axum::response::Json;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::meeting::MeetingRepo;
use crate::services::knowledge::KnowledgeService;
use crate::types::{MeetingIngestRequest, MeetingOut};
use crate::AppState;

pub async fn ingest(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<MeetingIngestRequest>,
) -> Result<Json<MeetingOut>, AppError> {
    let repo = MeetingRepo::new(state.db.clone());
    let now = crate::util::now_ms();
    let meeting_out = repo.create(auth.company_id, req.clone(), now).await?;

    // Embed transcript in the background so the HTTP response returns immediately
    let docs = KnowledgeService::chunk_transcript(&req.transcript, meeting_out.id, req.date);
    let db = state.db.clone();
    let vs = state.vector_store.clone();
    let company_id = auth.company_id;
    let meeting_id = meeting_out.id;
    tokio::spawn(async move {
        if let Err(e) =
            KnowledgeService::ingest_documents(&db, &vs, company_id, meeting_id, &docs).await
        {
            tracing::error!(error = %e, "Failed to ingest knowledge chunks into vector store");
        }
    });

    Ok(Json(meeting_out))
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Vec<MeetingOut>>, AppError> {
    let repo = MeetingRepo::new(state.db.clone());
    let meetings = repo.list(auth.company_id).await?;
    Ok(Json(meetings))
}
