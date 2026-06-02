use chrono::Utc;
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, sea_query::Expr};
use serde_json::Value;
use uuid::Uuid;

use crate::db::entities::sources::{self, Entity as Sources};

#[derive(Debug, Clone)]
pub struct SourceInput {
    pub user_id: Uuid,
    pub name: String,
    pub src_source_id: Uuid,
    pub src_source_type: String,
    pub dst_source_id: Uuid,
    pub dst_source_type: String,
    pub src_path: String,
    pub dst_path: String,
    pub encoder: String,
    pub encoder_params: Value,
    pub preflight_bitrate_ref: i32,
    pub hybrid_health_check: bool,
    pub max_gap_seconds: i32,
    pub max_group_duration_seconds: i32,
    pub monthly_subdirs: String,
    pub allow_combined_input: bool,
    pub no_broken_split: bool,
    pub trigger_mode: String,
    pub cron_expr: Option<String>,
    pub watcher_debounce_secs: i32,
    pub enabled: bool,
}

pub struct SourcesRepo;

impl SourcesRepo {
    pub async fn list<C: ConnectionTrait>(db: &C, user_id: Uuid) -> anyhow::Result<Vec<sources::Model>> {
        Ok(Sources::find()
            .filter(sources::Column::UserId.eq(user_id))
            .order_by_desc(sources::Column::CreatedAt)
            .all(db)
            .await?)
    }

    pub async fn list_enabled<C: ConnectionTrait>(db: &C) -> anyhow::Result<Vec<sources::Model>> {
        Ok(Sources::find()
            .filter(sources::Column::Enabled.eq(true))
            .order_by_asc(sources::Column::CreatedAt)
            .all(db)
            .await?)
    }

    pub async fn get<C: ConnectionTrait>(db: &C, id: Uuid, user_id: Uuid) -> anyhow::Result<Option<sources::Model>> {
        Ok(Sources::find()
            .filter(sources::Column::Id.eq(id))
            .filter(sources::Column::UserId.eq(user_id))
            .one(db)
            .await?)
    }

    pub async fn create<C: ConnectionTrait>(db: &C, input: SourceInput) -> anyhow::Result<sources::Model> {
        let now = Utc::now().into();
        let model = sources::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(input.user_id),
            name: Set(input.name),
            src_source_id: Set(input.src_source_id),
            src_source_type: Set(input.src_source_type),
            dst_source_id: Set(input.dst_source_id),
            dst_source_type: Set(input.dst_source_type),
            src_path: Set(input.src_path),
            dst_path: Set(input.dst_path),
            encoder: Set(input.encoder),
            encoder_params: Set(input.encoder_params),
            preflight_bitrate_ref: Set(input.preflight_bitrate_ref),
            hybrid_health_check: Set(input.hybrid_health_check),
            max_gap_seconds: Set(input.max_gap_seconds),
            max_group_duration_seconds: Set(input.max_group_duration_seconds),
            monthly_subdirs: Set(input.monthly_subdirs),
            allow_combined_input: Set(input.allow_combined_input),
            no_broken_split: Set(input.no_broken_split),
            trigger_mode: Set(input.trigger_mode),
            cron_expr: Set(input.cron_expr),
            watcher_debounce_secs: Set(input.watcher_debounce_secs),
            enabled: Set(input.enabled),
            created_at: Set(now),
            updated_at: Set(now),
        };
        Ok(Sources::insert(model).exec_with_returning(db).await?)
    }

    pub async fn update<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
        user_id: Uuid,
        input: SourceInput,
    ) -> anyhow::Result<Option<sources::Model>> {
        let results = Sources::update_many()
            .filter(sources::Column::Id.eq(id))
            .filter(sources::Column::UserId.eq(user_id))
            .col_expr(sources::Column::Name, Expr::value(input.name))
            .col_expr(sources::Column::SrcSourceId, Expr::value(input.src_source_id))
            .col_expr(sources::Column::SrcSourceType, Expr::value(input.src_source_type))
            .col_expr(sources::Column::DstSourceId, Expr::value(input.dst_source_id))
            .col_expr(sources::Column::DstSourceType, Expr::value(input.dst_source_type))
            .col_expr(sources::Column::SrcPath, Expr::value(input.src_path))
            .col_expr(sources::Column::DstPath, Expr::value(input.dst_path))
            .col_expr(sources::Column::Encoder, Expr::value(input.encoder))
            .col_expr(sources::Column::EncoderParams, Expr::value(input.encoder_params))
            .col_expr(
                sources::Column::PreflightBitrateRef,
                Expr::value(input.preflight_bitrate_ref),
            )
            .col_expr(
                sources::Column::HybridHealthCheck,
                Expr::value(input.hybrid_health_check),
            )
            .col_expr(sources::Column::MaxGapSeconds, Expr::value(input.max_gap_seconds))
            .col_expr(
                sources::Column::MaxGroupDurationSeconds,
                Expr::value(input.max_group_duration_seconds),
            )
            .col_expr(sources::Column::MonthlySubdirs, Expr::value(input.monthly_subdirs))
            .col_expr(
                sources::Column::AllowCombinedInput,
                Expr::value(input.allow_combined_input),
            )
            .col_expr(sources::Column::NoBrokenSplit, Expr::value(input.no_broken_split))
            .col_expr(sources::Column::TriggerMode, Expr::value(input.trigger_mode))
            .col_expr(sources::Column::CronExpr, Expr::value(input.cron_expr))
            .col_expr(
                sources::Column::WatcherDebounceSecs,
                Expr::value(input.watcher_debounce_secs),
            )
            .col_expr(sources::Column::Enabled, Expr::value(input.enabled))
            .col_expr(sources::Column::UpdatedAt, Expr::value(Utc::now().fixed_offset()))
            .exec_with_returning(db)
            .await?;
        Ok(results.into_iter().next())
    }

    pub async fn delete<C: ConnectionTrait>(db: &C, id: Uuid, user_id: Uuid) -> anyhow::Result<u64> {
        let res = Sources::delete_many()
            .filter(sources::Column::Id.eq(id))
            .filter(sources::Column::UserId.eq(user_id))
            .exec(db)
            .await?;
        Ok(res.rows_affected)
    }
}
