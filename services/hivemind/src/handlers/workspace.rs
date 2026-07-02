use axum::extract::{Path, State};
use axum::response::Json;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::auth::{self, AuthRepo, MembershipRecord, WorkspaceRecord};
use crate::repos::invite::InviteRepo;
use crate::types::{
    InviteCreate, InviteOut, WorkspaceCreate, WorkspaceCreateResponse, WorkspaceOut,
};
use crate::AppState;

fn to_workspace_out(workspace: WorkspaceRecord, role: String) -> WorkspaceOut {
    WorkspaceOut {
        id: workspace.id,
        name: workspace.name,
        slug: workspace.slug,
        tier: workspace.tier,
        role,
        created_at: workspace.created_at,
    }
}

/// List every workspace the caller belongs to.
pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Vec<WorkspaceOut>>, AppError> {
    let repo = AuthRepo::new(state.db.clone());
    let ids: Vec<Uuid> = auth.memberships.iter().map(|m| m.workspace_id).collect();
    let workspaces = repo.find_workspaces_by_ids(&ids).await?;

    let out = workspaces
        .into_iter()
        .filter_map(|c| {
            let role = auth
                .memberships
                .iter()
                .find(|m| m.workspace_id == c.id)
                .map(|m| m.role.clone())?;
            Some(to_workspace_out(c, role))
        })
        .collect();

    Ok(Json(out))
}

/// Create a new workspace, with the caller as its admin. Every workspace
/// starts on the Free tier (capped at 1 member) — see migrations/008 and
/// invite::create for the tier gate. Returns a fresh token whose
/// `memberships` includes the new workspace, since the caller's old token
/// doesn't know about it yet.
pub async fn create(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<WorkspaceCreate>,
) -> Result<Json<WorkspaceCreateResponse>, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Workspace name is required".into()));
    }

    let repo = AuthRepo::new(state.db.clone());
    let now = crate::util::now_ms();
    let workspace_id = Uuid::new_v4();
    let slug = req.slug.clone().unwrap_or_else(|| auth::slugify(&req.name));

    repo.create_workspace(workspace_id, &req.name, &slug, now)
        .await?;
    repo.create_membership(Uuid::new_v4(), workspace_id, auth.user_id, "admin", now)
        .await?;
    repo.create_default_config(workspace_id, now).await?;

    let mut memberships: Vec<MembershipRecord> = auth
        .memberships
        .iter()
        .map(|m| MembershipRecord {
            workspace_id: m.workspace_id,
            role: m.role.clone(),
        })
        .collect();
    memberships.push(MembershipRecord {
        workspace_id,
        role: "admin".to_string(),
    });

    let token = auth::create_token(
        &state.settings.jwt_secret,
        auth.user_id,
        workspace_id,
        "admin",
        &memberships,
        state.settings.jwt_ttl_seconds,
    )?;
    let expires_at = now + (state.settings.jwt_ttl_seconds * 1000);
    repo.create_auth_token(&token, auth.user_id, workspace_id, now, expires_at)
        .await?;

    let workspace = repo.find_workspace_by_id(workspace_id).await?;
    Ok(Json(WorkspaceCreateResponse {
        workspace: to_workspace_out(workspace, "admin".to_string()),
        token,
    }))
}

/// Invite a teammate into a specific workspace (by id or slug) — unlike the
/// legacy `/workspace/invites`, which always targets the caller's active
/// workspace, this lets an admin of multiple workspaces target a
/// non-active one directly, e.g. `kioku ws <name> --invite <email>`.
pub async fn create_invite(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(workspace_id_or_slug): Path<String>,
    Json(req): Json<InviteCreate>,
) -> Result<Json<InviteOut>, AppError> {
    let auth_repo = AuthRepo::new(state.db.clone());
    let workspace = resolve_workspace(&auth_repo, &workspace_id_or_slug).await?;

    let membership = auth
        .memberships
        .iter()
        .find(|m| m.workspace_id == workspace.id)
        .ok_or_else(|| AppError::Forbidden("Not a member of that workspace".into()))?;
    if membership.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }
    if workspace.is_free_tier() {
        return Err(AppError::Forbidden(
            "Free tier is limited to 1 person — upgrade to Pro to invite teammates".into(),
        ));
    }

    let repo = InviteRepo::new(state.db.clone());
    let now = crate::util::now_ms();
    let invite = repo.create(workspace.id, req, now).await?;
    Ok(Json(invite))
}

async fn resolve_workspace(repo: &AuthRepo, id_or_slug: &str) -> Result<WorkspaceRecord, AppError> {
    if let Ok(id) = Uuid::parse_str(id_or_slug) {
        return repo.find_workspace_by_id(id).await;
    }
    repo.find_workspace_by_slug(id_or_slug)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("Workspace `{}` not found", id_or_slug)))
}
