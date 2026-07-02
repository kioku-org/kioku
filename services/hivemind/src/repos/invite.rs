use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::AppError;
use crate::types::{InviteCreate, InviteOut};

pub struct InviteRepo {
    db: PgPool,
}

impl InviteRepo {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        workspace_id: Uuid,
        req: InviteCreate,
        now: i64,
    ) -> Result<InviteOut, AppError> {
        let id = Uuid::new_v4();
        let email = req.email.clone();
        let role = req.role.clone();
        sqlx::query(
            r#"INSERT INTO workspace_invites (id, workspace_id, email, role, created_at) VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(req.email)
        .bind(req.role)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;

        Ok(InviteOut {
            id,
            workspace_id,
            email,
            role,
            created_at: now,
            used_at: None,
        })
    }

    pub async fn list(&self, workspace_id: Uuid) -> Result<Vec<InviteOut>, AppError> {
        let rows = sqlx::query(
            r#"SELECT id, workspace_id, email, role, created_at, used_at
               FROM workspace_invites WHERE workspace_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(workspace_id)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::from)?;

        Ok(rows
            .into_iter()
            .map(|r| InviteOut {
                id: r.get("id"),
                workspace_id: r.get("workspace_id"),
                email: r.get("email"),
                role: r.get("role"),
                created_at: r.get("created_at"),
                used_at: r.get("used_at"),
            })
            .collect())
    }

    pub async fn remove(&self, invite_id: Uuid, workspace_id: Uuid) -> Result<(), AppError> {
        sqlx::query(r#"DELETE FROM workspace_invites WHERE id = $1 AND workspace_id = $2"#)
            .bind(invite_id)
            .bind(workspace_id)
            .execute(&self.db)
            .await
            .map_err(AppError::from)?;
        Ok(())
    }
}
