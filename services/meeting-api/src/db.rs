use crate::config::Config;
use sqlx::{postgres::PgPoolOptions, Executor, PgPool};

const VEXA_SCHEMA: &str = "vexa";

pub async fn connect(cfg: &Config) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(cfg.db_pool_size)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute(format!("SET search_path TO {VEXA_SCHEMA}, public").as_str()).await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::new()
                .host(&cfg.db_host)
                .port(cfg.db_port)
                .database(&cfg.db_name)
                .username(&cfg.db_user)
                .password(&cfg.db_password),
        )
        .await?;

    // Table definitions are IF NOT EXISTS mirrors of what the Python service already created —
    // this must be a no-op against the existing production schema, see migrations/0001_init.sql.
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!(host = %cfg.db_host, db = %cfg.db_name, "database connected");
    Ok(pool)
}
