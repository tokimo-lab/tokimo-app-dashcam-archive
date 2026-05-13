use std::{path::Path, sync::Arc};

use tokimo_package_ffmpeg::{DirectInput, probe_direct};
use tokimo_vfs::FileInfo;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    core::naming::is_video_file,
    db::repos::scan_cache_repo::{CacheUpsert, ScanCacheRepo},
};

#[derive(Debug, Clone)]
pub struct FileFingerprint {
    pub size: Option<i64>,
    pub mtime_ns: Option<i64>,
    pub ctime_ns: Option<i64>,
}

pub fn vfs_fingerprint(info: &FileInfo) -> FileFingerprint {
    FileFingerprint {
        size: i64::try_from(info.size).ok(),
        mtime_ns: info.modified.and_then(|value| value.timestamp_nanos_opt()),
        ctime_ns: None,
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProbeResult {
    pub duration_secs: Option<f64>,
    pub broken: bool,
    pub codec: Option<String>,
    pub format_bps: Option<i64>,
    pub size_bytes: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

#[derive(Clone)]
pub struct DurationResolver {
    db: sea_orm::DatabaseConnection,
    semaphore: Arc<Semaphore>,
}

impl DurationResolver {
    pub fn new(db: sea_orm::DatabaseConnection, concurrency: usize) -> Self {
        Self {
            db,
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
        }
    }

    pub async fn resolve_vfs(
        &self,
        source_id: Uuid,
        vfs: &tokimo_vfs::Vfs,
        info: &FileInfo,
    ) -> anyhow::Result<ProbeResult> {
        let stat = vfs_fingerprint(info);
        let abs_path = info.path.clone();
        let is_video = is_video_file(Path::new(&info.path));
        self.resolve_with_fingerprint(
            source_id,
            &abs_path,
            stat,
            |semaphore| async move {
                let permit = semaphore.acquire().await?;
                let probe = probe_via_direct_input(vfs, info).await;
                drop(permit);
                Ok(probe)
            },
            is_video,
        )
        .await
    }

    async fn resolve_with_fingerprint<F, Fut>(
        &self,
        source_id: Uuid,
        abs_path: &str,
        stat: FileFingerprint,
        probe_fn: F,
        should_probe: bool,
    ) -> anyhow::Result<ProbeResult>
    where
        F: FnOnce(Arc<Semaphore>) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<ProbeResult>>,
    {
        if let Some(cached) =
            ScanCacheRepo::find(&self.db, source_id, abs_path, stat.size, stat.mtime_ns, stat.ctime_ns).await?
        {
            return Ok(ProbeResult {
                duration_secs: cached.duration_secs,
                broken: cached.broken,
                codec: cached.codec,
                format_bps: cached.format_bps,
                size_bytes: cached.size_bytes,
                width: cached.width,
                height: cached.height,
            });
        }
        let probe = if should_probe {
            probe_fn(Arc::clone(&self.semaphore)).await?
        } else {
            ProbeResult::default()
        };
        ScanCacheRepo::upsert(
            &self.db,
            CacheUpsert {
                source_id,
                abs_path: abs_path.to_string(),
                size: stat.size,
                mtime_ns: stat.mtime_ns,
                ctime_ns: stat.ctime_ns,
                duration_secs: probe.duration_secs,
                healthy: !probe.broken,
                broken: probe.broken,
                codec: probe.codec.clone(),
                format_bps: probe.format_bps,
                size_bytes: probe.size_bytes,
                width: probe.width,
                height: probe.height,
            },
        )
        .await?;
        Ok(probe)
    }
}

async fn probe_via_direct_input(vfs: &tokimo_vfs::Vfs, info: &FileInfo) -> ProbeResult {
    let read_at = vfs.to_read_at(Path::new(&info.path).to_path_buf()).await;
    let filename_hint = Path::new(&info.path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string());
    let input = DirectInput::from_read_at(read_at, info.size, filename_hint, Some(2 * 1024 * 1024));
    match tokio::task::spawn_blocking(move || probe_direct(input)).await {
        Ok(Ok(media)) => {
            let stream = media.streams.iter().find(|stream| stream.codec_type == "video");
            let duration_secs = Some(media.format.duration_secs()).filter(|value| value.is_finite() && *value > 0.0);
            ProbeResult {
                duration_secs,
                broken: duration_secs.is_none(),
                codec: stream.map(|stream| stream.codec_name.clone()),
                format_bps: media.format.bit_rate.parse::<i64>().ok(),
                size_bytes: media.format.size.parse::<i64>().ok(),
                width: stream.and_then(|stream| stream.video.as_ref()).map(|video| video.width),
                height: stream
                    .and_then(|stream| stream.video.as_ref())
                    .map(|video| video.height),
            }
        }
        _ => ProbeResult {
            broken: true,
            ..ProbeResult::default()
        },
    }
}
