use axum::extract::{Path, State};
use axum::response::Json;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::knowledge::KnowledgeRepo;
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
    let meeting_out = repo.create(auth.workspace_id, req.clone(), now).await?;

    // Embed transcript in the background so the HTTP response returns immediately
    let docs = KnowledgeService::chunk_transcript(&req.transcript, meeting_out.id, req.date);
    let db = state.db.clone();
    let vs = state.vector_store.clone();
    let workspace_id = auth.workspace_id;
    let meeting_id = meeting_out.id;
    tokio::spawn(async move {
        if let Err(e) =
            KnowledgeService::ingest_documents(&db, &vs, workspace_id, meeting_id, &docs).await
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
    let meetings = repo.list(auth.workspace_id).await?;
    Ok(Json(meetings))
}

pub async fn get(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<MeetingOut>, AppError> {
    let repo = MeetingRepo::new(state.db.clone());
    let meeting = repo
        .get(meeting_id, auth.workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Meeting not found".into()))?;
    Ok(Json(meeting))
}

pub async fn transcript(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let repo = KnowledgeRepo::new(state.db.clone());
    let chunks = repo
        .get_meeting_chunks(meeting_id, auth.workspace_id)
        .await?;
    Ok(Json(chunks))
}
