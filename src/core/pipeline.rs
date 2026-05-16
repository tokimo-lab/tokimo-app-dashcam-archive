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
        encoder::{EncodeProfile, EncoderRegistry, X265_DEFAULT_CRF},
        ffmpeg::{
            CancellationToken, FfmpegRunError, FfmpegRunner, WarningTracker, default_concat_input_flags,
            nvenc_concat_input_flags,
        },
        grouping::{ScanEntry, create_combined_filename, group_by_time, item_from_probe},
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
        if !source.allow_combined_input {
            let src_path_lower = source.src_path.trim_end_matches(['/', '\\']).to_lowercase();
            if src_path_lower.contains("_combined") {
                anyhow::bail!("_Combined input rejected (set allow_combined_input=true to override)");
            }
        }

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
                if probe.broken {
                    videos.push(ScanEntry::Broken(file.path.clone()));
                } else {
                    videos.push(ScanEntry::Video(item_from_probe(
                        file.path.clone(),
                        probe.duration_secs.map(secs_to_ms),
                        probe.codec,
                        probe.format_bps,
                        probe.size_bytes,
                    )));
                }
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

        let gap = if source.max_gap_seconds > 0 {
            Some(Duration::from_secs(u64::try_from(source.max_gap_seconds).unwrap_or(60)))
        } else {
            None
        };

        let mut broken_paths = videos
            .iter()
            .filter_map(|entry| match entry {
                ScanEntry::Video(_) => None,
                ScanEntry::Broken(path) => Some(path.display().to_string()),
            })
            .collect::<Vec<_>>();

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

            let had_broken_warnings = !broken_paths.is_empty();
            let group_warning_examples = broken_paths
                .drain(..)
                .map(|broken_path| {
                    (
                        "broken_file_skipped".to_string(),
                        format!("broken file skipped: {broken_path}"),
                    )
                })
                .collect::<Vec<_>>();

            let group_stage = staging.join(format!("group-{idx}"));
            tokio::fs::create_dir_all(&group_stage).await?;
            let local_out = group_stage.join(create_combined_filename(group));

            let start_dt = group.files.first().and_then(|f| parse_filename(&f.path).timestamp);
            let end_dt = group.files.last().and_then(|f| parse_filename(&f.path).timestamp);

            let failure_members = group.files.iter().map(|item| item.path.clone()).collect::<Vec<_>>();
            let mut fallback_attempted = false;
            let direct_input_allowed = group.files.len() == 1
                && runner.available()
                && !matches!(source.encoder.as_str(), "nvenc-h265" | "x265-veryslow");
            let (result, bytes_in) = if direct_input_allowed {
                let item = &group.files[0];
                let file_size = file_size_map.get(&item.path).copied().unwrap_or(0);
                let direct_result = try_direct_input(&src_vfs, item, file_size, &local_out, cancel.clone()).await;
                match direct_result {
                    Ok(r) => (Ok(r), file_size as i64),
                    Err(ref e) if e.to_string().contains("cancelled") => {
                        (Err(anyhow::anyhow!("cancelled")), file_size as i64)
                    }
                    Err(e) => {
                        tracing::warn!("direct input failed ({}), falling back to staging+concat", e);
                        let inputs = stage_group_files(&src_vfs, &group.files, &group_stage).await?;
                        let bsz = sum_file_sizes(&inputs).unwrap_or(0);
                        let fallback = if runner.available() {
                            fallback_attempted = true;
                            run_with_encoder(&runner, &source, &group.files, &inputs, &local_out, cancel.clone())
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
                let r = if runner.available() {
                    run_with_encoder(&runner, &source, &group.files, &inputs, &local_out, cancel.clone()).await
                } else {
                    Err(anyhow::anyhow!("ffmpeg binary is unavailable"))
                };
                (r, bsz)
            };
            total_bytes_in += bytes_in;

            let expected_duration = expected_duration_secs(&group.files);
            let result = match result {
                Ok((mut tracker, fallback_used)) => {
                    let mode = warn_log_mode_for_decision(decision);
                    tracker.set_mode(mode.to_string(), fallback_used);
                    let tolerance_factor = post_validate_tolerance_factor(mode, fallback_used);
                    match post_validate_output(&runner, &local_out, expected_duration, tolerance_factor).await {
                        Ok(()) => Ok((tracker, fallback_used)),
                        Err(reason) => {
                            let _ = tokio::fs::remove_file(&local_out).await;
                            Err(FfmpegRunError {
                                message: reason,
                                tracker,
                            }
                            .into())
                        }
                    }
                }
                Err(error) => Err(error),
            };

            match result {
                Ok((mut tracker, fallback_used)) => {
                    for (category, message) in &group_warning_examples {
                        tracker.add_warning(category.clone(), message.clone());
                    }
                    if fallback_used {
                        tracker.add_warning("fallback_used", "ffmpeg fallback path used");
                    }
                    let warning_summaries = tracker.warning_summaries();
                    for warning in &warning_summaries {
                        WarningsRepo::add(
                            &self.db,
                            group_model.id,
                            warning.category.clone(),
                            i32::try_from(warning.count).unwrap_or(i32::MAX),
                            warning.first_example.clone(),
                        )
                        .await?;
                    }
                    upload_local_file(&dst_vfs, &local_out, &out_vfs).await?;
                    let warning_level = if fallback_used || !warning_summaries.is_empty() || had_broken_warnings {
                        "warn"
                    } else {
                        "clean"
                    };
                    let status = if fallback_used { "downgraded" } else { "ok" };
                    upload_warn_log(&dst_vfs, &out_vfs, &tracker, status, None).await?;
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
                    let reason = error.to_string();
                    let ffmpeg_error = error.downcast_ref::<FfmpegRunError>();
                    let internal_fallback_attempted =
                        ffmpeg_error.map(|err| err.tracker.was_fallback()).unwrap_or(false);
                    let mut tracker = ffmpeg_error.map(|err| err.tracker.clone()).unwrap_or_default();
                    tracker.set_mode(
                        warn_log_mode_for_decision(decision).to_string(),
                        fallback_attempted || internal_fallback_attempted,
                    );
                    for (category, message) in &group_warning_examples {
                        tracker.add_warning(category.clone(), message.clone());
                    }
                    for warning in tracker.warning_summaries() {
                        WarningsRepo::add(
                            &self.db,
                            group_model.id,
                            warning.category,
                            i32::try_from(warning.count).unwrap_or(i32::MAX),
                            warning.first_example,
                        )
                        .await?;
                    }
                    upload_warn_log(&dst_vfs, &out_vfs, &tracker, "failed", Some(&reason)).await?;
                    upload_failure_log(&dst_vfs, &out_vfs, &failure_members, &reason).await?;
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
                            abort_reason: Some(reason),
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

        if !broken_paths.is_empty() && groups.is_empty() {
            let group_model = MergeRunsRepo::create_group(
                &self.db,
                run.id,
                "broken-files".to_string(),
                "".to_string(),
                "none".to_string(),
            )
            .await?;

            for broken_path in &broken_paths {
                WarningsRepo::add(
                    &self.db,
                    group_model.id,
                    "broken_file_skipped".to_string(),
                    1,
                    Some(format!("broken file skipped: {}", broken_path)),
                )
                .await?;
            }

            MergeRunsRepo::update_group(
                &self.db,
                group_model.id,
                GroupUpdate {
                    start_dt: None,
                    end_dt: None,
                    status: "skipped".to_string(),
                    warning_level: "warn".to_string(),
                    duration_secs: None,
                    bytes_in: None,
                    bytes_out: None,
                    abort_reason: Some("broken file skipped".to_string()),
                },
            )
            .await?;
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

async fn upload_warn_log(
    dst_vfs: &Vfs,
    out: &Path,
    tracker: &WarningTracker,
    status: &str,
    reason: Option<&str>,
) -> anyhow::Result<()> {
    let log_path = warn_log_path(out);
    ensure_vfs_parent(dst_vfs, &log_path).await?;
    dst_vfs
        .put(&log_path, tracker.format_warn_log(out, status, reason).into_bytes())
        .await?;
    Ok(())
}

fn warn_log_path(out: &Path) -> PathBuf {
    PathBuf::from(format!("{}.warn.log", out.to_string_lossy()))
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
    let log_path = PathBuf::from(format!("{}.failure.log", out.to_string_lossy()));
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

fn expected_duration_secs(files: &[crate::core::grouping::VideoItem]) -> Option<f64> {
    let mut total_ms = 0_i64;
    let mut has_any = false;
    for item in files {
        if let Some(duration_ms) = item.duration_ms.filter(|value| *value > 0) {
            total_ms += duration_ms;
            has_any = true;
        }
    }
    has_any.then_some(total_ms as f64 / 1000.0)
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

fn warn_log_mode_for_decision(decision: &str) -> &'static str {
    match decision {
        "encode_nvenc" | "encode_x265" => "compress",
        _ => "concat_copy",
    }
}

fn post_validate_tolerance_factor(mode: &str, fallback_used: bool) -> f64 {
    if mode == "concat_copy" || fallback_used {
        0.10
    } else {
        0.05
    }
}

async fn post_validate_output(
    runner: &FfmpegRunner,
    output: &Path,
    expected_duration: Option<f64>,
    tolerance_factor: f64,
) -> Result<(), String> {
    let metadata = tokio::fs::metadata(output).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "post-validate: output missing".to_string()
        } else {
            format!("post-validate: stat failed: {error}")
        }
    })?;
    if metadata.len() == 0 {
        return Err("post-validate: output empty".to_string());
    }
    let measured_duration = runner
        .probe_output_duration(output)
        .await
        .map_err(|error| format!("post-validate: ffprobe failed: {error}"))?;
    if let Some(expected_duration) = expected_duration {
        let tolerance = (expected_duration * tolerance_factor).max(1.0);
        let delta = (measured_duration - expected_duration).abs();
        if delta > tolerance {
            return Err(format!(
                "post-validate: duration mismatch (expected {expected_duration:.1}, got {measured_duration:.1}, tolerance ±{tolerance:.1})"
            ));
        }
    }
    Ok(())
}

const H264_TARGET_RATIO: f64 = 0.65;

fn profile_for_source(source: &sources::Model, group_files: &[crate::core::grouping::VideoItem]) -> EncodeProfile {
    match source.encoder.as_str() {
        "nvenc-h265" => {
            let base =
                nvenc_profile_for_camera(group_files.first().map(|item| item.camera.as_str()).unwrap_or_default());
            h264_group_input_bps(group_files)
                .map_or(base.clone(), |input_bps| derive_h264_nvenc_profile(&base, input_bps))
        }
        "x265-veryslow" => EncodeProfile {
            crf: x265_crf_from_encoder_params(&source.encoder_params),
            ..EncodeProfile::default()
        },
        _ => EncodeProfile::default(),
    }
}

fn nvenc_profile_for_camera(camera: &str) -> EncodeProfile {
    let mut profile = EncodeProfile::default();
    match camera {
        "AA" => {
            profile.bitrate = "8M".to_string();
            profile.maxrate = "12M".to_string();
            profile.bufsize = "16M".to_string();
        }
        "AB" | "AC" => {
            profile.bitrate = "3M".to_string();
            profile.maxrate = "5M".to_string();
            profile.bufsize = "8M".to_string();
        }
        _ => {}
    }
    profile
}

fn h264_group_input_bps(group_files: &[crate::core::grouping::VideoItem]) -> Option<i64> {
    if group_files.is_empty()
        || group_files.iter().any(|item| {
            !item
                .codec
                .as_deref()
                .is_some_and(|codec| codec.eq_ignore_ascii_case("h264"))
        })
    {
        return None;
    }

    let mut total_size_bytes = 0_i128;
    let mut total_duration_ms = 0_i128;
    for item in group_files {
        let Some(size_bytes) = item.size_bytes.filter(|value| *value > 0) else {
            continue;
        };
        let Some(duration_ms) = item.duration_ms.filter(|value| *value > 0) else {
            continue;
        };
        total_size_bytes = total_size_bytes.checked_add(i128::from(size_bytes))?;
        total_duration_ms = total_duration_ms.checked_add(i128::from(duration_ms))?;
    }

    if total_duration_ms <= 0 {
        return None;
    }

    let bps = total_size_bytes.checked_mul(8)?.checked_mul(1_000)? / total_duration_ms;
    i64::try_from(bps).ok()
}

fn derive_h264_nvenc_profile(default_profile: &EncodeProfile, input_bps: i64) -> EncodeProfile {
    let target_from_input = (input_bps as f64 * H264_TARGET_RATIO) as i64;
    let default_target = parse_bps(&default_profile.bitrate);
    let target = default_target.map_or(target_from_input, |default_bps| default_bps.min(target_from_input));
    let default_maxrate = parse_bps(&default_profile.maxrate);
    let maxrate = default_maxrate.map_or(input_bps, |default_bps| default_bps.min(input_bps));
    let bufsize = target.saturating_mul(2).max(maxrate);

    EncodeProfile {
        bitrate: target.to_string(),
        maxrate: maxrate.to_string(),
        bufsize: bufsize.to_string(),
        ..default_profile.clone()
    }
}

fn parse_bps(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (number, multiplier) = match trimmed.as_bytes().last().copied() {
        Some(b'm' | b'M') => (&trimmed[..trimmed.len() - 1], 1_000_000_i64),
        Some(b'k' | b'K') => (&trimmed[..trimmed.len() - 1], 1_000_i64),
        _ => (trimmed, 1_i64),
    };
    number.trim().parse::<i64>().ok()?.checked_mul(multiplier)
}

fn x265_crf_from_encoder_params(params: &serde_json::Value) -> u8 {
    params
        .get("x265_crf")
        .and_then(|value| match value {
            serde_json::Value::Number(number) => number.as_u64(),
            serde_json::Value::String(text) => text.trim().parse::<u64>().ok(),
            _ => None,
        })
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(X265_DEFAULT_CRF)
}

async fn run_with_encoder(
    runner: &FfmpegRunner,
    source: &sources::Model,
    group_files: &[crate::core::grouping::VideoItem],
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
                    .map(|tracker| (tracker, true))
                    .map_err(mark_internal_fallback_attempted);
            };
            let profile = profile_for_source(source, group_files);
            let input_flags = if source.encoder == "nvenc-h265" {
                nvenc_concat_input_flags()
            } else {
                default_concat_input_flags()
            };
            match runner
                .concat_encode(
                    inputs,
                    out,
                    &encoder.encode_args(&profile),
                    &input_flags,
                    cancel.clone(),
                )
                .await
            {
                Ok(tracker) => Ok((tracker, false)),
                Err(error) if !error.to_string().contains("cancelled") => {
                    let tracker = runner
                        .concat_copy(inputs, out, cancel)
                        .await
                        .map_err(mark_internal_fallback_attempted)?;
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

fn mark_internal_fallback_attempted(error: anyhow::Error) -> anyhow::Error {
    match error.downcast::<FfmpegRunError>() {
        Ok(mut error) => {
            error.tracker.mark_fallback_attempted();
            error.into()
        }
        Err(error) => error,
    }
}

async fn try_direct_input(
    src_vfs: &Arc<Vfs>,
    item: &crate::core::grouping::VideoItem,
    file_size: u64,
    local_out: &Path,
    cancel: CancellationToken,
) -> anyhow::Result<(WarningTracker, bool)> {
    let read_at = src_vfs.to_read_at(&item.path).await;
    let filename_hint = item.path.file_name().map(|n| n.to_string_lossy().into_owned());
    let direct_input = FfmpegDirectInput::from_read_at(read_at, file_size, filename_hint, Some(32 * 1024 * 1024));

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
            video_codec: "copy".to_string(),
            audio_codec: "copy".to_string(),
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
        Ok(Ok(())) => {
            let mut tracker = WarningTracker::default();
            tracker.set_command("direct_input", &[]);
            Ok((tracker, false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_h264_nvenc_profile_from_input_bitrate() {
        let profile = derive_h264_nvenc_profile(&EncodeProfile::default(), 10_000_000);

        assert_eq!(profile.cq, 32);
        assert_eq!(profile.preset, "p7");
        assert_eq!(profile.bitrate, "5000000");
        assert_eq!(profile.maxrate, "8000000");
        assert_eq!(profile.bufsize, "10000000");
    }

    #[test]
    fn derives_camera_profile_with_decimal_bps_strings() {
        let profile = derive_h264_nvenc_profile(&nvenc_profile_for_camera("AC"), 4_000_000);

        assert_eq!(profile.bitrate, "2600000");
        assert_eq!(profile.maxrate, "4000000");
        assert_eq!(profile.bufsize, "5200000");
    }

    #[test]
    fn h264_group_input_bps_uses_homogeneous_weighted_average() {
        let group_files = vec![
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000000_a.mp4"),
                Some(1_000),
                Some("h264".to_string()),
                Some(8_000_000),
                Some(1_000_000),
            ),
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000001_b.mp4"),
                Some(3_000),
                Some("H264".to_string()),
                Some(24_000_000),
                Some(9_000_000),
            ),
        ];

        assert_eq!(h264_group_input_bps(&group_files), Some(20_000_000));
    }

    #[test]
    fn h264_group_input_bps_returns_none_for_mixed_codecs() {
        let group_files = vec![
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000000_a.mp4"),
                Some(1_000),
                Some("h264".to_string()),
                Some(8_000_000),
                Some(1_000_000),
            ),
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000001_b.mp4"),
                Some(1_000),
                Some("hevc".to_string()),
                Some(8_000_000),
                Some(1_000_000),
            ),
        ];

        assert_eq!(h264_group_input_bps(&group_files), None);
    }

    #[test]
    fn parses_x265_crf_override_number_or_string() {
        assert_eq!(x265_crf_from_encoder_params(&serde_json::json!({"x265_crf": 23})), 23);
        assert_eq!(x265_crf_from_encoder_params(&serde_json::json!({"x265_crf": "24"})), 24);
        assert_eq!(
            x265_crf_from_encoder_params(&serde_json::json!({"x265_crf": "bad"})),
            X265_DEFAULT_CRF
        );
    }
}
