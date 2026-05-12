use chrono::Utc;
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, sea_query::OnConflict};
use uuid::Uuid;

use crate::db::entities::scan_cache::{self, Entity as ScanCache};

#[derive(Debug, Clone)]
pub struct CacheUpsert {
    pub source_id: Uuid,
    pub abs_path: String,
    pub size: Option<i64>,
    pub mtime_ns: Option<i64>,
    pub ctime_ns: Option<i64>,
    pub duration_secs: Option<f64>,
    pub healthy: bool,
    pub broken: bool,
    pub codec: Option<String>,
    pub format_bps: Option<i64>,
    pub size_bytes: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

pub struct ScanCacheRepo;

impl ScanCacheRepo {
    pub async fn find<C: ConnectionTrait>(
        db: &C,
        source_id: Uuid,
        abs_path: &str,
        size: Option<i64>,
        mtime_ns: Option<i64>,
        ctime_ns: Option<i64>,
    ) -> anyhow::Result<Option<scan_cache::Model>> {
        Ok(ScanCache::find()
            .filter(scan_cache::Column::SourceId.eq(source_id))
            .filter(scan_cache::Column::AbsPath.eq(abs_path))
            .filter(scan_cache::Column::Size.eq(size))
            .filter(scan_cache::Column::MtimeNs.eq(mtime_ns))
            .filter(scan_cache::Column::CtimeNs.eq(ctime_ns))
            .one(db)
            .await?)
    }

    pub async fn upsert<C: ConnectionTrait>(db: &C, input: CacheUpsert) -> anyhow::Result<scan_cache::Model> {
        let active = scan_cache::ActiveModel {
            source_id: Set(input.source_id),
            abs_path: Set(input.abs_path),
            size: Set(input.size),
            mtime_ns: Set(input.mtime_ns),
            ctime_ns: Set(input.ctime_ns),
            duration_secs: Set(input.duration_secs),
            healthy: Set(input.healthy),
            broken: Set(input.broken),
            probed_at: Set(Utc::now().into()),
            codec: Set(input.codec),
            format_bps: Set(input.format_bps),
            size_bytes: Set(input.size_bytes),
            width: Set(input.width),
            height: Set(input.height),
        };
        Ok(ScanCache::insert(active)
            .on_conflict(
                OnConflict::columns([scan_cache::Column::SourceId, scan_cache::Column::AbsPath])
                    .update_columns([
                        scan_cache::Column::Size,
                        scan_cache::Column::MtimeNs,
                        scan_cache::Column::CtimeNs,
                        scan_cache::Column::DurationSecs,
                        scan_cache::Column::Healthy,
                        scan_cache::Column::Broken,
                        scan_cache::Column::ProbedAt,
                        scan_cache::Column::Codec,
                        scan_cache::Column::FormatBps,
                        scan_cache::Column::SizeBytes,
                        scan_cache::Column::Width,
                        scan_cache::Column::Height,
                    ])
                    .to_owned(),
            )
            .exec_with_returning(db)
            .await?)
    }
}
