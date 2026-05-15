use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use chrono::Datelike;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use tokimo_bus_client::BusClient;
use tokimo_bus_protocol::CallerCtx;
use tokimo_package_ffmpeg::{
    DirectInput as FfmpegDirectInput, TranscodeOptions as FfmpegTranscodeOptions,
    cancellation_token as ffmpeg_cancellation_token,
};
use tokimo_vfs::{FileInfo, Vfs};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};

use crate::{
    core::{
        duration::DurationResolver,
        encoder::{EncodeProfile, EncoderRegistry},
        ffmpeg::{CancellationToken, FfmpegRunner, WarningTracker},
        grouping::{create_combined_filename, group_by_time, item_from_path},
        naming::{is_video_file, parse_filename},
        report::RunReport,
        vfs_source,
    },
    db::{
        entities::{merge_runs, sources},
        repos::{
            merge_runs_repo::{GroupUpdate, MergeRunsRepo},
            warnings_repo::WarningsRepo,
        },
    },
};

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub run_id: uuid::Uuid,
    pub phase: String,
    pub group_count: usize,
    pub ok_count: usize,
    pub failed_count: usize,
    pub current_file: Option<String>,
    pub percent: f64,
}

#[derive(Debug, Clone)]
struct VfsFile {
    info: FileInfo,
    path: PathBuf,
}

pub struct Pipeline {
    db: DatabaseConnection,
    paths: Arc<tokio::sync::RwLock<crate::core::ffmpeg::FfmpegPaths>>,
    progress: tokio::sync::broadcast::Sender<ProgressEvent>,
    bus: Arc<std::sync::OnceLock<Arc<BusClient>>>,
    user_id: uuid::Uuid,
}

impl Pipeline {
    pub fn new(
        db: DatabaseConnection,
        paths: Arc<tokio::sync::RwLock<crate::core::ffmpeg::FfmpegPaths>>,
        progress: tokio::sync::broadcast::Sender<ProgressEvent>,
        bus: Arc<std::sync::OnceLock<Arc<BusClient>>>,
        user_id: uuid::Uuid,
    ) -> Self {
        Self {
            db,
            paths,
            progress,
            bus,
            user_id,
        }
    }

    async fn bus_invoke(&self, method: &str, payload: serde_json::Value) {
        let Some(client) = self.bus.get() else { return };
        let caller = CallerCtx {
            user_id: Some(self.user_id.to_string()),
            request_id: uuid::Uuid::new_v4().to_string(),
            workspace: None,
        };
        match serde_json::to_vec(&payload) {
            Ok(bytes) => {
                if let Err(error) = client.invoke("task_queue", method, bytes, caller).await {
                    tracing::warn!(%error, method, "dashcam-archive: task_queue bus call failed");
                }
            }
            Err(error) => tracing::warn!(%error, "dashcam-archive: task_queue payload serialize failed"),
        }
    }

