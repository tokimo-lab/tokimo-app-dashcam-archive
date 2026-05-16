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
            let camera = special_camera(path, name)
                .or_else(|| caps.name("cam").map(|m| m.as_str().to_ascii_uppercase()))
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
        .is_some_and(|ext| matches!(ext.as_str(), "mp4" | "ts"))
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
            r"(?i)^(?P<dt>\d{14})_\d{14}_\d+(?P<cam>[A-Za-z]+)\.(?:MP4|TS)$",
            r"(?i)^(?P<dt>\d{14})_\d+(?P<cam>[A-Za-z]+)\.MP4$",
            r"(?i)^[A-Za-z]+(?P<date>\d{8})-(?P<time>\d{6})-\d+(?P<cam>[A-Za-z]+)\.(?:MP4|TS)$",
            r"(?i)^[A-Za-z]*\d+_(?P<dt>\d{14})\.(?:MP4|TS)$",
            r"(?i)^(?P<date>\d{8})_(?P<h>\d{2})h(?P<m>\d{2})m(?P<s>\d{2})s(?:-\d+)?\.(?:MP4|TS)$",
            r"(?i)^(?P<cam>\d{2})_(?P<dt>\d{14})_\d{14}\.(?:MP4|TS)$",
            r"(?i)^\d{2}M\d{2}S_(?P<unix>\d{10})\.(?:MP4|TS)$",
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
    if Regex::new(r"(?i)^[A-Za-z]*\d+_\d{14}\.(?:MP4|TS)$")
        .ok()?
        .is_match(name)
    {
        return Some(fixed_camera(path, "AR_IMX335"));
    }
    if Regex::new(r"(?i)^\d{8}_\d{2}h\d{2}m\d{2}s(?:-\d+)?\.(?:MP4|TS)$")
        .ok()?
        .is_match(name)
    {
        return Some(fixed_camera(path, "LS_S3"));
    }
    if Regex::new(r"(?i)^\d{2}_\d{14}_\d{14}\.(?:MP4|TS)$")
        .ok()?
        .is_match(name)
    {
        return Some(fixed_camera(path, "XIAOMI_A"));
    }
    if Regex::new(r"(?i)^\d{2}M\d{2}S_\d{10}\.(?:MP4|TS)$")
        .ok()?
        .is_match(name)
    {
        return Some(fixed_camera(path, "XIAOMI_B"));
    }
    None
}

fn fixed_camera(path: &Path, key: &str) -> String {
    format!("{key}:{}", infer_device(path))
}

fn infer_device(path: &Path) -> String {
    for part in path.components().filter_map(|c| c.as_os_str().to_str()) {
        if is_ls_device(part) {
            return part.to_ascii_uppercase();
        }
        if is_xiaomi_device(part) {
            return part.to_string();
        }
        if is_12_hex(part) {
            return part.to_ascii_lowercase();
        }
    }
    infer_parent(path)
}

fn is_ls_device(value: &str) -> bool {
    let Some((prefix, suffix)) = value.split_once('_') else {
        return false;
    };
    prefix.eq_ignore_ascii_case("LS")
        && !suffix.is_empty()
        && suffix.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_xiaomi_device(value: &str) -> bool {
    let mut parts = value.split('_');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some("XiaomiCamera"), Some(number), Some(mac), None)
            if number.len() == 2
                && number.chars().all(|c| c.is_ascii_digit())
                && is_12_hex(mac)
    )
}

fn is_12_hex(value: &str) -> bool {
    value.len() == 12 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn infer_parent(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map_or_else(|| "SINGLE_CAMERA".to_string(), |s| sanitize(&s.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_video_file_accepts_only_mp4_and_ts_case_insensitive() {
        for name in ["clip.mp4", "clip.MP4", "clip.Mp4", "clip.ts", "clip.TS", "clip.tS"] {
            assert!(is_video_file(Path::new(name)), "{name} should be accepted");
        }

        for name in [
            "clip.mov",
            "clip.MOV",
            "clip.mkv",
            "clip.MKV",
            "clip.avi",
            "clip",
            "clip.mp4.bak",
        ] {
            assert!(!is_video_file(Path::new(name)), "{name} should be rejected");
        }
    }

    #[test]
    fn filename_camera_id_patterns_keep_filename_camera() {
        let combined = parse_filename(Path::new(
            "/video/XiaomiCamera_00_AABBCCDDEEFF/20250419195801_20250419200101_000785AC.TS",
        ));
        assert_eq!(combined.camera, "AC");
        assert!(combined.timestamp.is_some());

        let common = parse_filename(Path::new(
            "/video/XiaomiCamera_00_AABBCCDDEEFF/20250419195801_000785BD.MP4",
        ));
        assert_eq!(common.camera, "BD");
        assert!(common.timestamp.is_some());

        let no_vendor = parse_filename(Path::new("/video/aabbccddeeff/NO20200101-001521-002110b.TS"));
        assert_eq!(no_vendor.camera, "B");
        assert!(no_vendor.timestamp.is_some());
    }

    #[test]
    fn fixed_camera_patterns_use_path_device_key() {
        let ar = parse_filename(Path::new("/media/LS_AR/MOV2084_20260503165638.TS"));
        assert_eq!(ar.camera, "AR_IMX335:LS_AR");
        assert!(ar.timestamp.is_some());

        let ls = parse_filename(Path::new("/archive/LS_s3/20260503_15h10m04s.ts"));
        assert_eq!(ls.camera, "LS_S3:LS_S3");
        assert!(ls.timestamp.is_some());

        let xiaomi_a = parse_filename(Path::new(
            "/video/XiaomiCamera_05_AABBCCDDFACE/05_20250516070050_20250516072431.TS",
        ));
        assert_eq!(xiaomi_a.camera, "XIAOMI_A:XiaomiCamera_05_AABBCCDDFACE");
        assert!(xiaomi_a.timestamp.is_some());

        let xiaomi_b = parse_filename(Path::new("/video/AABBCCDDFACE/00M56S_1747350056.TS"));
        assert_eq!(xiaomi_b.camera, "XIAOMI_B:aabbccddface");
        assert!(xiaomi_b.timestamp.is_some());
    }
}
