use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
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
        let Some(existing) = Self::get(db, id, user_id).await? else {
            return Ok(None);
        };
        let mut model: sources::ActiveModel = existing.into();
        model.name = Set(input.name);
        model.src_source_id = Set(input.src_source_id);
        model.src_source_type = Set(input.src_source_type);
        model.dst_source_id = Set(input.dst_source_id);
        model.dst_source_type = Set(input.dst_source_type);
        model.src_path = Set(input.src_path);
        model.dst_path = Set(input.dst_path);
        model.encoder = Set(input.encoder);
        model.encoder_params = Set(input.encoder_params);
        model.preflight_bitrate_ref = Set(input.preflight_bitrate_ref);
        model.hybrid_health_check = Set(input.hybrid_health_check);
        model.max_gap_seconds = Set(input.max_gap_seconds);
        model.max_group_duration_seconds = Set(input.max_group_duration_seconds);
        model.monthly_subdirs = Set(input.monthly_subdirs);
        model.allow_combined_input = Set(input.allow_combined_input);
        model.no_broken_split = Set(input.no_broken_split);
        model.trigger_mode = Set(input.trigger_mode);
        model.cron_expr = Set(input.cron_expr);
        model.watcher_debounce_secs = Set(input.watcher_debounce_secs);
        model.enabled = Set(input.enabled);
        model.updated_at = Set(Utc::now().into());
        Ok(Some(model.update(db).await?))
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
