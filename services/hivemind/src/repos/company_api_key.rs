use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub struct CompanyApiKeyRepo {
    db: PgPool,
}

impl CompanyApiKeyRepo {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn find_by_key_prefix(&self, key_prefix: &str) -> Result<Option<CompanyApiKeyRecord>, AppError> {
        sqlx::query_as::<_, CompanyApiKeyRecord>(
            r#"SELECT id, company_id, user_id, name, key_prefix, key_hash, created_at, last_used_at
               FROM company_api_keys WHERE key_prefix = $1"#,
        )
        .bind(key_prefix)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn create(
        &self,
        id: Uuid,
        company_id: Uuid,
        user_id: Uuid,
        name: &str,
        key_prefix: &str,
        key_hash: &str,
        now: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO company_api_keys (id, company_id, user_id, name, key_prefix, key_hash, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(id)
        .bind(company_id)
        .bind(user_id)
        .bind(name)
        .bind(key_prefix)
        .bind(key_hash)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }

    pub async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<CompanyApiKeyRecord>, AppError> {
        sqlx::query_as::<_, CompanyApiKeyRecord>(
            r#"SELECT id, company_id, user_id, name, key_prefix, key_hash, created_at, last_used_at
               FROM company_api_keys WHERE company_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(company_id)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::from)
    }

    pub async fn delete(&self, id: Uuid, company_id: Uuid) -> Result<(), AppError> {
        let result = sqlx::query(
            r#"DELETE FROM company_api_keys WHERE id = $1 AND company_id = $2"#,
        )
        .bind(id)
        .bind(company_id)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("API key not found".into()));
        }
        Ok(())
    }

    pub async fn update_last_used(&self, id: Uuid, now: i64) -> Result<(), AppError> {
        sqlx::query(
            r#"UPDATE company_api_keys SET last_used_at = $1 WHERE id = $2"#,
        )
        .bind(now)
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(AppError::from)?;
        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct CompanyApiKeyRecord {
    pub id: Uuid,
    pub company_id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}