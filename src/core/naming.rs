use std::{path::Path, sync::OnceLock};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct ParsedName {
    pub camera: String,
    pub timestamp: Option<DateTime<FixedOffset>>,
}

static RE: OnceLock<Vec<Regex>> = OnceLock::new();

pub fn parse_filename(path: &Path) -> ParsedName {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
    for regex in patterns() {
        if let Some(caps) = regex.captures(name) {
            let timestamp = caps
                .name("dt")
                .and_then(|m| parse_dt(m.as_str()))
                .or_else(|| parse_joined(&caps));
            let camera = caps
                .name("cam")
                .map(|m| m.as_str().to_ascii_uppercase())
                .or_else(|| special_camera(path, name))
                .unwrap_or_else(|| infer_parent(path));
            return ParsedName { camera, timestamp };
        }
    }
    ParsedName {
        camera: special_camera(path, name).unwrap_or_else(|| infer_parent(path)),
        timestamp: None,
    }
}

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| matches!(ext.as_str(), "mp4" | "ts" | "mov" | "mkv"))
}

pub fn mp4_name_for_ts(camera: &str, ts: DateTime<FixedOffset>) -> String {
    format!("{}_{}.mp4", sanitize(camera), ts.format("%Y%m%d_%H%M%S"))
}

pub fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn patterns() -> &'static [Regex] {
    RE.get_or_init(|| {
        [
            r"^(?P<dt>\d{14})_\d{14}_\d+(?P<cam>[A-Za-z]+)\.(?:MP4|TS)$",
            r"^(?P<dt>\d{14})_\d+(?P<cam>[A-Za-z]+)\.MP4$",
            r"^[A-Za-z]+(?P<date>\d{8})-(?P<time>\d{6})-\d+(?P<cam>[A-Za-z]+)\.(?:MP4|TS)$",
            r"^[A-Za-z]*\d+_(?P<dt>\d{14})\.(?:MP4|TS)$",
            r"^(?P<date>\d{8})_(?P<h>\d{2})h(?P<m>\d{2})m(?P<s>\d{2})s(?:-\d+)?\.(?:MP4|TS)$",
            r"^(?P<cam>\d{2})_(?P<dt>\d{14})_\d{14}\.(?:MP4|TS)$",
            r"^\d{2}M\d{2}S_(?P<unix>\d{10})\.(?:MP4|TS)$",
        ]
        .into_iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
    })
}

fn parse_joined(caps: &regex::Captures<'_>) -> Option<DateTime<FixedOffset>> {
    if let (Some(date), Some(time)) = (caps.name("date"), caps.name("time")) {
        return parse_dt(&format!("{}{}", date.as_str(), time.as_str()));
    }
    if let (Some(date), Some(h), Some(m), Some(s)) = (caps.name("date"), caps.name("h"), caps.name("m"), caps.name("s"))
    {
        return parse_dt(&format!("{}{}{}{}", date.as_str(), h.as_str(), m.as_str(), s.as_str()));
    }
    if let Some(unix) = caps.name("unix").and_then(|m| m.as_str().parse::<i64>().ok()) {
        return Utc.timestamp_opt(unix, 0).single().map(|dt| dt.fixed_offset());
    }
    None
}

fn parse_dt(value: &str) -> Option<DateTime<FixedOffset>> {
    NaiveDateTime::parse_from_str(value, "%Y%m%d%H%M%S")
        .ok()
        .map(|dt| Utc.from_utc_datetime(&dt).fixed_offset())
}

fn special_camera(path: &Path, name: &str) -> Option<String> {
    if Regex::new(r"^[A-Za-z]*\d+_\d{14}\.(?:MP4|TS)$").ok()?.is_match(name) {
        return Some(format!("AR_IMX335:{}", infer_parent(path)));
    }
    if Regex::new(r"^\d{8}_\d{2}h\d{2}m\d{2}s").ok()?.is_match(name) {
        return Some(format!("LS_S3:{}", infer_parent(path)));
    }
    for part in path.components().filter_map(|c| c.as_os_str().to_str()) {
        if let Some(tail) = part.strip_prefix("XiaomiCamera_") {
            return Some(format!("XIAOMI_A:{tail}"));
        }
        if part.len() == 12 && part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(format!("XIAOMI_B:{}", part.to_ascii_lowercase()));
        }
    }
    None
}

fn infer_parent(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map_or_else(|| "SINGLE_CAMERA".to_string(), |s| sanitize(&s.to_ascii_uppercase()))
}
