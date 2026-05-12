use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub mod entities;
pub mod repos;

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let base_url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;

    let sep = if base_url.contains('?') { '&' } else { '?' };
    let url = format!("{base_url}{sep}application_name=tokimo-app-dashcam-archive");

    let mut opts = ConnectOptions::new(url);
    opts.max_connections(4).min_connections(1).sqlx_logging(false);

    Ok(Database::connect(opts).await?)
}

pub async fn init_schema(_db: &DatabaseConnection) -> anyhow::Result<()> {
    Ok(())
}
