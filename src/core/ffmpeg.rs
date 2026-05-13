use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FfmpegPaths {
    pub ffmpeg: Option<String>,
    pub ffprobe: Option<String>,
    pub library_dir: Option<String>,
}

impl FfmpegPaths {
    pub fn is_available(&self) -> bool {
        self.ffmpeg.is_some() && self.ffprobe.is_some()
    }

    pub fn from_env() -> Self {
        Self {
            ffmpeg: std::env::var("TOKIMO_FFMPEG_BIN").ok(),
            ffprobe: std::env::var("TOKIMO_FFPROBE_BIN").ok(),
            library_dir: None,
        }
    }
    pub fn with_env_fallbacks(mut self) -> Self {
        let env = Self::from_env();
        if self.ffmpeg.is_none() {
            self.ffmpeg = env.ffmpeg;
        }
        if self.ffprobe.is_none() {
            self.ffprobe = env.ffprobe;
        }
        if self.library_dir.is_none() {
            self.library_dir = env.library_dir;
        }
        self
    }

    pub fn apply_library_env(&self, command: &mut Command) {
        if let Some(lib) = &self.library_dir {
            #[cfg(unix)]
            {
                let existing = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
                let value = if existing.is_empty() {
                    lib.clone()
                } else {
                    format!("{lib}:{existing}")
                };
                command.env("LD_LIBRARY_PATH", value);
            }
            #[cfg(windows)]
            {
                let existing = std::env::var("PATH").unwrap_or_default();
                let value = if existing.is_empty() {
                    lib.clone()
                } else {
                    format!("{lib};{existing}")
                };
                command.env("PATH", value);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FfmpegWarning {
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct WarningTracker {
    warnings: Vec<FfmpegWarning>,
    last_out_time_ms: Option<i64>,
    last_time: Option<String>,
}

impl WarningTracker {
    pub fn observe(&mut self, line: &str) {
        if let Some(value) = line.strip_prefix("out_time_ms=") {
            self.last_out_time_ms = value.trim().parse::<i64>().ok();
            return;
        }
        if let Some(idx) = line.find("time=") {
            let value = &line[idx + 5..];
            let end = value.find(char::is_whitespace).unwrap_or(value.len());
            self.last_time = Some(value[..end].to_string());
        }
        let patterns = [
            ("corrupt_frame", "corrupt decoded frame|corrupt input|Corrupted frame"),
            ("concealing", "concealing\\s+\\d+|error concealment"),
            (
                "missing_ref",
                "reference picture missing|Missing reference picture|reference frame missing",
            ),
            (
                "missing_picture",
                "missing picture in access unit|No start code|missing picture",
            ),
            (
                "non_existing_pps",
                "non-existing PPS|non-existing SPS|sps_id .* out of range|pps_id .* out of range",
            ),
            ("application_invalid", "Application provided invalid"),
            ("slice_header", "decode_slice_header error|slice header damaged"),
            (
                "mb_decode",
                "\\bmb decoding\\b|MB decoding error|cbp too large|ac-tex damaged|AC tex damaged|dc-tex damaged",
            ),
            ("co_located_poc", "co located POCs unavailable|co-located"),
            ("bytestream", "bytestream"),
            (
                "decode_error",
                "error while decoding|error decoding|Error decoding|decoding error",
            ),
            ("nonmono_dts", "non[- ]monoton(ous|ic) (DTS|PTS)|out of order"),
            ("invalid_dts", "Invalid (DTS|PTS)"),
            (
                "guess_pts",
                "replacing by guess|generating non-monotonous|generating non-monotonic",
            ),
        ];
        for (category, pattern) in patterns {
            if regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .is_ok_and(|re| re.is_match(line))
            {
                self.warnings.push(FfmpegWarning {
                    category: category.to_string(),
                    message: line.to_string(),
                });
                return;
            }
        }
        let lower = line.to_ascii_lowercase();
        if lower.contains("error") && lower.contains('@') && !lower.contains("frame=") {
            self.warnings.push(FfmpegWarning {
                category: "other_error".to_string(),
                message: line.to_string(),
            });
        }
    }
    pub fn warnings(&self) -> &[FfmpegWarning] {
        &self.warnings
    }
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct FfmpegRunner {
    paths: FfmpegPaths,
}

impl FfmpegRunner {
    pub fn new(paths: FfmpegPaths) -> Self {
        Self { paths }
    }
    pub fn available(&self) -> bool {
        self.paths.is_available()
    }

    pub fn paths(&self) -> &FfmpegPaths {
        &self.paths
    }

    pub async fn concat_copy(
        &self,
        inputs: &[PathBuf],
        output: &Path,
        cancel: CancellationToken,
    ) -> anyhow::Result<WarningTracker> {
        self.run_concat(inputs, output, &["-c".to_string(), "copy".to_string()], cancel)
            .await
    }

    pub async fn concat_encode(
        &self,
        inputs: &[PathBuf],
        output: &Path,
        encode_args: &[String],
        cancel: CancellationToken,
    ) -> anyhow::Result<WarningTracker> {
        let mut args = encode_args.to_vec();
        args.push("-c:a".to_string());
        args.push("copy".to_string());
        self.run_concat(inputs, output, &args, cancel).await
    }

    async fn run_concat(
        &self,
        inputs: &[PathBuf],
        output: &Path,
        codec_args: &[String],
        cancel: CancellationToken,
    ) -> anyhow::Result<WarningTracker> {
        let ffmpeg = self
            .paths
            .ffmpeg
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ffmpeg binary is unavailable"))?;
        let parent = output
            .parent()
            .ok_or_else(|| anyhow::anyhow!("output path has no parent"))?;
        tokio::fs::create_dir_all(parent).await?;
        let list_path = output.with_extension("mp4.concat_list.txt");
        let mut list = String::new();
        for input in inputs {
            if writeln!(list, "file '{}'", input.to_string_lossy().replace('\'', "'\\''")).is_err() {
                anyhow::bail!("failed to build concat list");
            }
        }
        tokio::fs::write(&list_path, list).await?;

        let list_arg = list_path.to_string_lossy().to_string();
        let output_arg = output.to_string_lossy().to_string();
        let mut args = vec![
            "-y".to_string(),
            "-progress".to_string(),
            "pipe:2".to_string(),
            "-nostats".to_string(),
            "-loglevel".to_string(),
            "warning".to_string(),
            "-fflags".to_string(),
            "+genpts".to_string(),
            "-f".to_string(),
            "concat".to_string(),
            "-safe".to_string(),
            "0".to_string(),
            "-i".to_string(),
            list_arg,
        ];
        args.extend_from_slice(codec_args);
        args.push("-movflags".to_string());
        args.push("+faststart".to_string());
        args.push(output_arg);
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let result = self.run(ffmpeg, &arg_refs, cancel).await;
        let _ = tokio::fs::remove_file(&list_path).await;
        result
    }

    async fn run(&self, program: &str, args: &[&str], cancel: CancellationToken) -> anyhow::Result<WarningTracker> {
        let mut command = Command::new(program);
        command.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
        self.paths.apply_library_env(&mut command);
        let mut child = command.spawn()?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("ffmpeg stderr unavailable"))?;
        let mut lines = BufReader::new(stderr).lines();
        let mut tracker = WarningTracker::default();
        loop {
            tokio::select! {
                line = lines.next_line() => match line? { Some(line) => tracker.observe(&line), None => break },
                () = tokio::time::sleep(Duration::from_millis(250)), if cancel.is_cancelled() => {
                    let _ = child.kill().await;
                    anyhow::bail!("ffmpeg cancelled");
                }
            }
        }
        let status = child.wait().await?;
        if !status.success() {
            anyhow::bail!("ffmpeg exited with {status}");
        }
        Ok(tracker)
    }
}
