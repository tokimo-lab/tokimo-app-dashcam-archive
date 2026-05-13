use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use chrono::Datelike;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use walkdir::WalkDir;

use crate::{
    core::{
        duration::DurationResolver,
        encoder::{EncodeProfile, EncoderRegistry},
        ffmpeg::{CancellationToken, FfmpegRunner, WarningTracker},
        grouping::{create_combined_filename, group_by_time, item_from_path},
        naming::{is_video_file, parse_filename},
        report::{RunReport, write_report},
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

pub struct Pipeline {
    db: DatabaseConnection,
    paths: Arc<tokio::sync::RwLock<crate::core::ffmpeg::FfmpegPaths>>,
    progress: tokio::sync::broadcast::Sender<ProgressEvent>,
}

impl Pipeline {
    pub fn new(
        db: DatabaseConnection,
        paths: Arc<tokio::sync::RwLock<crate::core::ffmpeg::FfmpegPaths>>,
        progress: tokio::sync::broadcast::Sender<ProgressEvent>,
    ) -> Self {
        Self { db, paths, progress }
    }

    pub async fn run(
        &self,
        run: merge_runs::Model,
        source: sources::Model,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        MergeRunsRepo::set_status(&self.db, run.id, "running").await?;
        let input = PathBuf::from(&source.src_path);
        let output = PathBuf::from(&source.dst_path);
        tokio::fs::create_dir_all(&output).await?;
        self.emit(run.id, "scan", 0, 0, 0, None, 0.0);

        let files = scan_files(&input)?;
        let total_files = files.len().max(1);
        let resolver = DurationResolver::new(self.db.clone(), Arc::clone(&self.paths), 4);
        let mut videos = Vec::new();
        for (idx, path) in files.iter().enumerate() {
            if cancel.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            if is_video_file(path) {
                let probe = resolver.resolve(source.id, path).await?;
                videos.push(item_from_path(path.clone(), probe.duration_secs.map(secs_to_ms)));
            } else {
                copy_non_video(&input, &output, path).await?;
            }
            let percent = (idx + 1) as f64 / total_files as f64 * 20.0;
            self.emit(
                run.id,
                "scan",
                0,
                0,
                0,
                Some(path.to_string_lossy().to_string()),
                percent,
            );
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
            let out_dir = output_dir_for_group(
                &output,
                &input,
                group.files.first().map(|item| item.path.as_path()),
                &source.monthly_subdirs,
            )
            .await?;
            let out = out_dir.join(create_combined_filename(group));

            let decision = decision_for_encoder(&source.encoder, group.files.len());

            let group_model = MergeRunsRepo::create_group(
                &self.db,
                run.id,
                group.camera.clone(),
                out.to_string_lossy().to_string(),
                decision.to_string(),
            )
            .await?;

            let inputs: Vec<PathBuf> = group.files.iter().map(|item| item.path.clone()).collect();
            let bytes_in = sum_file_sizes(&inputs).unwrap_or(0);
            total_bytes_in += bytes_in;

            let start_dt = group.files.first().and_then(|f| parse_filename(&f.path).timestamp);
            let end_dt = group.files.last().and_then(|f| parse_filename(&f.path).timestamp);

            let result = if runner.available() {
                run_with_encoder(&runner, &source, &inputs, &out, cancel.clone()).await
            } else {
                Err(anyhow::anyhow!("ffmpeg binary is unavailable"))
            };

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
                    let bytes_out = file_size(&out).unwrap_or(0);
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
                    inherit_last_mtime(&inputs, &out).await;
                }
                Err(error) => {
                    failed_count += 1;
                    write_failure_log(&out, &inputs, &error.to_string()).await?;
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
                Some(out.to_string_lossy().to_string()),
                percent,
            );
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

        write_report(
            &output.join("_transcode_warnings.txt"),
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

fn scan_files(input: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(input).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.metadata()?.len() > 0 {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

async fn copy_non_video(root: &Path, output: &Path, path: &Path) -> anyhow::Result<()> {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let dest = output.join(rel);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
        return Ok(());
    }
    tokio::fs::copy(path, dest).await?;
    Ok(())
}

async fn output_dir_for_group(
    base: &Path,
    root: &Path,
    first: Option<&Path>,
    monthly: &str,
) -> anyhow::Result<PathBuf> {
    let Some(first) = first else {
        return Ok(base.to_path_buf());
    };
    let rel_dir = first
        .strip_prefix(root)
        .ok()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(""));
    let parsed = parse_filename(first);
    let use_monthly = monthly == "on" || (monthly == "auto" && parsed.camera.starts_with("XIAOMI_"));
    let dir = if use_monthly {
        parsed.timestamp.map_or_else(
            || base.join(rel_dir),
            |ts| base.join(format!("{:04}{:02}", ts.year(), ts.month())),
        )
    } else {
        base.join(rel_dir)
    };
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
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

async fn inherit_last_mtime(inputs: &[PathBuf], out: &Path) {
    let _ = (inputs, out); // mtime preservation is best-effort and platform dependent in stable std/tokio.
}

async fn write_failure_log(out: &Path, inputs: &[PathBuf], reason: &str) -> anyhow::Result<()> {
    let mut text = format!(
        "merge failed: {}\nreason: {reason}\nmembers: {}\n",
        out.display(),
        inputs.len()
    );
    for input in inputs {
        text.push_str(&format!("  {}\n", input.display()));
    }
    tokio::fs::write(out.with_extension("mp4.failure.log"), text).await?;
    Ok(())
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
