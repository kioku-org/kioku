use axum::extract::{Path, State};
use axum::response::Json;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::auth::AuthRepo;
use crate::repos::invite::InviteRepo;
use crate::types::{InviteCreate, InviteOut};
use crate::AppState;

pub async fn create(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<InviteCreate>,
) -> Result<Json<InviteOut>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let auth_repo = AuthRepo::new(state.db.clone());
    let company = auth_repo.find_company_by_id(auth.company_id).await?;
    if company.is_free_tier() {
        return Err(AppError::Forbidden(
            "Free tier is limited to 1 person — upgrade to Pro to invite teammates".into(),
        ));
    }

    let repo = InviteRepo::new(state.db.clone());
    let now = crate::util::now_ms();
    let invite = repo.create(auth.company_id, req, now).await?;
    Ok(Json(invite))
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Vec<InviteOut>>, AppError> {
    let repo = InviteRepo::new(state.db.clone());
    let invites = repo.list(auth.company_id).await?;
    Ok(Json(invites))
}

pub async fn remove(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<()>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let repo = InviteRepo::new(state.db.clone());
    repo.remove(invite_id, auth.company_id).await?;
    Ok(Json(()))
}
