use uuid::Uuid;

use crate::config::Settings;
use crate::errors::AppError;
use crate::repos::auth::{self, AuthRepo, MembershipRecord};
use crate::types::{
    AuthSession, ProvisionRequest, RegisterAdminRequest, RegisterMemberRequest,
    RegisterPersonalRequest, SignInRequest,
};

#[derive(Clone)]
pub struct AuthService;

impl AuthService {
    pub async fn register_admin(
        &self,
        repo: &AuthRepo,
        settings: &Settings,
        req: RegisterAdminRequest,
    ) -> Result<AuthSession, AppError> {
        if req.workspace_name.trim().is_empty() {
            return Err(AppError::BadRequest("Workspace name is required".into()));
        }
        if !req.email.contains('@') {
            return Err(AppError::BadRequest("Valid email is required".into()));
        }
        if req.password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }

        if repo.find_user_by_email(&req.email).await?.is_some() {
            return Err(AppError::BadRequest("Email already registered".into()));
        }

        let now = crate::util::now_ms();
        let workspace_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let slug = req
            .workspace_slug
            .clone()
            .unwrap_or_else(|| auth::slugify(&req.workspace_name));
        let password_hash = auth::hash_password(&req.password)?;

        repo.create_workspace(workspace_id, &req.workspace_name, &slug, now)
            .await?;
        repo.create_user(user_id, &req.email, &req.name, &password_hash, now)
            .await?;
        repo.create_membership(Uuid::new_v4(), workspace_id, user_id, "admin", now)
            .await?;
        repo.create_default_config(workspace_id, now).await?;

        let memberships = [MembershipRecord {
            workspace_id,
            role: "admin".to_string(),
        }];
        let token = auth::create_token(
            &settings.jwt_secret,
            user_id,
            workspace_id,
            "admin",
            &memberships,
            settings.jwt_ttl_seconds,
        )?;
        let expires_at = now + (settings.jwt_ttl_seconds * 1000);
        repo.create_auth_token(&token, user_id, workspace_id, now, expires_at)
            .await?;

