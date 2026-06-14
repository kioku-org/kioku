use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::auth::AuthRepo;
use crate::repos::company_api_key::CompanyApiKeyRepo;
use crate::types::{AuthSession, CompanyApiKeyCreate, CompanyApiKeyOut};

fn extract_api_key(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing X-API-Key header".into()))
}

pub async fn exchange_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthSession>, AppError> {
    let raw_key = extract_api_key(&headers)?;

    if !raw_key.starts_with("cmp_") {
        return Err(AppError::Unauthorized("Invalid API key format".into()));
    }

    let key_prefix = &raw_key[..12];

    let key_repo = CompanyApiKeyRepo::new(state.db.clone());
    let key_record = key_repo
        .find_by_key_prefix(key_prefix)
        .await?
        .ok_or_else(|| AppError::Unauthorized("API key not found".into()))?;

    let valid = bcrypt::verify(raw_key, &key_record.key_hash)
        .map_err(|e| AppError::Internal(e.into()))?;

    if !valid {
        return Err(AppError::Unauthorized("Invalid API key".into()));
    }

    let now = crate::util::now_ms();
    let _ = key_repo.update_last_used(key_record.id, now).await;

    let auth_repo = AuthRepo::new(state.db.clone());
    let ctx = auth_repo
        .find_user_context(key_record.user_id, key_record.company_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    let token = crate::repos::auth::create_token(
        &state.settings.jwt_secret,
        key_record.user_id,
        key_record.company_id,
        &ctx.role,
        state.settings.jwt_ttl_seconds,
    )?;

    let expires_at = now + (state.settings.jwt_ttl_seconds * 1000);
    auth_repo
        .create_auth_token(&token, key_record.user_id, key_record.company_id, now, expires_at)
        .await?;

    Ok(Json(AuthSession::new(
        key_record.user_id,
        ctx.email,
        ctx.name,
        key_record.company_id,
        ctx.company_name,
        ctx.company_slug,
        ctx.role,
        token,
    )))
}

pub async fn create_api_key(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<CompanyApiKeyCreate>,
) -> Result<Json<serde_json::Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden("Only admins can create API keys".into()));
    }

    let raw_key = format!("cmp_{}", Uuid::new_v4().simple());
    let key_prefix = &raw_key[..12];
    let key_hash = bcrypt::hash(&raw_key, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::Internal(e.into()))?;

    let id = Uuid::new_v4();
    let now = crate::util::now_ms();

    let repo = CompanyApiKeyRepo::new(state.db.clone());
    repo.create(id, auth.company_id, auth.user_id, &req.name, key_prefix, &key_hash, now)
        .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "name": req.name,
        "key": raw_key,
        "key_prefix": key_prefix,
        "created_at": now,
    })))
}

pub async fn list_api_keys(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Vec<CompanyApiKeyOut>>, AppError> {
    let repo = CompanyApiKeyRepo::new(state.db.clone());
    let keys = repo.list_by_company(auth.company_id).await?;

    let out: Vec<CompanyApiKeyOut> = keys
        .into_iter()
        .map(|k| CompanyApiKeyOut {
            id: k.id,
            user_id: k.user_id,
            name: k.name,
            key_prefix: k.key_prefix,
            created_at: k.created_at,
            last_used_at: k.last_used_at,
        })
        .collect();

    Ok(Json(out))
}

pub async fn delete_api_key(
    State(state): State<AppState>,
    auth: AuthContext,
    axum::extract::Path(key_id): axum::extract::Path<Uuid>,
) -> Result<Json<()>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden("Only admins can delete API keys".into()));
    }

    let repo = CompanyApiKeyRepo::new(state.db.clone());
    repo.delete(key_id, auth.company_id).await?;
    Ok(Json(()))
}