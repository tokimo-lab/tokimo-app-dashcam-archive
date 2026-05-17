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
            CancellationToken, FfmpegRunError, FfmpegRunner, NegativeCompressionOptions, WarningTracker,
            default_concat_input_flags, nvenc_concat_input_flags,
        },
        grouping::{ScanEntry, create_combined_filename, group_by_time, item_from_path, item_from_probe},
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

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DryRunGroup {
    pub output_name: String,
    pub input_files: Vec<String>,
    pub encoder: String,
    #[ts(type = "number")]
    pub estimated_duration_ms: u64,
    #[ts(type = "number")]
    pub estimated_size_bytes: u64,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DryRunPlan {
    pub groups: Vec<DryRunGroup>,
}

pub async fn dry_run_plan(db: &DatabaseConnection, source: sources::Model) -> anyhow::Result<DryRunPlan> {
    if !source.allow_combined_input {
        let src_path_lower = source.src_path.trim_end_matches(['/', '\\']).to_lowercase();
        if src_path_lower.contains("_combined") {
            anyhow::bail!("_Combined input rejected (set allow_combined_input=true to override)");
        }
    }
    let src_vfs = vfs_source::build_vfs(db, source.src_source_id, &source.src_source_type).await?;
    let input = PathBuf::from(&source.src_path);
    let (files, _) = scan_vfs(&src_vfs, &input).await?;
    let file_size_map: std::collections::HashMap<PathBuf, u64> =
        files.iter().map(|f| (f.path.clone(), f.info.size)).collect();
    let mut videos = Vec::new();
    for file in &files {
        if is_video_file(&file.path) {
            videos.push(ScanEntry::Video(item_from_path(file.path.clone(), None)));
        }
        // Non-video files are skipped in dry-run (no copy performed)
    }
    let gap = if source.max_gap_seconds > 0 {
        Some(Duration::from_secs(u64::try_from(source.max_gap_seconds).unwrap_or(60)))
    } else {
        None
    };
    let groups = group_by_time(videos, gap);
    let dry_groups = groups
        .iter()
        .map(|group| {
            let output_name = create_combined_filename(group);
            let input_files = group
                .files
                .iter()
                .map(|item| item.path.to_string_lossy().to_string())
                .collect();
            let estimated_duration_ms = match (group.start, group.end) {
                (Some(start), Some(end)) => {
                    let ms = end.signed_duration_since(start).num_milliseconds();
                    if ms > 0 { ms as u64 } else { 0 }
                }
                _ => 0,
            };
            let estimated_size_bytes = group
                .files
                .iter()
                .map(|item| *file_size_map.get(&item.path).unwrap_or(&0))
                .sum::<u64>();
            DryRunGroup {
                output_name,
                input_files,
                encoder: source.encoder.clone(),
                estimated_duration_ms,
                estimated_size_bytes,
            }
        })
        .collect();
    Ok(DryRunPlan { groups: dry_groups })
}

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

        let (files, zero_byte_paths) = scan_vfs(&src_vfs, &input).await?;
        let total_files = files.len().max(1);
        let file_size_map: std::collections::HashMap<PathBuf, u64> =
            files.iter().map(|f| (f.path.clone(), f.info.size)).collect();
        let resolver = source
            .hybrid_health_check
            .then(|| DurationResolver::new(self.db.clone(), 4));
        let mut videos = Vec::new();
        for (idx, file) in files.iter().enumerate() {
            if cancel.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            if is_video_file(&file.path) {
                if let Some(resolver) = &resolver {
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
                    videos.push(ScanEntry::Video(item_from_path(file.path.clone(), None)));
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
                    "message": format!("扫描: {}", file.path.display()),
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

        let mut zero_byte_warnings = zero_byte_paths;

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

            // Preflight bitrate gate: if preflight_bitrate_ref > 0 and input bitrate is low, skip encoding
            // Only applies when input codec already matches target encoder output (H265/HEVC)
            let should_preflight_copy = source.hybrid_health_check
                && should_preflight_copy_group(&group.files, &source.encoder, source.preflight_bitrate_ref);

            let decision = if should_preflight_copy {
                "preflight_copy"
            } else {
                decision_for_encoder(&source.encoder, group.files.len())
            };

            let group_model = MergeRunsRepo::create_group(
                &self.db,
                run.id,
                group.camera.clone(),
                out_vfs.to_string_lossy().to_string(),
                decision.to_string(),
            )
            .await?;

            let had_broken_warnings = !broken_paths.is_empty();
            let had_zero_byte_warnings = !zero_byte_warnings.is_empty();
            let mut group_warning_examples = broken_paths
                .drain(..)
                .map(|broken_path| {
                    (
                        "broken_file_skipped".to_string(),
                        format!("broken file skipped: {broken_path}"),
                    )
                })
                .collect::<Vec<_>>();
            group_warning_examples.extend(
                zero_byte_warnings
                    .drain(..)
                    .map(|msg| ("zero_byte_file_skipped".to_string(), msg)),
            );

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
                            run_with_encoder(
                                &runner,
                                &source,
                                &group.files,
                                &inputs,
                                &local_out,
                                bsz,
                                decision,
                                cancel.clone(),
                            )
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
                    run_with_encoder(
                        &runner,
                        &source,
                        &group.files,
                        &inputs,
                        &local_out,
                        bsz,
                        decision,
                        cancel.clone(),
                    )
                    .await
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

                    if let Err(e) = inherit_file_times(&src_vfs, &group.files, &local_out).await {
                        tracing::warn!(error = ?e, "failed to inherit source file times");
                    }

                    upload_local_file(&dst_vfs, &local_out, &out_vfs).await?;

                    if let Some(dst_real_path) = dst_vfs.resolve_real_path(&out_vfs).await
                        && let Err(e) = inherit_file_times(&src_vfs, &group.files, Path::new(&dst_real_path)).await
                    {
                        tracing::warn!(error = ?e, "failed to set destination file times");
                    }

                    let warning_level = if fallback_used
                        || !warning_summaries.is_empty()
                        || had_broken_warnings
                        || had_zero_byte_warnings
                    {
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
                    "message": format!("归并: {}", out_vfs.display()),
                }),
            )
            .await;
        }

        if (!broken_paths.is_empty() || !zero_byte_warnings.is_empty()) && groups.is_empty() {
            let group_model = MergeRunsRepo::create_group(
                &self.db,
                run.id,
                "skipped-files".to_string(),
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

            for zero_byte_msg in &zero_byte_warnings {
                WarningsRepo::add(
                    &self.db,
                    group_model.id,
                    "zero_byte_file_skipped".to_string(),
                    1,
                    Some(zero_byte_msg.clone()),
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

async fn inherit_file_times(
    src_vfs: &Vfs,
    group_files: &[crate::core::grouping::VideoItem],
    output_path: &Path,
) -> anyhow::Result<()> {
    let last_file = group_files.last().ok_or_else(|| anyhow::anyhow!("no files in group"))?;

    // Try to get real filesystem times if source is local
    if let Some(real_src) = src_vfs.resolve_real_path(last_file.path.as_path()).await {
        let metadata = tokio::fs::metadata(&real_src).await?;
        let modified = metadata.modified()?;
        let accessed = metadata.accessed().unwrap_or(modified);

        let accessed_ft = filetime::FileTime::from_system_time(accessed);
        let modified_ft = filetime::FileTime::from_system_time(modified);
        filetime::set_file_times(output_path, accessed_ft, modified_ft)?;
    } else {
        // Fall back to VFS stat (only has modified)
        let info = src_vfs.stat(last_file.path.as_path()).await?;
        if let Some(modified_dt) = info.modified {
            let modified = std::time::SystemTime::from(modified_dt);

            let modified_ft = filetime::FileTime::from_system_time(modified);
            filetime::set_file_times(output_path, modified_ft, modified_ft)?;
        }
    }

    Ok(())
}

async fn scan_vfs(vfs: &Vfs, root: &Path) -> anyhow::Result<(Vec<VfsFile>, Vec<String>)> {
    let mut files = Vec::new();
    let mut zero_byte_paths = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for info in vfs.list(&dir).await? {
            let path = PathBuf::from(&info.path);
            if info.is_dir {
                stack.push(path);
            } else if info.size == 0 {
                let warning_msg = format!("zero-byte file skipped: {}", path.display());
                tracing::warn!("{}", warning_msg);
                zero_byte_paths.push(warning_msg);
            } else {
                files.push(VfsFile { info, path });
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, zero_byte_paths))
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
        "preflight_copy" => "concat_copy",
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

fn is_h265_preflight_eligible(group_files: &[crate::core::grouping::VideoItem], encoder: &str) -> bool {
    match encoder {
        "nvenc-h265" | "x265-veryslow" => {
            // For H265 encoders, only allow preflight copy if input is already H265/HEVC
            !group_files.is_empty()
                && group_files.iter().all(|item| {
                    item.codec
                        .as_deref()
                        .is_some_and(|c| c.eq_ignore_ascii_case("hevc") || c.eq_ignore_ascii_case("h265"))
                })
        }
        _ => {
            // For non-H265 encoders (copy-only, auto, current, h264, etc.), no preflight copy
            false
        }
    }
}

fn group_weighted_input_bps(group_files: &[crate::core::grouping::VideoItem]) -> Option<i64> {
    if group_files.is_empty() {
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

fn group_format_bps_average(group_files: &[crate::core::grouping::VideoItem]) -> Option<i64> {
    let mut total_bps = 0_i128;
    let mut count = 0_i128;
    for format_bps in group_files
        .iter()
        .filter_map(|item| item.format_bps.filter(|value| *value > 0))
    {
        total_bps = total_bps.checked_add(i128::from(format_bps))?;
        count += 1;
    }

    if count == 0 {
        return None;
    }

    i64::try_from(total_bps / count).ok()
}

fn should_preflight_copy_group(
    group_files: &[crate::core::grouping::VideoItem],
    encoder: &str,
    preflight_bitrate_ref: i32,
) -> bool {
    if preflight_bitrate_ref <= 0 {
        return false;
    }

    if !is_h265_preflight_eligible(group_files, encoder) {
        return false;
    }

    let Some(avg_bps) = group_weighted_input_bps(group_files).or_else(|| group_format_bps_average(group_files)) else {
        return false;
    };

    let threshold = (preflight_bitrate_ref as i64 * 11) / 10; // 1.1x
    avg_bps < threshold
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
    input_size: i64,
    decision: &str,
    cancel: CancellationToken,
) -> anyhow::Result<(WarningTracker, bool)> {
    let attempts = encoder_attempts(source.encoder.as_str(), decision);
    let expected_duration = expected_duration_secs(group_files);
    let Some(ffmpeg) = runner.paths().ffmpeg.as_deref() else {
        anyhow::bail!("ffmpeg binary is unavailable");
    };
    let registry =
        EncoderRegistry::new_with_builtins(Path::new(ffmpeg), runner.paths().library_dir.as_deref().map(Path::new));
    let mut retry_warnings = Vec::<(String, String)>::new();
    let mut failed_attempt_warnings = WarningTracker::default();

    for (attempt_index, attempt) in attempts.iter().enumerate() {
        let is_last = attempt_index + 1 == attempts.len();
        let context = EncoderAttemptContext {
            runner,
            registry: &registry,
            source,
            group_files,
            inputs,
            out,
            expected_duration,
            input_size,
        };
        let result = run_encoder_attempt(context, attempt, cancel.clone()).await;
        match result {
            Ok(mut tracker) => {
                if attempt_index > 0 {
                    tracker.mark_fallback_attempted();
                }
                tracker.extend_warnings_from(&failed_attempt_warnings);
                for (category, message) in retry_warnings {
                    tracker.add_warning(category, message);
                }
                return Ok((tracker, attempt_index > 0));
            }
            Err(error) if error.to_string().contains("cancelled") => return Err(error),
            Err(error) if !is_last => {
                if let Some(ffmpeg_error) = error.downcast_ref::<FfmpegRunError>() {
                    failed_attempt_warnings.extend_warnings_from(&ffmpeg_error.tracker);
                }
                let reason = error.to_string();
                let next_attempt = attempts[attempt_index + 1];
                tracing::warn!(%reason, attempt, next_attempt, "dashcam-archive: encoder attempt failed; retrying");
                retry_warnings.push((
                    "encoder_retry".to_string(),
                    format!("{attempt} failed; retrying with {next_attempt}: {reason}"),
                ));
                let _ = tokio::fs::remove_file(out).await;
            }
            Err(error) => {
                let reason = error.to_string();
                let mut tracker = error
                    .downcast_ref::<FfmpegRunError>()
                    .map(|err| err.tracker.clone())
                    .unwrap_or_default();
                tracker.extend_warnings_from(&failed_attempt_warnings);
                for (category, message) in retry_warnings {
                    tracker.add_warning(category, message);
                }
                if attempt_index > 0 {
                    tracker.mark_fallback_attempted();
                }
                return Err(FfmpegRunError {
                    message: reason,
                    tracker,
                }
                .into());
            }
        }
    }

    anyhow::bail!("encoder retry chain is empty")
}

struct EncoderAttemptContext<'a> {
    runner: &'a FfmpegRunner,
    registry: &'a EncoderRegistry,
    source: &'a sources::Model,
    group_files: &'a [crate::core::grouping::VideoItem],
    inputs: &'a [PathBuf],
    out: &'a Path,
    expected_duration: Option<f64>,
    input_size: i64,
}

async fn run_encoder_attempt(
    context: EncoderAttemptContext<'_>,
    attempt: &str,
    cancel: CancellationToken,
) -> anyhow::Result<WarningTracker> {
    let mut tracker = if attempt == "copy" {
        context.runner.concat_copy(context.inputs, context.out, cancel).await?
    } else {
        let Some(encoder) = context.registry.get(attempt) else {
            anyhow::bail!("encoder {attempt} is unavailable");
        };
        let profile = profile_for_attempt(attempt, context.source, context.group_files);
        let input_flags = if attempt == "nvenc-h265" {
            nvenc_concat_input_flags()
        } else {
            default_concat_input_flags()
        };
        context
            .runner
            .concat_encode(
                context.inputs,
                context.out,
                &encoder.encode_args(&profile),
                &input_flags,
                cancel,
                negative_compression_options(context.expected_duration, context.input_size),
            )
            .await?
    };

    let fallback_used = attempt == "copy";
    let tolerance_factor = post_validate_tolerance_factor(warn_log_mode_for_attempt(attempt), fallback_used);
    if let Err(reason) =
        post_validate_output(context.runner, context.out, context.expected_duration, tolerance_factor).await
    {
        let _ = tokio::fs::remove_file(context.out).await;
        tracker.add_warning("encoder_retry", reason.clone());
        return Err(FfmpegRunError {
            message: reason,
            tracker,
        }
        .into());
    }

    if attempt != "copy" && output_exceeds_input(context.out, context.input_size).await? {
        let output_size = file_size(context.out).unwrap_or(0);
        let reason = format!(
            "post negative compression: output {output_size} > input {}",
            context.input_size
        );
        let _ = tokio::fs::remove_file(context.out).await;
        tracker.add_warning("negative_compression", reason.clone());
        return Err(FfmpegRunError {
            message: reason,
            tracker,
        }
        .into());
    }

    Ok(tracker)
}

fn encoder_attempts(encoder: &str, decision: &str) -> Vec<&'static str> {
    if decision == "preflight_copy" {
        return vec!["copy"];
    }
    match encoder {
        "nvenc-h265" => vec!["nvenc-h265", "x265-veryslow", "copy"],
        "x265-veryslow" => vec!["x265-veryslow", "copy"],
        _ => vec!["copy"],
    }
}

fn profile_for_attempt(
    attempt: &str,
    source: &sources::Model,
    group_files: &[crate::core::grouping::VideoItem],
) -> EncodeProfile {
    match attempt {
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

fn warn_log_mode_for_attempt(attempt: &str) -> &'static str {
    match attempt {
        "nvenc-h265" | "x265-veryslow" => "compress",
        _ => "concat_copy",
    }
}

fn negative_compression_options(expected_duration: Option<f64>, input_size: i64) -> Option<NegativeCompressionOptions> {
    expected_duration
        .filter(|duration| *duration > 0.0 && input_size > 0)
        .map(|duration| NegativeCompressionOptions {
            expected_duration_secs: duration,
            input_size_bytes: input_size,
        })
}

async fn output_exceeds_input(out: &Path, input_size: i64) -> anyhow::Result<bool> {
    if input_size <= 0 {
        return Ok(false);
    }
    let metadata = tokio::fs::metadata(out).await?;
    let output_size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
    Ok(output_size > input_size)
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
    fn preflight_h265_eligible_rejects_h264_for_h265_encoders() {
        let group_files = vec![crate::core::grouping::item_from_probe(
            PathBuf::from("/src/AA/20250101000000_a.mp4"),
            Some(1_000),
            Some("h264".to_string()),
            Some(8_000_000),
            Some(1_000_000),
        )];

        assert!(!is_h265_preflight_eligible(&group_files, "nvenc-h265"));
        assert!(!is_h265_preflight_eligible(&group_files, "x265-veryslow"));
    }

    #[test]
    fn preflight_h265_eligible_accepts_hevc_for_h265_encoders() {
        let group_files = vec![
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000000_a.mp4"),
                Some(1_000),
                Some("hevc".to_string()),
                Some(8_000_000),
                Some(1_000_000),
            ),
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000001_b.mp4"),
                Some(1_000),
                Some("H265".to_string()),
                Some(8_000_000),
                Some(1_000_000),
            ),
        ];

        assert!(is_h265_preflight_eligible(&group_files, "nvenc-h265"));
        assert!(is_h265_preflight_eligible(&group_files, "x265-veryslow"));
    }

    #[test]
    fn preflight_h265_eligible_rejects_non_h265_encoders() {
        let hevc_group = vec![crate::core::grouping::item_from_probe(
            PathBuf::from("/src/AA/20250101000000_a.mp4"),
            Some(1_000),
            Some("hevc".to_string()),
            Some(3_000_000),
            Some(1_000_000),
        )];

        // Non-H265 encoders are never eligible for preflight copy
        assert!(!is_h265_preflight_eligible(&hevc_group, "copy-only"));
        assert!(!is_h265_preflight_eligible(&hevc_group, "auto"));
        assert!(!is_h265_preflight_eligible(&hevc_group, "current"));
        assert!(!is_h265_preflight_eligible(&hevc_group, "nvenc-h264"));
    }

    #[test]
    fn preflight_copy_disabled_when_ref_is_zero() {
        let hevc_group = vec![crate::core::grouping::item_from_probe(
            PathBuf::from("/src/AA/20250101000000_a.mp4"),
            Some(1_000),
            Some("hevc".to_string()),
            Some(3_000_000), // Low bitrate
            Some(1_000_000),
        )];

        // Even with low bitrate HEVC, disabled ref returns false
        assert!(!should_preflight_copy_group(&hevc_group, "nvenc-h265", 0));
    }

    #[test]
    fn preflight_copy_accepts_low_bitrate_hevc_below_threshold() {
        let hevc_group = vec![crate::core::grouping::item_from_probe(
            PathBuf::from("/src/AA/20250101000000_a.mp4"),
            Some(1_000),
            Some("hevc".to_string()),
            Some(4_000_000), // Below 5_000_000 * 1.1 = 5_500_000
            Some(500_000),
        )];

        assert!(should_preflight_copy_group(&hevc_group, "nvenc-h265", 5_000_000));
    }

    #[test]
    fn preflight_copy_prefers_weighted_bitrate_over_format_bps() {
        let hevc_group = vec![
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000000_a.mp4"),
                Some(1_000),
                Some("hevc".to_string()),
                Some(12_000_000),
                Some(500_000),
            ),
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000001_b.mp4"),
                Some(1_000),
                Some("h265".to_string()),
                Some(12_000_000),
                Some(500_000),
            ),
        ];

        assert!(should_preflight_copy_group(&hevc_group, "nvenc-h265", 5_000_000));
    }

    #[test]
    fn preflight_copy_falls_back_to_positive_format_bps_average() {
        let hevc_group = vec![
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000000_a.mp4"),
                None,
                Some("hevc".to_string()),
                Some(4_000_000),
                None,
            ),
            crate::core::grouping::item_from_probe(
                PathBuf::from("/src/AA/20250101000001_b.mp4"),
                Some(1_000),
                Some("h265".to_string()),
                Some(4_500_000),
                None,
            ),
        ];

        assert!(should_preflight_copy_group(&hevc_group, "nvenc-h265", 5_000_000));
    }

    #[test]
    fn preflight_copy_returns_false_without_weighted_or_format_bps() {
        let hevc_group = vec![crate::core::grouping::item_from_probe(
            PathBuf::from("/src/AA/20250101000000_a.mp4"),
            None,
            Some("hevc".to_string()),
            Some(0),
            Some(500_000),
        )];

        assert!(!should_preflight_copy_group(&hevc_group, "nvenc-h265", 5_000_000));
    }

    #[test]
    fn preflight_copy_rejects_low_bitrate_h264_with_h265_encoder() {
        let h264_group = vec![crate::core::grouping::item_from_probe(
            PathBuf::from("/src/AA/20250101000000_a.mp4"),
            Some(1_000),
            Some("h264".to_string()),
            Some(3_000_000), // Low bitrate but wrong codec
            Some(1_000_000),
        )];

        // Codec incompatibility trumps bitrate
        assert!(!should_preflight_copy_group(&h264_group, "nvenc-h265", 5_000_000));
    }

    #[test]
    fn preflight_copy_rejects_non_h265_encoders_regardless_of_codec_or_bitrate() {
        let hevc_group = vec![crate::core::grouping::item_from_probe(
            PathBuf::from("/src/AA/20250101000000_a.mp4"),
            Some(1_000),
            Some("hevc".to_string()),
            Some(2_000_000), // Very low bitrate, well below threshold
            Some(1_000_000),
        )];

        // Non-H265 encoders never preflight copy, even with HEVC input and low bitrate
        assert!(!should_preflight_copy_group(&hevc_group, "copy-only", 5_000_000));
        assert!(!should_preflight_copy_group(&hevc_group, "auto", 5_000_000));
        assert!(!should_preflight_copy_group(&hevc_group, "current", 5_000_000));
        assert!(!should_preflight_copy_group(&hevc_group, "nvenc-h264", 5_000_000));
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
