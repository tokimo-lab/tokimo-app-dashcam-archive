use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::db::entities::{merge_groups, merge_runs, sources};

pub struct GroupUpdate {
    pub start_dt: Option<DateTime<FixedOffset>>,
    pub end_dt: Option<DateTime<FixedOffset>>,
    pub status: String,
    pub warning_level: String,
    pub duration_secs: Option<f64>,
    pub bytes_in: Option<i64>,
    pub bytes_out: Option<i64>,
    pub abort_reason: Option<String>,
}

pub struct MergeRunsRepo;

impl MergeRunsRepo {
    pub async fn create_run<C: ConnectionTrait>(
        db: &C,
        source_id: Uuid,
        trigger: String,
    ) -> anyhow::Result<merge_runs::Model> {
        let active = merge_runs::ActiveModel {
            id: Set(Uuid::new_v4()),
            source_id: Set(source_id),
            trigger: Set(trigger),
            status: Set("queued".to_string()),
            started_at: Set(Utc::now().into()),
            finished_at: Set(None),
            total_groups: Set(0),
            ok_groups: Set(0),
            downgraded_groups: Set(0),
            failed_groups: Set(0),
            bytes_in: Set(None),
            bytes_out: Set(None),
            folder_breaker_tripped: Set(false),
            log_summary: Set(None),
        };
        Ok(merge_runs::Entity::insert(active).exec_with_returning(db).await?)
    }

    pub async fn get_run<C: ConnectionTrait>(db: &C, id: Uuid) -> anyhow::Result<Option<merge_runs::Model>> {
        Ok(merge_runs::Entity::find_by_id(id).one(db).await?)
    }

    pub async fn get_run_for_user<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Option<merge_runs::Model>> {
        let Some(run) = Self::get_run(db, id).await? else {
            return Ok(None);
        };
        let source = sources::Entity::find_by_id(run.source_id)
            .filter(sources::Column::UserId.eq(user_id))
            .one(db)
            .await?;
        if source.is_none() {
            return Ok(None);
        }
        Ok(Some(run))
    }

    pub async fn list_for_source<C: ConnectionTrait>(
        db: &C,
        source_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<merge_runs::Model>> {
        use crate::db::repos::sources_repo::SourcesRepo;
        if SourcesRepo::get(db, source_id, user_id).await?.is_none() {
            return Ok(Vec::new());
        }
        Ok(merge_runs::Entity::find()
            .filter(merge_runs::Column::SourceId.eq(source_id))
            .order_by_desc(merge_runs::Column::StartedAt)
            .all(db)
            .await?
            .into_iter()
            .take(50)
            .collect())
    }

    pub async fn set_status<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
        status: &str,
    ) -> anyhow::Result<Option<merge_runs::Model>> {
        let Some(existing) = Self::get_run(db, id).await? else {
            return Ok(None);
        };
        let mut active: merge_runs::ActiveModel = existing.into();
        active.status = Set(status.to_string());
        if matches!(status, "succeeded" | "failed" | "cancelled") {
            active.finished_at = Set(Some(Utc::now().into()));
        }
        Ok(Some(active.update(db).await?))
    }

    pub async fn update_counters<C: ConnectionTrait>(
        db: &C,
        id: Uuid,
        total_groups: i32,
        ok_groups: i32,
        downgraded_groups: i32,
        failed_groups: i32,
        bytes_in: Option<i64>,
        bytes_out: Option<i64>,
    ) -> anyhow::Result<()> {
        merge_runs::Entity::update_many()
            .col_expr(
                merge_runs::Column::TotalGroups,
                sea_orm::sea_query::Expr::value(total_groups),
            )
            .col_expr(merge_runs::Column::OkGroups, sea_orm::sea_query::Expr::value(ok_groups))
            .col_expr(
                merge_runs::Column::DowngradedGroups,
                sea_orm::sea_query::Expr::value(downgraded_groups),
            )
            .col_expr(
                merge_runs::Column::FailedGroups,
                sea_orm::sea_query::Expr::value(failed_groups),
            )
            .col_expr(merge_runs::Column::BytesIn, sea_orm::sea_query::Expr::value(bytes_in))
            .col_expr(merge_runs::Column::BytesOut, sea_orm::sea_query::Expr::value(bytes_out))
            .filter(merge_runs::Column::Id.eq(id))
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn create_group<C: ConnectionTrait>(
        db: &C,
        run_id: Uuid,
        camera_key: String,
        output_path: String,
        decision: String,
    ) -> anyhow::Result<merge_groups::Model> {
        let now: DateTime<FixedOffset> = Utc::now().into();
        let active = merge_groups::ActiveModel {
            id: Set(Uuid::new_v4()),
            run_id: Set(run_id),
            camera_key: Set(camera_key),
            start_dt: Set(None),
            end_dt: Set(None),
            output_path: Set(output_path),
            decision: Set(decision),
            status: Set("ok".to_string()),
            warning_level: Set("clean".to_string()),
            bytes_in: Set(None),
            bytes_out: Set(None),
            duration_secs: Set(None),
            abort_reason: Set(None),
            created_at: Set(now),
        };
        Ok(merge_groups::Entity::insert(active).exec_with_returning(db).await?)
    }

    pub async fn update_group<C: ConnectionTrait>(db: &C, id: Uuid, update: GroupUpdate) -> anyhow::Result<()> {
        merge_groups::Entity::update_many()
            .col_expr(
                merge_groups::Column::StartDt,
                sea_orm::sea_query::Expr::value(update.start_dt),
            )
            .col_expr(
                merge_groups::Column::EndDt,
                sea_orm::sea_query::Expr::value(update.end_dt),
            )
            .col_expr(
                merge_groups::Column::Status,
                sea_orm::sea_query::Expr::value(update.status),
            )
            .col_expr(
                merge_groups::Column::WarningLevel,
                sea_orm::sea_query::Expr::value(update.warning_level),
            )
            .col_expr(
                merge_groups::Column::DurationSecs,
                sea_orm::sea_query::Expr::value(update.duration_secs),
            )
            .col_expr(
                merge_groups::Column::BytesIn,
                sea_orm::sea_query::Expr::value(update.bytes_in),
            )
            .col_expr(
                merge_groups::Column::BytesOut,
                sea_orm::sea_query::Expr::value(update.bytes_out),
            )
            .col_expr(
                merge_groups::Column::AbortReason,
                sea_orm::sea_query::Expr::value(update.abort_reason),
            )
            .filter(merge_groups::Column::Id.eq(id))
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn list_groups<C: ConnectionTrait>(db: &C, run_id: Uuid) -> anyhow::Result<Vec<merge_groups::Model>> {
        Ok(merge_groups::Entity::find()
            .filter(merge_groups::Column::RunId.eq(run_id))
            .order_by_asc(merge_groups::Column::CreatedAt)
            .all(db)
            .await?)
    }
}
