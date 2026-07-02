use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::AppError;
use crate::types::{WorkspaceConfigOut, WorkspaceConfigPatch};

pub struct WorkspaceConfigRepo {
    db: PgPool,
}

impl WorkspaceConfigRepo {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn get_config(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<WorkspaceConfigOut>, AppError> {
        let row = sqlx::query(
            r#"SELECT workspace_id, hivemind_enabled, updated_at
               FROM workspace_config WHERE workspace_id = $1"#,
        )
        .bind(workspace_id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)?;

        match row {
            Some(row) => Ok(Some(WorkspaceConfigOut {
                workspace_id: row.get("workspace_id"),
                hivemind_enabled: row.get("hivemind_enabled"),
                updated_at: row.get("updated_at"),
            })),
            None => Ok(None),
        }
    }

    pub async fn update_config(
        &self,
        workspace_id: Uuid,
        patch: WorkspaceConfigPatch,
        now: i64,
    ) -> Result<WorkspaceConfigOut, AppError> {
        sqlx::query(
            r#"
            INSERT INTO workspace_config (workspace_id, hivemind_enabled, updated_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (workspace_id) DO UPDATE SET
                hivemind_enabled = COALESCE(EXCLUDED.hivemind_enabled, workspace_config.hivemind_enabled),
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(workspace_id)
        .bind(patch.hivemind_enabled)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;

        self.get_config(workspace_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Workspace config not found".into()))
    }
}