    pub async fn run(
        &self,
        run: merge_runs::Model,
        source: sources::Model,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        MergeRunsRepo::set_status(&self.db, run.id, "running").await?;
        self.bus_invoke(
            "upsert_job",
            serde_json::json!({
                "job_id": run.id,
                "app_id": "dashcam-archive",
                "user_id": self.user_id,
                "title": format!("{} (归并)", source.name),
                "status": "running",
                "progress": 0.0,
                "metadata": {},
            }),
        )
        .await;
        let state_dir = std::env::var("TOKIMO_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
        let staging = PathBuf::from(state_dir)
            .join("dashcam-archive")
            .join("staging")
            .join(run.id.to_string());
        tokio::fs::create_dir_all(&staging).await?;

        let result = self.run_vfs(run.clone(), source, cancel, &staging).await;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        result
    }

    async fn run_vfs(
        &self,
        run: merge_runs::Model,
        source: sources::Model,
        cancel: CancellationToken,
        staging: &Path,
    ) -> anyhow::Result<()> {
        let src_vfs = vfs_source::build_vfs(&self.db, source.src_source_id, &source.src_source_type).await?;
        let dst_vfs = vfs_source::build_vfs(&self.db, source.dst_source_id, &source.dst_source_type).await?;
        let input = PathBuf::from(&source.src_path);
        let output = PathBuf::from(&source.dst_path);
        ensure_vfs_parent(&dst_vfs, &output).await?;
        let _ = dst_vfs.mkdir(&output).await;
        self.emit(run.id, "scan", 0, 0, 0, None, 0.0);

        let files = scan_vfs(&src_vfs, &input).await?;
        let total_files = files.len().max(1);
        let file_size_map: std::collections::HashMap<PathBuf, u64> =
            files.iter().map(|f| (f.path.clone(), f.info.size)).collect();
        let resolver = DurationResolver::new(self.db.clone(), 4);
        let mut videos = Vec::new();
        for (idx, file) in files.iter().enumerate() {
            if cancel.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            if is_video_file(&file.path) {
                let probe = resolver.resolve_vfs(source.id, &src_vfs, &file.info).await?;
                videos.push(item_from_path(file.path.clone(), probe.duration_secs.map(secs_to_ms)));
            } else {
                copy_non_video_vfs(&src_vfs, &dst_vfs, &input, &output, file).await?;
            }
            let percent = (idx + 1) as f64 / total_files as f64 * 20.0;
            self.emit(
                run.id,
                "scan",
                0,
                0,
                0,
                Some(file.path.to_string_lossy().to_string()),
                percent,
            );
            self.bus_invoke(
                "update_progress",
                serde_json::json!({
                    "job_id": run.id,
                    "progress": percent / 100.0,
                }),
            )
            .await;
        }

        let gap = Duration::from_secs(u64::try_from(source.max_gap_seconds.max(1)).unwrap_or(60));
        let groups = group_by_time(videos, gap);
        let runner = FfmpegRunner::new(self.paths.read().await.clone());
        let mut ok_count = 0_usize;
        let mut downgraded_count = 0_usize;
        let mut failed_count = 0_usize;
        let mut total_bytes_in = 0_i64;
        let mut total_bytes_out = 0_i64;

        for (idx, group) in groups.iter().enumerate() {
            if cancel.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            let out_dir = output_vfs_dir_for_group(
                &dst_vfs,
                &output,
                &input,
                group.files.first().map(|item| item.path.as_path()),
                &source.monthly_subdirs,
            )
            .await?;
            let out_vfs = vfs_join(&out_dir, Path::new(&create_combined_filename(group)));
            let decision = decision_for_encoder(&source.encoder, group.files.len());

            let group_model = MergeRunsRepo::create_group(
                &self.db,
                run.id,
                group.camera.clone(),
                out_vfs.to_string_lossy().to_string(),
                decision.to_string(),
            )
            .await?;

            let group_stage = staging.join(format!("group-{idx}"));
            tokio::fs::create_dir_all(&group_stage).await?;
            let local_out = group_stage.join(create_combined_filename(group));

            let start_dt = group.files.first().and_then(|f| parse_filename(&f.path).timestamp);
            let end_dt = group.files.last().and_then(|f| parse_filename(&f.path).timestamp);

            let mut failure_inputs = Vec::new();
            let (result, bytes_in) = if group.files.len() == 1 && runner.available() {
                let item = &group.files[0];
                let file_size = file_size_map.get(&item.path).copied().unwrap_or(0);
                let direct_result = try_direct_input(
                    &src_vfs,
                    item,
                    file_size,
                    &local_out,
                    &source.encoder,
                    cancel.clone(),
                )
                .await;
                match direct_result {
                    Ok(r) => (Ok(r), file_size as i64),
                    Err(ref e) if e.to_string().contains("cancelled") => {
                        (Err(anyhow::anyhow!("cancelled")), file_size as i64)
                    }
                    Err(e) => {
                        tracing::warn!("direct input failed ({}), falling back to staging+concat", e);
                        let inputs = stage_group_files(&src_vfs, &group.files, &group_stage).await?;
                        let bsz = sum_file_sizes(&inputs).unwrap_or(0);
                        failure_inputs = inputs.clone();
                        let fallback = if runner.available() {
                            run_with_encoder(&runner, &source, &inputs, &local_out, cancel.clone())
                                .await
                                .map(|(t, _)| (t, true))
                        } else {
                            Err(anyhow::anyhow!("ffmpeg binary is unavailable"))
                        };
                        (fallback, bsz)
                    }
                }
            } else {
                let inputs = stage_group_files(&src_vfs, &group.files, &group_stage).await?;
                let bsz = sum_file_sizes(&inputs).unwrap_or(0);
                failure_inputs = inputs.clone();
                let r = if runner.available() {
                    run_with_encoder(&runner, &source, &inputs, &local_out, cancel.clone()).await
                } else {
                    Err(anyhow::anyhow!("ffmpeg binary is unavailable"))
                };
                (r, bsz)
            };
            total_bytes_in += bytes_in;

            match result {
                Ok((tracker, fallback_used)) => {
                    let warnings = tracker.warnings();
                    for warning in warnings {
                        WarningsRepo::add(
                            &self.db,
                            group_model.id,
                            warning.category.clone(),
                            1,
                            Some(warning.message.clone()),
                        )
                        .await?;
                    }
                    upload_local_file(&dst_vfs, &local_out, &out_vfs).await?;
                    let warning_level = if fallback_used || !warnings.is_empty() {
                        "warn"
                    } else {
                        "clean"
                    };
                    let status = if fallback_used { "downgraded" } else { "ok" };
                    if fallback_used {
                        downgraded_count += 1;
                    } else {
                        ok_count += 1;
                    }
                    let duration = group.files.iter().filter_map(|item| item.duration_ms).sum::<i64>() as f64 / 1000.0;
                    let bytes_out = file_size(&local_out).unwrap_or(0);
                    total_bytes_out += bytes_out;
                    MergeRunsRepo::update_group(
                        &self.db,
                        group_model.id,
                        GroupUpdate {
                            start_dt,
                            end_dt,
                            status: status.to_string(),
                            warning_level: warning_level.to_string(),
                            duration_secs: Some(duration),
                            bytes_in: Some(bytes_in),
                            bytes_out: Some(bytes_out),
                            abort_reason: None,
                        },
                    )
                    .await?;
                }
                Err(error) => {
                    failed_count += 1;
                    upload_failure_log(&dst_vfs, &out_vfs, &failure_inputs, &error.to_string()).await?;
                    MergeRunsRepo::update_group(
                        &self.db,
                        group_model.id,
                        GroupUpdate {
                            start_dt,
                            end_dt,
                            status: "failed".to_string(),
                            warning_level: "fatal".to_string(),
                            duration_secs: None,
                            bytes_in: Some(bytes_in),
                            bytes_out: None,
                            abort_reason: Some(error.to_string()),
                        },
                    )
                    .await?;
                }
            }
            let percent = 20.0 + (idx + 1) as f64 / groups.len().max(1) as f64 * 80.0;
            self.emit(
                run.id,
                "merge",
                groups.len(),
                ok_count,
                failed_count,
                Some(out_vfs.to_string_lossy().to_string()),
                percent,
            );
            self.bus_invoke(
                "update_progress",
                serde_json::json!({
                    "job_id": run.id,
                    "progress": percent / 100.0,
                }),
            )
            .await;
        }

        MergeRunsRepo::update_counters(
            &self.db,
            run.id,
            groups.len() as i32,
            ok_count as i32,
            downgraded_count as i32,
            failed_count as i32,
            Some(total_bytes_in),
            Some(total_bytes_out),
        )
        .await?;

        upload_report(
            &dst_vfs,
            &vfs_join(&output, Path::new("_transcode_warnings.txt")),
            &RunReport {
                run_id: run.id,
                status: "succeeded",
                files: total_files,
                groups: groups.len(),
                warnings: failed_count,
            },
        )
        .await?;
        MergeRunsRepo::set_status(&self.db, run.id, "succeeded").await?;
        self.emit(run.id, "succeeded", groups.len(), ok_count, failed_count, None, 100.0);
        self.bus_invoke(
            "complete_job",
            serde_json::json!({
                "job_id": run.id,
                "status": "completed",
            }),
        )
        .await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &self,
        run_id: uuid::Uuid,
        phase: &str,
        group_count: usize,
        ok_count: usize,
        failed_count: usize,
        current_file: Option<String>,
        percent: f64,
    ) {
        let _ = self.progress.send(ProgressEvent {
            run_id,
            phase: phase.to_string(),
            group_count,
            ok_count,
            failed_count,
            current_file,
            percent,
        });
    }
}

async fn scan_vfs(vfs: &Vfs, root: &Path) -> anyhow::Result<Vec<VfsFile>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for info in vfs.list(&dir).await? {
            let path = PathBuf::from(&info.path);
            if info.is_dir {
                stack.push(path);
            } else if info.size > 0 {
                files.push(VfsFile { info, path });
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

async fn copy_non_video_vfs(
    src_vfs: &Arc<Vfs>,
    dst_vfs: &Vfs,
    root: &Path,
    output: &Path,
    file: &VfsFile,
) -> anyhow::Result<()> {
    let rel = relative_vfs_path(&file.path, root);
    let dest = vfs_join(output, &rel);
    if dst_vfs.stat(&dest).await.is_ok() {
        return Ok(());
    }
    ensure_vfs_parent(dst_vfs, &dest).await?;

    let (tx, rx) = mpsc::channel(8);
    let stream_vfs = Arc::clone(src_vfs);
    let stream_path = file.path.clone();
    let stream_task = tokio::spawn(async move {
        stream_vfs.stream_to(&stream_path, 0, None, tx).await;
    });

    if dst_vfs.has_put_stream(&dest).await {
        let put_result = dst_vfs.put_stream(&dest, file.info.size, rx).await;
        stream_task.await?;
        put_result?;
        return Ok(());
    }

    let mut data = Vec::new();
    let mut rx = rx;
    while let Some(chunk) = rx.recv().await {
        data.extend_from_slice(&chunk);
    }
    stream_task.await?;
    dst_vfs.put(&dest, data).await?;
    Ok(())
}

async fn stage_group_files(
    src_vfs: &Arc<Vfs>,
    files: &[crate::core::grouping::VideoItem],
    group_stage: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut staged = Vec::with_capacity(files.len());
    for (idx, item) in files.iter().enumerate() {
        let ext = item
            .path
            .extension()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "mp4".to_string());
        let local = group_stage.join(format!("{idx:04}.{ext}"));
        copy_vfs_to_local(src_vfs, &item.path, &local).await?;
        staged.push(local);
    }
    Ok(staged)
}

async fn copy_vfs_to_local(vfs: &Arc<Vfs>, vfs_path: &Path, local_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = local_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::File::create(local_path).await?;
    let (tx, mut rx) = mpsc::channel(8);
    let stream_vfs = Arc::clone(vfs);
    let stream_path = vfs_path.to_path_buf();
    let stream_task = tokio::spawn(async move {
        stream_vfs.stream_to(&stream_path, 0, None, tx).await;
    });

    let mut write_result = Ok(());
    while let Some(chunk) = rx.recv().await {
        if let Err(error) = file.write_all(&chunk).await {
            write_result = Err(error);
            break;
        }
    }
    drop(rx);
    stream_task.await?;
    write_result?;
    file.flush().await?;
    Ok(())
}

async fn upload_local_file(dst_vfs: &Vfs, local_path: &Path, dst_path: &Path) -> anyhow::Result<()> {
    ensure_vfs_parent(dst_vfs, dst_path).await?;
    let size = tokio::fs::metadata(local_path).await?.len();
    if dst_vfs.has_put_stream(dst_path).await {
        let (tx, rx) = mpsc::channel(8);
        let local = local_path.to_path_buf();
        let sender = tokio::spawn(async move {
            let mut file = tokio::fs::File::open(local).await?;
            let mut buf = vec![0; 1024 * 1024];
            loop {
                let read = file.read(&mut buf).await?;
                if read == 0 {
                    break;
                }
                if tx.send(buf[..read].to_vec()).await.is_err() {
                    break;
                }
            }
            anyhow::Ok(())
        });
        match dst_vfs.put_stream(dst_path, size, rx).await {
            Ok(()) => {
                sender.await??;
                return Ok(());
            }
            Err(_) => {
                let _ = sender.await;
            }
        }
    }
    dst_vfs.put(dst_path, tokio::fs::read(local_path).await?).await?;
    Ok(())
}

async fn ensure_vfs_parent(vfs: &Vfs, path: &Path) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let mut dirs: Vec<PathBuf> = parent
        .ancestors()
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .collect();
    dirs.reverse();
    for dir in dirs {
        let _ = vfs.mkdir(&dir).await;
    }
    Ok(())
}

async fn output_vfs_dir_for_group(
    dst_vfs: &Vfs,
    base: &Path,
    root: &Path,
    first: Option<&Path>,
    monthly: &str,
) -> anyhow::Result<PathBuf> {
    let Some(first) = first else {
        ensure_vfs_parent(dst_vfs, base).await?;
        let _ = dst_vfs.mkdir(base).await;
        return Ok(base.to_path_buf());
    };
    let rel_dir = relative_vfs_path(first, root)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    let parsed = parse_filename(first);
    let use_monthly = monthly == "on" || (monthly == "auto" && parsed.camera.starts_with("XIAOMI_"));
    let dir = if use_monthly {
        parsed.timestamp.map_or_else(
            || vfs_join(base, &rel_dir),
            |ts| vfs_join(base, Path::new(&format!("{:04}{:02}", ts.year(), ts.month()))),
        )
    } else {
        vfs_join(base, &rel_dir)
    };
    ensure_vfs_parent(dst_vfs, &dir).await?;
    let _ = dst_vfs.mkdir(&dir).await;
    Ok(dir)
}

async fn upload_failure_log(dst_vfs: &Vfs, out: &Path, inputs: &[PathBuf], reason: &str) -> anyhow::Result<()> {
    let mut text = format!(
        "merge failed: {}\nreason: {reason}\nmembers: {}\n",
        out.display(),
        inputs.len()
    );
    for input in inputs {
        text.push_str(&format!("  {}\n", input.display()));
    }
    let log_path = out.with_extension("mp4.failure.log");
    ensure_vfs_parent(dst_vfs, &log_path).await?;
    dst_vfs.put(&log_path, text.into_bytes()).await?;
    Ok(())
}

async fn upload_report(dst_vfs: &Vfs, path: &Path, report: &RunReport<'_>) -> anyhow::Result<()> {
    let body = format!(
        "# 转码警告汇总\n\nrun_id: {}\nstatus: {}\nfiles: {}\ngroups: {}\nwarnings: {}\n",
        report.run_id, report.status, report.files, report.groups, report.warnings
    );
    ensure_vfs_parent(dst_vfs, path).await?;
    dst_vfs.put(path, body.into_bytes()).await?;
    Ok(())
}

fn relative_vfs_path(path: &Path, root: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn vfs_join(base: &Path, rel: &Path) -> PathBuf {
    let rel = rel.to_string_lossy();
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        base.to_path_buf()
    } else {
        base.join(rel)
    }
}

fn secs_to_ms(secs: f64) -> i64 {
    (secs * 1000.0).round() as i64
}

fn file_size(path: &Path) -> Option<i64> {
    std::fs::metadata(path).ok().and_then(|m| i64::try_from(m.len()).ok())
}
fn sum_file_sizes(paths: &[PathBuf]) -> Option<i64> {
    Some(paths.iter().filter_map(|p| file_size(p)).sum())
}

fn decision_for_encoder(encoder: &str, input_count: usize) -> &'static str {
    match encoder {
        "nvenc-h265" => "encode_nvenc",
        "x265-veryslow" => "encode_x265",
        "copy-only" | "auto" | "current" if input_count == 1 => "single_copy",
        _ => "copy",
    }
}

