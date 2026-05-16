use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};

use crate::core::naming::{mp4_name_for_ts, parse_filename, sanitize};

#[derive(Debug, Clone)]
pub struct VideoItem {
    pub path: PathBuf,
    pub camera: String,
    pub timestamp: Option<DateTime<FixedOffset>>,
    pub end_datetime: Option<DateTime<FixedOffset>>,
    pub rest_of_filename: Option<String>,
    pub duration_ms: Option<i64>,
    pub codec: Option<String>,
    pub format_bps: Option<i64>,
    pub size_bytes: Option<i64>,
    pub default_max_time_difference: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum ScanEntry {
    Video(VideoItem),
    Broken(PathBuf),
}

#[derive(Debug, Clone)]
pub struct VideoGroup {
    pub camera: String,
    pub files: Vec<VideoItem>,
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
    pub rest_of_filename: Option<String>,
}

pub fn item_from_path(path: PathBuf, duration_ms: Option<i64>) -> VideoItem {
    let parsed = parse_filename(&path);
    let camera = parsed.camera;
    let rest_of_filename = rest_for_item(&path, &camera);
    let end_datetime = local_end_datetime(&path);
    let timestamp = parsed.timestamp.or_else(|| local_start_datetime(&path));
    VideoItem {
        path,
        camera,
        timestamp,
        end_datetime,
        rest_of_filename,
        duration_ms,
        codec: None,
        format_bps: None,
        size_bytes: None,
        default_max_time_difference: parsed.default_max_time_difference,
    }
}

pub fn item_from_probe(
    path: PathBuf,
    duration_ms: Option<i64>,
    codec: Option<String>,
    format_bps: Option<i64>,
    size_bytes: Option<i64>,
) -> VideoItem {
    let mut item = item_from_path(path, duration_ms);
    item.codec = codec;
    item.format_bps = format_bps;
    item.size_bytes = size_bytes;
    item
}

pub fn group_by_camera(entries: Vec<ScanEntry>) -> BTreeMap<String, Vec<ScanEntry>> {
    let mut map: BTreeMap<String, Vec<ScanEntry>> = BTreeMap::new();
    for entry in entries {
        let camera = match &entry {
            ScanEntry::Video(item) => item.camera.clone(),
            ScanEntry::Broken(path) => parse_filename(path).camera,
        };
        map.entry(camera).or_default().push(entry);
    }
    map
}

pub fn group_by_time(entries: Vec<ScanEntry>, gap: Option<Duration>) -> Vec<VideoGroup> {
    let mut groups = Vec::new();
    for (camera, mut camera_entries) in group_by_camera(entries) {
        camera_entries.sort_by_key(|entry| match entry {
            ScanEntry::Video(item) => item.timestamp,
            ScanEntry::Broken(path) => parse_filename(path).timestamp,
        });
        let mut current: Vec<VideoItem> = Vec::new();
        let mut previous_end: Option<DateTime<FixedOffset>> = None;
        for entry in camera_entries {
            let item = match entry {
                ScanEntry::Video(item) => item,
                ScanEntry::Broken(path) => {
                    if !current.is_empty() {
                        push_group(&mut groups, &camera, &mut current);
                    }
                    previous_end = None;
                    tracing::warn!("broken file skipped: {}", path.display());
                    continue;
                }
            };

            let effective_gap = gap.or_else(|| item.default_max_time_difference.map(Duration::from_secs));
            let should_split = match (previous_end, item.timestamp, effective_gap) {
                (Some(prev), Some(ts), Some(max_gap)) => ts
                    .signed_duration_since(prev)
                    .to_std()
                    .map_or(true, |delta| delta > max_gap),
                (Some(_), Some(_), None) | (None, _, _) | (_, None, _) => !current.is_empty(),
            };
            if should_split {
                push_group(&mut groups, &camera, &mut current);
            }
            previous_end = effective_end(&item);
            current.push(item);
        }
        push_group(&mut groups, &camera, &mut current);
    }
    groups
}

pub fn create_combined_filename(group: &VideoGroup) -> String {
    if let (Some(start), Some(rest)) = (group.start, group.rest_of_filename.as_deref()) {
        let end = group.end.or(group.start);
        if let Some(end) = end {
            return format!(
                "{}_{}_{}",
                start.format("%Y%m%d%H%M%S"),
                end.format("%Y%m%d%H%M%S"),
                rest
            );
        }
    }
    if let Some(start) = group.start {
        let end = group.end.or(group.start);
        if let Some(end) = end {
            return format!(
                "{}_{}_{}.mp4",
                start.format("%Y%m%d%H%M%S"),
                end.format("%Y%m%d%H%M%S"),
                sanitize(&group.camera)
            );
        }
    }
    if let Some(ts) = group.start {
        return mp4_name_for_ts(&group.camera, ts);
    }
    first_source_basename(group).unwrap_or_else(|| format!("{}.mp4", sanitize(&group.camera)))
}

fn push_group(groups: &mut Vec<VideoGroup>, camera: &str, current: &mut Vec<VideoItem>) {
    if current.is_empty() {
        return;
    }
    let start = current.first().and_then(|item| item.timestamp);
    let end = current.last().and_then(|item| effective_end(item).or(item.timestamp));
    let rest_of_filename = current.first().and_then(|item| item.rest_of_filename.clone());
    groups.push(VideoGroup {
        camera: camera.to_string(),
        files: std::mem::take(current),
        start,
        end,
        rest_of_filename,
    });
}

fn effective_end(item: &VideoItem) -> Option<DateTime<FixedOffset>> {
    item.end_datetime.or_else(|| match (item.timestamp, item.duration_ms) {
        (Some(ts), Some(dur)) if dur > 0 => Some(ts + chrono::Duration::milliseconds(dur)),
        _ => None,
    })
}

fn rest_for_item(path: &Path, camera: &str) -> Option<String> {
    if camera.starts_with("AR_IMX335:") {
        return Some("AR_IMX335.mp4".to_string());
    }
    if camera.starts_with("LS_S3:") {
        return Some("LS_S3.mp4".to_string());
    }
    if let Some(rest) = xiaomi_a_rest(path) {
        return Some(rest);
    }
    if has_xiaomi_a_path_segment(path) || camera.starts_with("XIAOMI_A:") {
        return xiaomi_a_rest_from_camera(camera).or_else(|| Some("Xiaomi.mp4".to_string()));
    }
    if camera.starts_with("XIAOMI_B:") {
        return xiaomi_b_rest(path)
            .or_else(|| xiaomi_b_rest_from_camera(camera))
            .or_else(|| Some("Xiaomi.mp4".to_string()));
    }
    if is_xiaomi_b_basename(path) {
        return xiaomi_b_rest(path).or_else(|| Some("Xiaomi.mp4".to_string()));
    }
    generic_dvr_rest(path)
}

fn xiaomi_a_rest(path: &Path) -> Option<String> {
    path_parts(path)
        .into_iter()
        .find_map(|part| part.strip_prefix("XiaomiCamera_").and_then(xiaomi_a_rest_from_tail))
}

fn has_xiaomi_a_path_segment(path: &Path) -> bool {
    path_parts(path)
        .into_iter()
        .any(|part| part.starts_with("XiaomiCamera_"))
}

fn xiaomi_a_rest_from_camera(camera: &str) -> Option<String> {
    camera.strip_prefix("XIAOMI_A:").and_then(|tail| {
        let tail = tail.strip_prefix("XiaomiCamera_").unwrap_or(tail);
        xiaomi_a_rest_from_tail(tail)
    })
}

fn xiaomi_a_rest_from_tail(tail: &str) -> Option<String> {
    let mut pieces = tail.split('_');
    let index = pieces.next().filter(|value| is_xiaomi_camera_number(value))?;
    let mac = pieces.find(|value| is_12_hex(value)).and_then(last4_hex_upper)?;
    Some(format!("Xiaomi_{index}_{mac}.mp4"))
}

fn xiaomi_b_rest(path: &Path) -> Option<String> {
    path_parts(path)
        .into_iter()
        .find(|part| is_12_hex(part))
        .and_then(|part| last4_hex_upper(&part))
        .map(|mac| format!("Xiaomi_{mac}.mp4"))
}

fn xiaomi_b_rest_from_camera(camera: &str) -> Option<String> {
    camera
        .strip_prefix("XIAOMI_B:")
        .filter(|mac| is_12_hex(mac))
        .and_then(last4_hex_upper)
        .map(|mac| format!("Xiaomi_{mac}.mp4"))
}

fn path_parts(path: &Path) -> Vec<String> {
    path.to_string_lossy()
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn last4_hex_upper(value: &str) -> Option<String> {
    let tail = value.get(value.len().checked_sub(4)?..)?;
    Some(tail.to_ascii_uppercase())
}

fn is_12_hex(value: &str) -> bool {
    value.len() == 12 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_xiaomi_camera_number(value: &str) -> bool {
    value.len() == 2 && value.chars().all(|c| c.is_ascii_digit())
}

fn local_start_datetime(path: &Path) -> Option<DateTime<FixedOffset>> {
    let stem = path.file_stem()?.to_str()?;
    let parts = stem.split('_').collect::<Vec<_>>();
    match parts.as_slice() {
        [start, ..] if is_14_digit_datetime(start) => parse_local_dt(start),
        [camera, start, ..] if is_xiaomi_camera_number(camera) && is_14_digit_datetime(start) => parse_local_dt(start),
        [prefix, start] if is_alphanumeric(prefix) && is_14_digit_datetime(start) => parse_local_dt(start),
        [date, time] if is_8_digit_date(date) => parse_ls_s3_dt(date, time),
        [elapsed, unix] if is_xiaomi_b_elapsed(elapsed) && is_10_digit_unix_seconds(unix) => parse_unix_seconds(unix),
        _ => None,
    }
}

fn local_end_datetime(path: &Path) -> Option<DateTime<FixedOffset>> {
    let stem = path.file_stem()?.to_str()?;
    let parts = stem.split('_').collect::<Vec<_>>();
    match parts.as_slice() {
        [start, end, ..] if is_14_digit_datetime(start) && is_14_digit_datetime(end) => parse_local_dt(end),
        [camera, start, end]
            if is_xiaomi_camera_number(camera) && is_14_digit_datetime(start) && is_14_digit_datetime(end) =>
        {
            parse_local_dt(end)
        }
        _ => None,
    }
}

fn generic_dvr_rest(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let ext = normalized_output_extension(path)?;
    let parts = stem.split('_').collect::<Vec<_>>();
    match parts.as_slice() {
        [start, end, rest @ ..] if is_14_digit_datetime(start) && is_14_digit_datetime(end) && !rest.is_empty() => {
            Some(format!("{}.{}", rest.join("_"), ext))
        }
        [start, rest @ ..] if is_14_digit_datetime(start) && !rest.is_empty() => {
            Some(format!("{}.{}", rest.join("_"), ext))
        }
        _ => None,
    }
}

fn first_source_basename(group: &VideoGroup) -> Option<String> {
    let path = &group.files.first()?.path;
    let name = path.file_name()?.to_str()?;
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
    {
        let stem = path.file_stem()?.to_str()?;
        return Some(format!("{stem}.mp4"));
    }
    Some(name.to_string())
}

fn normalized_output_extension(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("ts") {
        Some("mp4".to_string())
    } else {
        Some(ext.to_string())
    }
}

fn is_14_digit_datetime(value: &str) -> bool {
    value.len() == 14 && value.chars().all(|c| c.is_ascii_digit())
}

fn is_8_digit_date(value: &str) -> bool {
    value.len() == 8 && value.chars().all(|c| c.is_ascii_digit())
}

fn is_10_digit_unix_seconds(value: &str) -> bool {
    value.len() == 10 && value.chars().all(|c| c.is_ascii_digit())
}

fn is_alphanumeric(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_xiaomi_b_elapsed(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 6
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b'M'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && bytes[5] == b'S'
}

fn is_xiaomi_b_basename(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
        return false;
    };
    let parts = stem.split('_').collect::<Vec<_>>();
    matches!(parts.as_slice(), [elapsed, unix] if is_xiaomi_b_elapsed(elapsed) && is_10_digit_unix_seconds(unix))
}

fn parse_ls_s3_dt(date: &str, time: &str) -> Option<DateTime<FixedOffset>> {
    let time = time.split_once('-').map_or(time, |(head, _)| head);
    if time.len() != 9 {
        return None;
    }
    let bytes = time.as_bytes();
    if bytes.get(2) != Some(&b'h') || bytes.get(5) != Some(&b'm') || bytes.get(8) != Some(&b's') {
        return None;
    }
    let hh = &time[0..2];
    let mm = &time[3..5];
    let ss = &time[6..8];
    if !hh.chars().all(|c| c.is_ascii_digit())
        || !mm.chars().all(|c| c.is_ascii_digit())
        || !ss.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    parse_local_dt(&format!("{date}{hh}{mm}{ss}"))
}

fn parse_unix_seconds(value: &str) -> Option<DateTime<FixedOffset>> {
    let seconds = value.parse::<i64>().ok()?;
    Utc.timestamp_opt(seconds, 0).single().map(|dt| dt.fixed_offset())
}

fn parse_local_dt(value: &str) -> Option<DateTime<FixedOffset>> {
    NaiveDateTime::parse_from_str(value, "%Y%m%d%H%M%S")
        .ok()
        .map(|dt| Utc.from_utc_datetime(&dt).fixed_offset())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grouped(paths: &[(&str, i64)]) -> VideoGroup {
        let files = paths
            .iter()
            .map(|(path, duration_ms)| ScanEntry::Video(item_from_path(PathBuf::from(path), Some(*duration_ms))))
            .collect::<Vec<_>>();
        let mut groups = group_by_time(files, Some(Duration::from_secs(120)));
        assert_eq!(groups.len(), 1);
        groups.remove(0)
    }

    fn item_start(path: &str) -> String {
        item_from_path(PathBuf::from(path), Some(60_000))
            .timestamp
            .unwrap()
            .format("%Y%m%d%H%M%S")
            .to_string()
    }

    #[test]
    fn group_by_time_splits_when_no_effective_gap_threshold() {
        let files = [
            "/src/CAM/20250101000000_first.mp4",
            "/src/CAM/20250101000100_second.mp4",
        ]
        .into_iter()
        .map(|path| ScanEntry::Video(item_from_path(PathBuf::from(path), Some(60_000))))
        .collect::<Vec<_>>();

        let groups = group_by_time(files, None);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].files.len(), 1);
        assert_eq!(groups[1].files.len(), 1);
    }

    #[test]
    fn group_by_time_uses_item_default_gap_when_caller_gap_missing() {
        let files = [
            "/src/CAM/20250101000000_000001AA.MP4",
            "/src/CAM/20250101000100_000002AA.MP4",
        ]
        .into_iter()
        .map(|path| ScanEntry::Video(item_from_path(PathBuf::from(path), Some(60_000))))
        .collect::<Vec<_>>();

        let groups = group_by_time(files, None);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn local_start_datetime_parses_lowercase_extension_patterns() {
        assert_eq!(item_start("/src/CAM/20250419195801_000785AC.mp4"), "20250419195801");
        assert_eq!(
            item_start("/src/CAM/20250419195801_20250419200001_000785AC.ts"),
            "20250419195801"
        );
        assert_eq!(
            item_start("/src/XiaomiCamera_05_AABBCCDDFACE/05_20250516070050_20250516072431.mp4"),
            "20250516070050"
        );
        assert_eq!(item_start("/src/front/A119_20250516070050.mp4"), "20250516070050");
        assert_eq!(item_start("/src/LS_S3/20260503_15h10m04s-2.ts"), "20260503151004");
        assert_eq!(item_start("/src/aabbccddface/00M56S_1747350056.mp4"), "20250515230056");
    }

    #[test]
    fn combined_filename_reuses_first_common_dvr_rest() {
        let group = grouped(&[
            ("/src/CAM/20250419195801_000785AC.MP4", 60_000),
            ("/src/CAM/20250419195901_000786AC.MP4", 60_000),
        ]);
        assert_eq!(
            create_combined_filename(&group),
            "20250419195801_20250419200001_000785AC.MP4"
        );
    }

    #[test]
    fn common_dvr_under_12_hex_dir_keeps_parsed_rest() {
        let group = grouped(&[
            ("/src/aabbccddface/CAM/20250419195801_000785AC.MP4", 60_000),
            ("/src/aabbccddface/CAM/20250419195901_000786AC.MP4", 60_000),
        ]);
        assert_eq!(
            create_combined_filename(&group),
            "20250419195801_20250419200001_000785AC.MP4"
        );
    }

    #[test]
    fn combined_filename_prefers_parsed_end_datetime() {
        let group = grouped(&[(
            "/src/XiaomiCamera_05_AABBCCDDFACE/05_20250516070050_20250516072431.mp4",
            60_000,
        )]);
        assert_eq!(
            create_combined_filename(&group),
            "20250516070050_20250516072431_Xiaomi_05_FACE.mp4"
        );
    }

    #[test]
    fn invalid_xiaomi_camera_a_without_mac_uses_xiaomi_fallback_rest() {
        let group = grouped(&[("/src/XiaomiCamera_05/05_20250516070050_20250516072431.mp4", 60_000)]);
        assert_eq!(
            create_combined_filename(&group),
            "20250516070050_20250516072431_Xiaomi.mp4"
        );
    }

    #[test]
    fn xiaomi_b_combined_filename_uses_path_mac_suffix() {
        let group = grouped(&[
            ("/src/aabbccddface/00M56S_1747350056.mp4", 60_000),
            ("/src/aabbccddface/01M56S_1747350116.mp4", 60_000),
        ]);
        assert_eq!(
            create_combined_filename(&group),
            "20250515230056_20250515230256_Xiaomi_FACE.mp4"
        );
    }

    #[test]
    fn xiaomi_b_without_mac_uses_base_parsed_rest() {
        let group = grouped(&[
            ("/src/cam/00M56S_1747350056.mp4", 60_000),
            ("/src/cam/01M56S_1747350116.mp4", 60_000),
        ]);
        assert_eq!(
            create_combined_filename(&group),
            "20250515230056_20250515230256_Xiaomi.mp4"
        );
    }

    #[test]
    fn ar_imx335_combined_filename_uses_fixed_rest() {
        let group = grouped(&[
            ("/src/front/A119_20250516070050.MP4", 60_000),
            ("/src/front/A119_20250516070150.MP4", 60_000),
        ]);
        assert_eq!(
            create_combined_filename(&group),
            "20250516070050_20250516070250_AR_IMX335.mp4"
        );
    }

    #[test]
    fn generic_dvr_ts_rest_is_converted_to_mp4() {
        let group = grouped(&[("/src/CAM/20250419195801_20250419200001_000785AC.TS", 60_000)]);
        assert_eq!(
            create_combined_filename(&group),
            "20250419195801_20250419200001_000785AC.mp4"
        );
    }

    #[test]
    fn timestampless_fallback_uses_first_basename_not_camera_combined() {
        let group = grouped(&[("/src/CAM/unparsed_source.ts", 60_000)]);
        let filename = create_combined_filename(&group);
        assert_eq!(filename, "unparsed_source.mp4");
        assert!(!filename.contains("combined"));
    }

    #[test]
    fn combined_filename_uses_fixed_camera_rest_and_converts_ts() {
        let group = grouped(&[
            ("/src/LS_S3/20260503_15h10m04s.ts", 60_000),
            ("/src/LS_S3/20260503_15h11m04s-2.ts", 60_000),
        ]);
        assert_eq!(
            create_combined_filename(&group),
            "20260503151004_20260503151204_LS_S3.mp4"
        );
    }
}
