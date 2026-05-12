use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(schema_name = "dashcam_archive", table_name = "merge_groups")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub run_id: Uuid,
    pub camera_key: String,
    pub start_dt: Option<DateTimeWithTimeZone>,
    pub end_dt: Option<DateTimeWithTimeZone>,
    pub output_path: String,
    pub decision: String,
    pub status: String,
    pub warning_level: String,
    pub bytes_in: Option<i64>,
    pub bytes_out: Option<i64>,
    pub duration_secs: Option<f64>,
    pub abort_reason: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