async fn run_with_encoder(
    runner: &FfmpegRunner,
    source: &sources::Model,
    inputs: &[PathBuf],
    out: &Path,
    cancel: CancellationToken,
) -> anyhow::Result<(WarningTracker, bool)> {
    match source.encoder.as_str() {
        "nvenc-h265" | "x265-veryslow" => {
            let Some(ffmpeg) = runner.paths().ffmpeg.as_deref() else {
                anyhow::bail!("ffmpeg binary is unavailable");
            };
            let registry = EncoderRegistry::new_with_builtins(
                Path::new(ffmpeg),
                runner.paths().library_dir.as_deref().map(Path::new),
            );
            let Some(encoder) = registry.get(&source.encoder) else {
                return runner
                    .concat_copy(inputs, out, cancel)
                    .await
                    .map(|tracker| (tracker, true));
            };
            let profile = EncodeProfile::default();
            match runner
                .concat_encode(inputs, out, &encoder.encode_args(&profile), cancel.clone())
                .await
            {
                Ok(tracker) => Ok((tracker, false)),
                Err(error) if !error.to_string().contains("cancelled") => {
                    let tracker = runner.concat_copy(inputs, out, cancel).await?;
                    Ok((tracker, true))
                }
                Err(error) => Err(error),
            }
        }
        _ => runner
            .concat_copy(inputs, out, cancel)
            .await
            .map(|tracker| (tracker, false)),
    }
}


