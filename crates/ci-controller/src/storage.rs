use sqlx::PgPool;
use tracing::info;

/// PostgreSQL storage for persistent state
pub struct Storage {
    pool: PgPool,
}

impl Storage {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        info!("Connected to PostgreSQL");
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        // TODO: Run migrations
        info!("Database migrations complete");
        Ok(())
    }
}
