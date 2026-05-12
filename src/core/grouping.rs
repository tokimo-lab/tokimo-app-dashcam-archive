use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use chrono::{DateTime, FixedOffset};

use crate::core::naming::{mp4_name_for_ts, parse_filename, sanitize};

#[derive(Debug, Clone)]
pub struct VideoItem {
    pub path: PathBuf,
    pub camera: String,
    pub timestamp: Option<DateTime<FixedOffset>>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct VideoGroup {
    pub camera: String,
    pub files: Vec<VideoItem>,
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
}

pub fn item_from_path(path: PathBuf, duration_ms: Option<i64>) -> VideoItem {
    let parsed = parse_filename(&path);
    VideoItem {
        path,
        camera: parsed.camera,
        timestamp: parsed.timestamp,
        duration_ms,
    }
}

pub fn group_by_camera(items: Vec<VideoItem>) -> BTreeMap<String, Vec<VideoItem>> {
    let mut map: BTreeMap<String, Vec<VideoItem>> = BTreeMap::new();
    for item in items {
        map.entry(item.camera.clone()).or_default().push(item);
    }
    map
}

pub fn group_by_time(items: Vec<VideoItem>, gap: Duration) -> Vec<VideoGroup> {
    let mut groups = Vec::new();
    for (camera, mut camera_items) in group_by_camera(items) {
        camera_items.sort_by_key(|item| item.timestamp);
        let mut current: Vec<VideoItem> = Vec::new();
        let mut previous_end: Option<DateTime<FixedOffset>> = None;
        for item in camera_items {
            let should_split = match (previous_end, item.timestamp) {
                (Some(prev), Some(ts)) => ts
                    .signed_duration_since(prev)
                    .to_std()
                    .map_or(true, |delta| delta > gap),
                _ => !current.is_empty(),
            };
            if should_split {
                push_group(&mut groups, &camera, &mut current);
            }
            previous_end = item
                .timestamp
                .map(|ts| ts + chrono::Duration::milliseconds(item.duration_ms.unwrap_or(0)));
            current.push(item);
        }
        push_group(&mut groups, &camera, &mut current);
    }
    groups
}

pub fn create_combined_filename(group: &VideoGroup) -> String {
    if let (Some(start), Some(end)) = (group.start, group.end) {
        return format!(
            "{}_{}_{}.mp4",
            start.format("%Y%m%d%H%M%S"),
            end.format("%Y%m%d%H%M%S"),
            sanitize(&group.camera)
        );
    }
    if let Some(ts) = group.start {
        return mp4_name_for_ts(&group.camera, ts);
    }
    format!("{}_combined.mp4", sanitize(&group.camera))
}

fn push_group(groups: &mut Vec<VideoGroup>, camera: &str, current: &mut Vec<VideoItem>) {
    if current.is_empty() {
        return;
    }
    let start = current.iter().filter_map(|item| item.timestamp).min();
    let end = current
        .iter()
        .filter_map(|item| {
            item.timestamp
                .map(|ts| ts + chrono::Duration::milliseconds(item.duration_ms.unwrap_or(0)))
        })
        .max();
    groups.push(VideoGroup {
        camera: camera.to_string(),
        files: std::mem::take(current),
        start,
        end,
    });
}
