use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(schema_name = "dashcam_archive", table_name = "merge_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub source_id: Uuid,
    pub trigger: String,
    pub status: String,
    pub started_at: DateTimeWithTimeZone,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub total_groups: i32,
    pub ok_groups: i32,
    pub downgraded_groups: i32,
    pub failed_groups: i32,
    pub bytes_in: Option<i64>,
    pub bytes_out: Option<i64>,
    pub folder_breaker_tripped: bool,
    pub log_summary: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
