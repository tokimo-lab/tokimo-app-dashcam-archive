use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub mod entities;
pub mod repos;

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let base_url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
    let schema = tokimo_bus_cli::manifest::parse_app_schema(crate::MANIFEST)?
        .ok_or_else(|| anyhow::anyhow!("manifest missing [database] schema"))?;

    let sep = if base_url.contains('?') { '&' } else { '?' };
    let encoded = urlencoding::encode(&schema);
    let url = format!(
        "{base_url}{sep}application_name=tokimo-app-dashcam-archive&options=-c%20search_path%3D%22{encoded}%22%2Cpublic"
    );
    let mut opts = ConnectOptions::new(url);
    opts.max_connections(4).min_connections(1).sqlx_logging(false);
    Ok(Database::connect(opts).await?)
}
