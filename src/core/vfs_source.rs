use std::sync::Arc;

use sea_orm::{DatabaseBackend, DatabaseConnection, FromQueryResult, Statement};
use serde_json::Value;
use tokimo_vfs::{Driver, DriverRegistry, StorageManager, StorageMount, Vfs};
use uuid::Uuid;

#[derive(Debug, FromQueryResult)]
struct VfsRecord {
    id: Uuid,
    vfs_type: String,
    config: Value,
}

pub async fn build_vfs(db: &DatabaseConnection, source_id: Uuid, source_type: &str) -> anyhow::Result<Arc<Vfs>> {
    let stmt = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        r#"SELECT id, "type" AS vfs_type, COALESCE(config, '{}'::jsonb) AS config FROM public.vfs WHERE id = $1"#,
        [source_id.into()],
    );
    let Some(record) = VfsRecord::find_by_statement(stmt).one(db).await? else {
        anyhow::bail!("vfs source {source_id} not found");
    };
    if record.vfs_type != source_type {
        anyhow::bail!(
            "vfs source {} type mismatch: expected {}, got {}",
            record.id,
            source_type,
            record.vfs_type
        );
    }

    let registry = DriverRegistry::new();
    let driver = registry.create(&record.vfs_type, &record.config)?;
    let driver: Arc<dyn Driver> = Arc::from(driver);
    driver.init().await?;

    let manager = StorageManager::new();
    manager.mount(StorageMount::new("/", driver)).await;
    Ok(Arc::new(Vfs::new(manager)))
}
