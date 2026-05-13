use std::{path::Path, sync::Arc, time::Duration};

use serde::Deserialize;
use tokio::{process::Command, sync::Semaphore};
use uuid::Uuid;

use crate::{
    core::{ffmpeg::FfmpegPaths, naming::is_video_file},
    db::repos::scan_cache_repo::{CacheUpsert, ScanCacheRepo},
};

#[derive(Debug, Clone)]
pub struct FileFingerprint {
    pub size: Option<i64>,
    pub mtime_ns: Option<i64>,
    pub ctime_ns: Option<i64>,
}

pub fn stat_fingerprint(path: &Path) -> anyhow::Result<FileFingerprint> {
    let meta = std::fs::metadata(path)?;
    let size = i64::try_from(meta.len()).ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileFingerprint {
            size,
            mtime_ns: Some(
                meta.mtime_nsec()
                    .saturating_add(meta.mtime().saturating_mul(1_000_000_000)),
            ),
            ctime_ns: Some(
                meta.ctime_nsec()
                    .saturating_add(meta.ctime().saturating_mul(1_000_000_000)),
            ),
        })
    }
    #[cfg(not(unix))]
    {
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        Ok(FileFingerprint {
            size,
            mtime_ns: modified.and_then(|d| i64::try_from(d.as_nanos()).ok()),
            ctime_ns: None,
        })
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

#[derive(Debug, Deserialize)]
struct FfprobeJson {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
}
#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
}
#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    bit_rate: Option<String>,
    size: Option<String>,
}

pub async fn ffprobe(paths: &FfmpegPaths, path: &Path) -> ProbeResult {
    let Some(ffprobe) = paths.ffprobe.as_ref() else {
        return ProbeResult {
            broken: true,
            ..ProbeResult::default()
        };
    };
    let mut command = Command::new(ffprobe);
    command.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_entries",
        "stream=codec_name,width,height",
        "-show_entries",
        "format=duration,bit_rate,size",
        "-select_streams",
        "v:0",
    ]);
    command.arg(path);
    paths.apply_library_env(&mut command);
    let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(60), command.output()).await else {
        return ProbeResult {
            broken: true,
            ..ProbeResult::default()
        };
    };
    if !output.status.success() {
        return ProbeResult {
            broken: true,
            ..ProbeResult::default()
        };
    }
    let Ok(data) = serde_json::from_slice::<FfprobeJson>(&output.stdout) else {
        return ProbeResult {
            broken: true,
            ..ProbeResult::default()
        };
    };
    let stream = data.streams.as_ref().and_then(|streams| streams.first());
    let duration_secs = data
        .format
        .as_ref()
        .and_then(|fmt| fmt.duration.as_deref())
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0);
    ProbeResult {
        duration_secs,
        broken: duration_secs.is_none(),
        codec: stream.and_then(|s| s.codec_name.clone()),
        format_bps: data
            .format
            .as_ref()
            .and_then(|fmt| fmt.bit_rate.as_deref())
            .and_then(|raw| raw.parse::<i64>().ok()),
        size_bytes: data
            .format
            .as_ref()
            .and_then(|fmt| fmt.size.as_deref())
            .and_then(|raw| raw.parse::<i64>().ok()),
        width: stream.and_then(|s| s.width),
        height: stream.and_then(|s| s.height),
    }
}

#[derive(Clone)]
pub struct DurationResolver {
    db: sea_orm::DatabaseConnection,
    paths: Arc<tokio::sync::RwLock<FfmpegPaths>>,
    semaphore: Arc<Semaphore>,
}

impl DurationResolver {
    pub fn new(
        db: sea_orm::DatabaseConnection,
        paths: Arc<tokio::sync::RwLock<FfmpegPaths>>,
        concurrency: usize,
    ) -> Self {
        Self {
            db,
            paths,
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
        }
    }

    pub async fn resolve(&self, source_id: Uuid, path: &Path) -> anyhow::Result<ProbeResult> {
        let stat = stat_fingerprint(path)?;
        let abs_path = path.to_string_lossy().to_string();
        if let Some(cached) =
            ScanCacheRepo::find(&self.db, source_id, &abs_path, stat.size, stat.mtime_ns, stat.ctime_ns).await?
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
        let mut probe = ProbeResult::default();
        if is_video_file(path) {
            let permit = self.semaphore.acquire().await?;
            let paths = self.paths.read().await.clone();
            probe = ffprobe(&paths, path).await;
            drop(permit);
        }
        ScanCacheRepo::upsert(
            &self.db,
            CacheUpsert {
                source_id,
                abs_path,
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
