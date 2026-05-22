use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub mod entities;
pub mod repos;

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let base_url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
    let schema = std::env::var("TOKIMO_APP_SCHEMA").unwrap_or_else(|_| "dashcam_archive".to_string());

    let sep = if base_url.contains('?') { '&' } else { '?' };
    let encoded = urlencoding::encode(&schema);
    let url = format!(
        "{base_url}{sep}application_name=tokimo-app-dashcam-archive&options=-c%20search_path%3D%22{encoded}%22%2Cpublic"
    );
    let mut opts = ConnectOptions::new(url);
    opts.max_connections(4).min_connections(1).sqlx_logging(false);
    Ok(Database::connect(opts).await?)
}
