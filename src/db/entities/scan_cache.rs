use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(schema_name = "dashcam_archive", table_name = "scan_cache")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub abs_path: String,
    pub size: Option<i64>,
    pub mtime_ns: Option<i64>,
    pub ctime_ns: Option<i64>,
    pub duration_secs: Option<f64>,
    pub healthy: bool,
    pub broken: bool,
    pub probed_at: DateTimeWithTimeZone,
    pub codec: Option<String>,
    pub format_bps: Option<i64>,
    pub size_bytes: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
