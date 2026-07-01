use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Json;

use crate::errors::AppError;
use crate::mcp::handler::resolve_claims_from_token;
use crate::middleware::AuthContext;
use crate::repos::auth::AuthRepo;
use crate::AppState;

/// Exchange a Kioku credential (JWT or raw `kioku_`/`cmp_` key) for the caller's
/// per-user Vexa API key. Lets any client holding a single Kioku credential
/// (e.g. the Vexa/meetings MCP server) authenticate to the Vexa gateway
/// without needing a separate Vexa-native token of its own.
pub async fn get_vexa_token(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".into()))?;

    let (company_id, user_id) =
        resolve_claims_from_token(&state.settings.jwt_secret, &state.db, token)
            .await
            .ok_or_else(|| AppError::Unauthorized("Invalid or expired token".into()))?;

    let auth = AuthContext {
        user_id,
        company_id,
        role: "member".into(),
    };
    let vexa_api_key = resolve_vexa_api_key(&state, &auth).await;
    Ok(Json(serde_json::json!({ "vexa_api_key": vexa_api_key })))
}

/// Resolve the Vexa `X-API-Key` to use on behalf of `auth`.
///
/// Lazily provisions a per-user Vexa API token on first use (find-or-create the
/// Vexa user by email, mint a token via admin-api), so bot-management calls are
/// scoped to the calling user instead of authenticating as one shared service
/// account for everyone. Falls back to the shared `vexa_admin_token` if
/// provisioning isn't configured or fails, preserving prior behavior.
async fn resolve_vexa_api_key(state: &AppState, auth: &AuthContext) -> String {
    let repo = AuthRepo::new(state.db.clone());

    if let Ok(Some(link)) = repo.find_vexa_link(auth.user_id).await {
        return link.vexa_token;
    }

    if state.settings.vexa_admin_token.is_empty() {
        return state.settings.vexa_admin_token.clone();
    }

    match provision_vexa_token(state, &repo, auth).await {
        Ok(token) => token,
        Err(e) => {
            tracing::warn!(error = %e, user_id = %auth.user_id, "Vexa per-user token provisioning failed, falling back to shared admin token");
            state.settings.vexa_admin_token.clone()
        }
    }
}

async fn provision_vexa_token(
    state: &AppState,
    repo: &AuthRepo,
    auth: &AuthContext,
) -> Result<String, AppError> {
    let ctx = repo
        .find_user_context(auth.user_id, auth.company_id)
        .await?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "user context not found for token provisioning"
            ))
        })?;

    let client = reqwest::Client::new();

    let user_resp = client
        .post(format!("{}/admin/users", state.settings.vexa_admin_api_url))
        .header("X-Admin-API-Key", &state.settings.vexa_admin_token)
        .json(&serde_json::json!({ "email": ctx.email, "name": ctx.name }))
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let user_json: serde_json::Value = user_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let vexa_user_id = user_json
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Vexa user response missing id")))?
        as i32;

    let token_resp = client
        .post(format!(
            "{}/admin/users/{}/tokens",
            state.settings.vexa_admin_api_url, vexa_user_id
        ))
        .header("X-Admin-API-Key", &state.settings.vexa_admin_token)
        .query(&[("scope", "bot"), ("name", "hivemind-cli")])
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let token_json: serde_json::Value = token_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let vexa_token_id = token_json
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Vexa token response missing id")))?
        as i32;
    let vexa_token = token_json
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Vexa token response missing token")))?
        .to_string();

    repo.set_vexa_link(auth.user_id, vexa_user_id, vexa_token_id, &vexa_token)
        .await?;

    Ok(vexa_token)
}

pub async fn request_bot(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(mut body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    if body.get("bot_name").is_none() {
        let company_name =
            sqlx::query_scalar::<_, String>("SELECT name FROM hivemind.companies WHERE id = $1")
                .bind(auth.company_id)
                .fetch_one(&state.db)
                .await
                .unwrap_or_else(|_| "Kioku".to_string());
        body["bot_name"] = serde_json::Value::String(format!("{}'s assistant", company_name));
    }

    let api_key = resolve_vexa_api_key(&state, &auth).await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/bots", state.settings.vexa_api_url))
        .header("X-API-Key", &api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let status = resp.status();
    let json = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Vexa request failed: {}", status)))?;

    Ok(Json(json))
}

pub async fn get_meetings(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, AppError> {
    let api_key = resolve_vexa_api_key(&state, &auth).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/meetings", state.settings.vexa_api_url))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let json = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Failed to parse Vexa response")))?;

    Ok(Json(json))
}

pub async fn get_bots_status(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, AppError> {
    let api_key = resolve_vexa_api_key(&state, &auth).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/bots/status", state.settings.vexa_api_url))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let json = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Failed to parse Vexa response")))?;

    Ok(Json(json))
}

pub async fn stop_bot(
    State(state): State<AppState>,
    auth: AuthContext,
    axum::extract::Path((platform, native_meeting_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let api_key = resolve_vexa_api_key(&state, &auth).await;
    let client = reqwest::Client::new();
    let resp = client
        .delete(format!(
            "{}/bots/{}/{}",
            state.settings.vexa_api_url, platform, native_meeting_id
        ))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let status = resp.status();
    let json = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({ "status": status.as_u16() }));

    Ok(Json(json))
}
