use axum::http::StatusCode;
use axum::response::Json;
use serde::Serialize;
use uuid::Uuid;

use crate::repos::auth;
use crate::types::MembershipClaim;
use crate::AppState;

/// Request header a client sends to select which workspace it's
/// operating against, when the token holds more than one membership. Falls
/// back to the token's default `workspace_id` when absent.
pub const WORKSPACE_HEADER: &str = "x-workspace-id";

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub role: String,
    /// Every workspace this user belongs to, per their token.
    pub memberships: Vec<MembershipClaim>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    ok: bool,
    error: String,
}

#[axum::async_trait]
impl axum::extract::FromRequestParts<AppState> for AuthContext {
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    ok: false,
                    error: "Missing Authorization header".into(),
                }),
            ))?;

        let token = auth_header.strip_prefix("Bearer ").ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                ok: false,
                error: "Invalid Authorization format".into(),
            }),
        ))?;

        let claims = auth::validate_token(&state.settings.jwt_secret, token).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    ok: false,
                    error: "Invalid or expired token".into(),
                }),
            )
        })?;

        // `memberships` is empty for tokens issued before workspaces existed
        // (see #[serde(default)] on Claims::memberships) — treat those as a
        // single-membership token so nothing breaks for already-signed-in
        // clients.
        let memberships = if claims.memberships.is_empty() {
            vec![MembershipClaim {
                workspace_id: claims.workspace_id,
                role: claims.role.clone(),
            }]
        } else {
            claims.memberships.clone()
        };

        // Resolve which workspace this request operates against: the
        // X-Workspace-Id header if the client sent one (must be one of the
        // token's memberships), otherwise the token's default workspace.
        let (workspace_id, role) = match parts.headers.get(WORKSPACE_HEADER) {
            Some(header_value) => {
                let requested: Uuid = header_value
                    .to_str()
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .ok_or((
                        StatusCode::BAD_REQUEST,
                        Json(ApiError {
                            ok: false,
                            error: "Invalid X-Workspace-Id header".into(),
                        }),
                    ))?;
                let membership = memberships
                    .iter()
                    .find(|m| m.workspace_id == requested)
                    .ok_or((
                    StatusCode::FORBIDDEN,
                    Json(ApiError {
                        ok: false,
                        error:
                            "Not a member of that workspace — run `kioku ws` to see your workspaces"
                                .into(),
                    }),
                ))?;
                (membership.workspace_id, membership.role.clone())
            }
            None => (claims.workspace_id, claims.role.clone()),
        };

        // Validate that the workspace still exists in the database.
        // This prevents FK violations when the DB was reset but the client
        // is still using an old JWT token.
        let workspace_exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1)")
                .bind(workspace_id)
                .fetch_one(&state.db)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to validate workspace: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError {
                            ok: false,
                            error: "Internal server error".into(),
                        }),
                    )
                })?;

        if !workspace_exists {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    ok: false,
                    error: "Workspace not found. Please re-authenticate.".into(),
                }),
            ));
        }

        // Validate that the token has not been revoked (e.g. via signout).
        let token_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM auth_tokens WHERE token = $1)",
        )
        .bind(token)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to validate token: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    ok: false,
                    error: "Internal server error".into(),
                }),
            )
        })?;

        if !token_exists {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    ok: false,
                    error: "Token has been revoked. Please re-authenticate.".into(),
                }),
            ));
        }

        Ok(AuthContext {
            user_id: claims.user_id,
            workspace_id,
            role,
            memberships,
        })
    }
}
