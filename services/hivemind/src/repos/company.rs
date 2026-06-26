use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::AppError;
use crate::types::{CompanyConfigOut, CompanyConfigPatch};

pub struct CompanyRepo {
    db: PgPool,
}

impl CompanyRepo {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn get_config(&self, company_id: Uuid) -> Result<Option<CompanyConfigOut>, AppError> {
        let row = sqlx::query(
            r#"SELECT company_id, hivemind_enabled, updated_at
               FROM company_config WHERE company_id = $1"#,
        )
        .bind(company_id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)?;

        match row {
            Some(row) => Ok(Some(CompanyConfigOut {
                company_id: row.get("company_id"),
                hivemind_enabled: row.get("hivemind_enabled"),
                updated_at: row.get("updated_at"),
            })),
            None => Ok(None),
        }
    }

    pub async fn update_config(
        &self,
        company_id: Uuid,
        patch: CompanyConfigPatch,
        now: i64,
    ) -> Result<CompanyConfigOut, AppError> {
        sqlx::query(
            r#"
            INSERT INTO company_config (company_id, hivemind_enabled, updated_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (company_id) DO UPDATE SET
                hivemind_enabled = COALESCE(EXCLUDED.hivemind_enabled, company_config.hivemind_enabled),
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(company_id)
        .bind(patch.hivemind_enabled)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;

        self.get_config(company_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Company config not found".into()))
    }
}