async fn try_direct_input(
    src_vfs: &Arc<Vfs>,
    item: &crate::core::grouping::VideoItem,
    file_size: u64,
    local_out: &Path,
    encoder: &str,
    cancel: CancellationToken,
) -> anyhow::Result<(WarningTracker, bool)> {
    let read_at = src_vfs.to_read_at(&item.path).await;
    let filename_hint = item.path.file_name().map(|n| n.to_string_lossy().into_owned());
    let direct_input = FfmpegDirectInput::from_read_at(read_at, file_size, filename_hint, Some(32 * 1024 * 1024));

    let profile = EncodeProfile::default();
    let (video_codec, audio_codec, preset, crf): (String, String, String, Option<u32>) = match encoder {
        "nvenc-h265" => (
            "hevc_nvenc".to_string(),
            "copy".to_string(),
            profile.preset.clone(),
            None,
        ),
        "x265-veryslow" => (
            "libx265".to_string(),
            "copy".to_string(),
            "veryslow".to_string(),
            Some(u32::from(profile.crf)),
        ),
        _ => (
            "copy".to_string(),
            "copy".to_string(),
            "medium".to_string(),
            None,
        ),
    };

    let ffmpeg_cancel = ffmpeg_cancellation_token();
    let dashcam_cancel_watcher = cancel.clone();
    let ffmpeg_cancel_watcher = ffmpeg_cancel.clone();
    let watcher = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if dashcam_cancel_watcher.is_cancelled() {
                ffmpeg_cancel_watcher.store(true, Ordering::SeqCst);
                break;
            }
        }
    });

    let local_out_buf = local_out.to_path_buf();
    if let Some(parent) = local_out_buf.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let result = tokio::task::spawn_blocking(move || {
        let opts = FfmpegTranscodeOptions {
            input: PathBuf::from("direct"),
            output: local_out_buf,
            video_codec,
            audio_codec,
            preset,
            crf,
            cancel: Some(ffmpeg_cancel),
            direct_input: Some(direct_input),
            ..Default::default()
        };
        tokimo_package_ffmpeg::transcode(&opts)
    })
    .await;
    watcher.abort();

    match result {
        Err(join_err) => Err(anyhow::anyhow!("spawn_blocking panicked: {join_err}")),
        Ok(Err(ffmpeg_err)) => {
            if cancel.is_cancelled() {
                anyhow::bail!("ffmpeg cancelled");
            }
            Err(anyhow::anyhow!("direct transcode failed: {ffmpeg_err}"))
        }
        Ok(Ok(())) => Ok((WarningTracker::default(), false)),
    }
}
