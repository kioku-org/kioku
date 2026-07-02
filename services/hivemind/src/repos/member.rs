use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::AppError;
use crate::types::MemberOut;

pub struct MemberRepo {
    db: PgPool,
}

impl MemberRepo {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn list(&self, workspace_id: Uuid) -> Result<Vec<MemberOut>, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT cm.id, cm.workspace_id, cm.user_id, cm.role, cm.joined_at,
                   u.email, u.name
            FROM workspace_members cm
            JOIN users u ON u.id = cm.user_id
            WHERE cm.workspace_id = $1
            ORDER BY cm.joined_at DESC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::from)?;

        Ok(rows
            .into_iter()
            .map(|r| MemberOut {
                id: r.get("id"),
                workspace_id: r.get("workspace_id"),
                user_id: r.get("user_id"),
                role: r.get("role"),
                email: r.get("email"),
                name: r.get("name"),
                joined_at: r.get("joined_at"),
            })
            .collect())
    }

    pub async fn remove(&self, user_id: Uuid, workspace_id: Uuid) -> Result<(), AppError> {
        sqlx::query(r#"DELETE FROM workspace_members WHERE user_id = $1 AND workspace_id = $2"#)
            .bind(user_id)
            .bind(workspace_id)
            .execute(&self.db)
            .await
            .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn update_role(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
        role: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"UPDATE workspace_members SET role = $1 WHERE user_id = $2 AND workspace_id = $3"#,
        )
        .bind(role)
        .bind(user_id)
        .bind(workspace_id)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }
}