        Ok(AuthSession::new(
            user_id,
            req.email,
            req.name,
            workspace_id,
            req.workspace_name,
            slug,
            "admin".into(),
            token,
        ))
    }

    pub async fn register_member(
        &self,
        repo: &AuthRepo,
        settings: &Settings,
        req: RegisterMemberRequest,
    ) -> Result<AuthSession, AppError> {
        if !req.email.contains('@') {
            return Err(AppError::BadRequest("Valid email is required".into()));
        }
        if req.password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }

        let workspace = repo
            .find_workspace_by_slug(&req.workspace_slug)
            .await?
            .ok_or_else(|| AppError::BadRequest("Workspace not found".into()))?;

        let invite = repo
            .find_invite(&req.email, workspace.id)
            .await?
            .ok_or_else(|| AppError::BadRequest("No invitation found".into()))?;

        if repo.find_user_by_email(&req.email).await?.is_some() {
            return Err(AppError::BadRequest("Email already registered".into()));
        }

        // Defense in depth: invite creation is already blocked for free-tier
        // workspaces (see invite::create), but re-check here in case tier
        // changed after the invite was issued.
        if workspace.is_free_tier() && repo.count_workspace_members(workspace.id).await? >= 1 {
            return Err(AppError::Forbidden(
                "Free tier is limited to 1 person — upgrade to Pro to add teammates".into(),
            ));
        }

        let now = crate::util::now_ms();
        let user_id = Uuid::new_v4();
        let password_hash = auth::hash_password(&req.password)?;

        repo.create_user(user_id, &req.email, &req.name, &password_hash, now)
            .await?;
        repo.create_membership(Uuid::new_v4(), workspace.id, user_id, &invite.role, now)
            .await?;
        repo.mark_invite_used(invite.id, now).await?;

        let memberships = [MembershipRecord {
            workspace_id: workspace.id,
            role: invite.role.clone(),
        }];
        let token = auth::create_token(
            &settings.jwt_secret,
            user_id,
            workspace.id,
            &invite.role,
            &memberships,
            settings.jwt_ttl_seconds,
        )?;
        let expires_at = now + (settings.jwt_ttl_seconds * 1000);
        repo.create_auth_token(&token, user_id, workspace.id, now, expires_at)
            .await?;

        Ok(AuthSession::new(
            user_id,
            req.email,
            req.name,
            workspace.id,
            workspace.name,
            workspace.slug,
            invite.role,
            token,
        ))
    }

    pub async fn register_personal(
        &self,
        repo: &AuthRepo,
        settings: &Settings,
        req: RegisterPersonalRequest,
    ) -> Result<AuthSession, AppError> {
        if !req.email.contains('@') {
            return Err(AppError::BadRequest("Valid email is required".into()));
        }
        if req.password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }

        if repo.find_user_by_email(&req.email).await?.is_some() {
            return Err(AppError::BadRequest("Email already registered".into()));
        }

        let now = crate::util::now_ms();
        let workspace_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let email_local = req.email.split('@').next().unwrap_or("user");
        let slug = auth::slugify(&format!("{}-personal", email_local));
        let workspace_name = format!("{}'s Workspace", req.name);

        let password_hash = auth::hash_password(&req.password)?;

        repo.create_workspace(workspace_id, &workspace_name, &slug, now)
            .await?;
        repo.create_user(user_id, &req.email, &req.name, &password_hash, now)
            .await?;
        repo.create_membership(Uuid::new_v4(), workspace_id, user_id, "admin", now)
            .await?;
        repo.create_default_config(workspace_id, now).await?;

        let memberships = [MembershipRecord {
            workspace_id,
            role: "admin".to_string(),
        }];
        let token = auth::create_token(
            &settings.jwt_secret,
            user_id,
            workspace_id,
            "admin",
            &memberships,
            settings.jwt_ttl_seconds,
        )?;
        let expires_at = now + (settings.jwt_ttl_seconds * 1000);
        repo.create_auth_token(&token, user_id, workspace_id, now, expires_at)
            .await?;

        Ok(AuthSession::new(
            user_id,
            req.email,
            req.name,
            workspace_id,
            workspace_name,
            slug,
            "admin".into(),
            token,
        ))
    }

    pub async fn provision(
        &self,
        repo: &AuthRepo,
        settings: &Settings,
        req: ProvisionRequest,
    ) -> Result<AuthSession, AppError> {
        let now = crate::util::now_ms();

        if let Some(user) = repo.find_user_by_email(&req.email).await? {
            let memberships = repo.list_memberships(user.id).await?;
            let membership = memberships
                .first()
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("User has no membership")))?;
            let workspace = repo.find_workspace_by_id(membership.workspace_id).await?;
            let token = auth::create_token(
                &settings.jwt_secret,
                user.id,
                workspace.id,
                &membership.role,
                &memberships,
                settings.jwt_ttl_seconds,
            )?;
            let expires_at = now + (settings.jwt_ttl_seconds * 1000);
            repo.create_auth_token(&token, user.id, workspace.id, now, expires_at)
                .await?;
            return Ok(AuthSession::new(
                user.id,
                user.email,
                user.name,
                workspace.id,
                workspace.name,
                workspace.slug,
                membership.role.clone(),
                token,
            ));
        }

        let workspace_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let email_local = req.email.split('@').next().unwrap_or("user");
        let slug = auth::slugify(&format!("{}-personal", email_local));
        let workspace_name = format!("{}'s Workspace", req.name);
        let password_hash = auth::hash_password(&Uuid::new_v4().to_string())?;

        repo.create_workspace(workspace_id, &workspace_name, &slug, now)
            .await?;
        repo.create_user(user_id, &req.email, &req.name, &password_hash, now)
            .await?;
        repo.create_membership(Uuid::new_v4(), workspace_id, user_id, "admin", now)
            .await?;
        repo.create_default_config(workspace_id, now).await?;

        let memberships = [MembershipRecord {
            workspace_id,
            role: "admin".to_string(),
        }];
        let token = auth::create_token(
            &settings.jwt_secret,
            user_id,
            workspace_id,
            "admin",
            &memberships,
            settings.jwt_ttl_seconds,
        )?;
        let expires_at = now + (settings.jwt_ttl_seconds * 1000);
        repo.create_auth_token(&token, user_id, workspace_id, now, expires_at)
            .await?;

        Ok(AuthSession::new(
            user_id,
            req.email,
            req.name,
            workspace_id,
            workspace_name,
            slug,
            "admin".into(),
            token,
        ))
    }

    pub async fn sign_in(
        &self,
        repo: &AuthRepo,
        settings: &Settings,
        req: SignInRequest,
    ) -> Result<AuthSession, AppError> {
        let user = repo
            .find_user_by_email(&req.email)
            .await?
            .ok_or_else(|| AppError::Unauthorized("No account found".into()))?;

        if !auth::verify_password(&req.password, &user.password_hash)? {
            return Err(AppError::Unauthorized("Incorrect password".into()));
        }

        let memberships = repo.list_memberships(user.id).await?;
        let membership = memberships
            .first()
            .ok_or_else(|| AppError::Unauthorized("No workspace membership found".into()))?;

        let workspace = repo.find_workspace_by_id(membership.workspace_id).await?;

        let now = crate::util::now_ms();
        let token = auth::create_token(
            &settings.jwt_secret,
            user.id,
            workspace.id,
            &membership.role,
            &memberships,
            settings.jwt_ttl_seconds,
        )?;
        let expires_at = now + (settings.jwt_ttl_seconds * 1000);
        repo.create_auth_token(&token, user.id, workspace.id, now, expires_at)
            .await?;

        Ok(AuthSession::new(
            user.id,
            user.email,
            user.name,
            workspace.id,
            workspace.name,
            workspace.slug,
            membership.role.clone(),
            token,
        ))
    }
}
