use axum::extract::{Path, State};
use axum::response::Json;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::member::MemberRepo;
use crate::types::MemberOut;
use crate::AppState;

pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Vec<MemberOut>>, AppError> {
    let repo = MemberRepo::new(state.db.clone());
    let members = repo.list(auth.workspace_id).await?;
    Ok(Json(members))
}

pub async fn remove(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(user_id): Path<Uuid>,
) -> Result<Json<()>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let repo = MemberRepo::new(state.db.clone());
    repo.remove(user_id, auth.workspace_id).await?;
    Ok(Json(()))
}

pub async fn update_role(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((user_id, role)): Path<(Uuid, String)>,
) -> Result<Json<()>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let repo = MemberRepo::new(state.db.clone());
    repo.update_role(user_id, auth.workspace_id, &role).await?;
    Ok(Json(()))
}
