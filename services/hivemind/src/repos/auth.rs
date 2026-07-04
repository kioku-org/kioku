use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::types::{AuthSession, Claims};

pub struct AuthRepo {
    db: PgPool,
}

impl AuthRepo {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<UserRecord>, AppError> {
        sqlx::query_as::<_, UserRecord>(
            r#"SELECT id, email, name, password_hash FROM users WHERE email = $1"#,
        )
        .bind(email)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_workspace_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<WorkspaceRecord>, AppError> {
        sqlx::query_as::<_, WorkspaceRecord>(
            r#"SELECT id, name, slug, tier, created_at FROM workspaces WHERE slug = $1"#,
        )
        .bind(slug)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_workspace_by_id(&self, id: Uuid) -> Result<WorkspaceRecord, AppError> {
        sqlx::query_as::<_, WorkspaceRecord>(
            r#"SELECT id, name, slug, tier, created_at FROM workspaces WHERE id = $1"#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_workspaces_by_ids(
        &self,
        ids: &[Uuid],
    ) -> Result<Vec<WorkspaceRecord>, AppError> {
        sqlx::query_as::<_, WorkspaceRecord>(
            r#"SELECT id, name, slug, tier, created_at FROM workspaces WHERE id = ANY($1)"#,
        )
        .bind(ids)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::from)
    }

    /// Number of members currently in a workspace. Free tier is capped at 1.
    pub async fn count_workspace_members(&self, workspace_id: Uuid) -> Result<i64, AppError> {
        let row: (i64,) =
            sqlx::query_as(r#"SELECT COUNT(*) FROM workspace_members WHERE workspace_id = $1"#)
                .bind(workspace_id)
                .fetch_one(&self.db)
                .await
                .map_err(AppError::from)?;
        Ok(row.0)
    }

    pub async fn find_invite(
        &self,
        email: &str,
        workspace_id: Uuid,
    ) -> Result<Option<InviteRecord>, AppError> {
        sqlx::query_as::<_, InviteRecord>(
            r#"SELECT id, role FROM workspace_invites WHERE email = $1 AND workspace_id = $2 AND used_at IS NULL"#,
        )
        .bind(email)
        .bind(workspace_id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_membership(
        &self,
        user_id: Uuid,
    ) -> Result<Option<MembershipRecord>, AppError> {
        sqlx::query_as::<_, MembershipRecord>(
            r#"SELECT workspace_id, role FROM workspace_members WHERE user_id = $1 LIMIT 1"#,
        )
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)
    }

    /// Every workspace a user belongs to, oldest first — the first entry is
    /// used as the default/active workspace when a token is issued.
    pub async fn list_memberships(&self, user_id: Uuid) -> Result<Vec<MembershipRecord>, AppError> {
        sqlx::query_as::<_, MembershipRecord>(
            r#"SELECT workspace_id, role FROM workspace_members WHERE user_id = $1 ORDER BY joined_at ASC"#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_user_context(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<UserContextRecord>, AppError> {
        sqlx::query_as::<_, UserContextRecord>(
            r#"
            SELECT u.email, u.name, c.name AS workspace_name, c.slug AS workspace_slug, cm.role
            FROM users u
            JOIN workspaces c ON c.id = $1
            JOIN workspace_members cm ON cm.user_id = $2 AND cm.workspace_id = $1
            WHERE u.id = $2
            "#,
        )
        .bind(workspace_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn create_workspace(
        &self,
        id: Uuid,
        name: &str,
        slug: &str,
        now: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO workspaces (id, name, slug, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(now)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn create_user(
        &self,
        id: Uuid,
        email: &str,
        name: &str,
        password_hash: &str,
        now: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO users (id, email, name, password_hash, created_at) VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(id)
        .bind(email)
        .bind(name)
        .bind(password_hash)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn create_membership(
        &self,
        id: Uuid,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
        now: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO workspace_members (id, workspace_id, user_id, role, joined_at) VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(user_id)
        .bind(role)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn create_default_config(
        &self,
        workspace_id: Uuid,
        now: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO workspace_config (workspace_id, hivemind_enabled, updated_at)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(workspace_id)
        .bind(true)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn create_auth_token(
        &self,
        token: &str,
        user_id: Uuid,
        workspace_id: Uuid,
        now: i64,
        expires_at: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO auth_tokens (token, user_id, workspace_id, created_at, expires_at) VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (token) DO UPDATE SET expires_at = $5, created_at = $4"#,
        )
        .bind(token)
        .bind(user_id)
        .bind(workspace_id)
        .bind(now)
        .bind(expires_at)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn delete_auth_tokens(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(r#"DELETE FROM auth_tokens WHERE user_id = $1 AND workspace_id = $2"#)
            .bind(user_id)
            .bind(workspace_id)
            .execute(&self.db)
            .await
            .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn mark_invite_used(&self, invite_id: Uuid, now: i64) -> Result<(), AppError> {
        sqlx::query(r#"UPDATE workspace_invites SET used_at = $1 WHERE id = $2"#)
            .bind(now)
            .bind(invite_id)
            .execute(&self.db)
            .await
            .map_err(AppError::from)?;
        Ok(())
    }

    /// Per-user Vexa credential link, provisioned lazily on first bot-management call.
    pub async fn find_vexa_link(&self, user_id: Uuid) -> Result<Option<VexaLinkRecord>, AppError> {
        sqlx::query_as::<_, VexaLinkRecord>(
            r#"SELECT vexa_user_id, vexa_token_id, vexa_token
               FROM users WHERE id = $1 AND vexa_token IS NOT NULL"#,
        )
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn set_vexa_link(
        &self,
        user_id: Uuid,
        vexa_user_id: i32,
        vexa_token_id: i32,
        vexa_token: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"UPDATE users SET vexa_user_id = $1, vexa_token_id = $2, vexa_token = $3 WHERE id = $4"#,
        )
        .bind(vexa_user_id)
        .bind(vexa_token_id)
        .bind(vexa_token)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRecord {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub password_hash: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct WorkspaceRecord {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub tier: String,
    pub created_at: i64,
}

impl WorkspaceRecord {
    pub fn is_free_tier(&self) -> bool {
        self.tier == "free"
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct InviteRecord {
    pub id: Uuid,
    pub role: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct MembershipRecord {
    pub workspace_id: Uuid,
    pub role: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserContextRecord {
    pub email: String,
    pub name: String,
    pub workspace_name: String,
    pub workspace_slug: String,
    pub role: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct VexaLinkRecord {
    pub vexa_user_id: i32,
    pub vexa_token_id: i32,
    pub vexa_token: String,
}

// ─── JWT helpers ─────────────────────────────────────────────────────────────

/// `memberships` should be every workspace the user belongs to (not just
/// `workspace_id`/`role`) so `kioku ws <name>` can switch between them without
/// a network round trip. Pass a single-element slice for brand-new users who
/// only just got their first membership.
pub fn create_token(
    secret: &str,
    user_id: Uuid,
    workspace_id: Uuid,
    role: &str,
    memberships: &[MembershipRecord],
    ttl_seconds: i64,
) -> Result<String, AppError> {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::Internal(e.into()))?
        .as_secs() as i64;

    let claims = Claims {
        user_id,
        workspace_id,
        role: role.to_string(),
        memberships: memberships
            .iter()
            .map(|m| crate::types::MembershipClaim {
                workspace_id: m.workspace_id,
                role: m.role.clone(),
            })
            .collect(),
        exp: now + ttl_seconds,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(e.into()))
}

pub fn validate_token(secret: &str, token: &str) -> Result<Claims, AppError> {
    use jsonwebtoken::{decode, DecodingKey, Validation};

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized("Invalid or expired token".into()))?;

    Ok(token_data.claims)
}

/// Resolve workspace_id + user_id from a Kioku credential: a JWT, an exchanged API-key session
/// token (`auth_tokens`), or a raw `kioku_`/`cmp_` API key (`workspace_api_keys`, looked up by
/// prefix). Tries JWT first (fast, no DB hit). Shared by the `/vexa/token` exchange and
/// (externally) the consolidated MCP service, which forwards whatever credential its caller
/// presents.
pub async fn resolve_claims_from_token(
    jwt_secret: &str,
    db: &PgPool,
    token: &str,
) -> Option<(Uuid, Uuid)> {
    if let Ok(claims) = validate_token(jwt_secret, token) {
        return Some((claims.workspace_id, claims.user_id));
    }

    if token.starts_with("kioku_") && token.len() >= 14 {
        let prefix = &token[..14];
        return sqlx::query_as("SELECT workspace_id, user_id FROM workspace_api_keys WHERE key_prefix = $1 LIMIT 1")
            .bind(prefix)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    }
    if token.starts_with("cmp_") && token.len() >= 12 {
        let prefix = &token[..12];
        return sqlx::query_as("SELECT workspace_id, user_id FROM workspace_api_keys WHERE key_prefix = $1 LIMIT 1")
            .bind(prefix)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    }

    sqlx::query_as("SELECT workspace_id, user_id FROM auth_tokens WHERE token = $1 LIMIT 1")
        .bind(token)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

// ─── Password helpers ────────────────────────────────────────────────────────

pub fn hash_password(password: &str) -> Result<String, AppError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|e| AppError::Internal(e.into()))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    bcrypt::verify(password, hash).map_err(|e| AppError::Internal(e.into()))
}

// ─── Slugify ─────────────────────────────────────────────────────────────────

pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(64)
        .collect()
}

// ─── AuthSession builder ────────────────────────────────────────────────────

impl AuthSession {
    pub fn new(
        user_id: Uuid,
        email: String,
        name: String,
        workspace_id: Uuid,
        workspace_name: String,
        workspace_slug: String,
        role: String,
        token: String,
    ) -> Self {
        Self {
            user_id,
            email,
            name,
            workspace_id,
            workspace_name,
            workspace_slug,
            role,
            token,
        }
    }
}
