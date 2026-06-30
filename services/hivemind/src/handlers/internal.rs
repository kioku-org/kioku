use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Json;

use crate::errors::AppError;
use crate::repos::auth::AuthRepo;
use crate::types::{AuthSession, ProvisionRequest};
use crate::AppState;

pub async fn provision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ProvisionRequest>,
) -> Result<Json<AuthSession>, AppError> {
    let secret = headers
        .get("X-Internal-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = if state.settings.internal_secret.is_empty() {
        &state.settings.jwt_secret
    } else {
        &state.settings.internal_secret
    };
    if secret != expected {
        return Err(AppError::Unauthorized("Invalid internal secret".into()));
    }

    let repo = AuthRepo::new(state.db.clone());
    let session = state
        .services
        .auth
        .provision(&repo, &state.settings, req)
        .await?;
    Ok(Json(session))
}
