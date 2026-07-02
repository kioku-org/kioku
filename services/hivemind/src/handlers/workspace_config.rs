use axum::extract::State;
use axum::response::Json;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::workspace_config::WorkspaceConfigRepo;
use crate::types::{WorkspaceConfigOut, WorkspaceConfigPatch};
use crate::AppState;

pub async fn get_config(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<WorkspaceConfigOut>, AppError> {
    let repo = WorkspaceConfigRepo::new(state.db.clone());
    repo.get_config(auth.workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Workspace config not found".into()))
        .map(Json)
}

pub async fn update_config(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(patch): Json<WorkspaceConfigPatch>,
) -> Result<Json<WorkspaceConfigOut>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let repo = WorkspaceConfigRepo::new(state.db.clone());
    let now = crate::util::now_ms();
    let config = repo.update_config(auth.workspace_id, patch, now).await?;
    Ok(Json(config))
}
