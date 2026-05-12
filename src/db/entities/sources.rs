use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(schema_name = "dashcam_archive", table_name = "sources")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub src_path: String,
    pub dst_path: String,
    pub encoder: String,
    pub encoder_params: Json,
    pub max_gap_seconds: i32,
    pub max_group_duration_seconds: i32,
    pub monthly_subdirs: String,
    pub allow_combined_input: bool,
    pub no_broken_split: bool,
    pub trigger_mode: String,
    pub cron_expr: Option<String>,
    pub watcher_debounce_secs: i32,
    pub enabled: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
