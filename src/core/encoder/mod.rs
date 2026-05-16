pub mod auto;
pub mod copy;
pub mod nvenc;
pub mod x265;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Serialize;

use crate::core::ffmpeg::FfmpegPaths;

pub const X265_DEFAULT_CRF: u8 = 26;

#[derive(Debug, Clone)]
pub struct EncodeProfile {
    pub cq: u8,
    pub bitrate: String,
    pub maxrate: String,
    pub bufsize: String,
    pub preset: String,
    pub crf: u8,
}
impl Default for EncodeProfile {
    fn default() -> Self {
        Self {
            cq: 32,
            bitrate: "5M".to_string(),
            maxrate: "8M".to_string(),
            bufsize: "10M".to_string(),
            preset: "p7".to_string(),
            crf: X265_DEFAULT_CRF,
        }
    }
}

pub trait Encoder: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn supports_codec(&self, input_codec: &str) -> bool;
    fn probe_available(&self, ffmpeg_bin: &Path, ld_lib: Option<&Path>) -> bool;
    fn encode_args(&self, profile: &EncodeProfile) -> Vec<String>;
}

#[derive(Clone)]
pub struct EncoderRegistry {
    encoders: BTreeMap<&'static str, Arc<dyn Encoder>>,
    available: BTreeMap<&'static str, bool>,
}
impl EncoderRegistry {
    pub fn new_with_builtins(ffmpeg_bin: &Path, ld_lib: Option<&Path>) -> Self {
        let builtins: Vec<Arc<dyn Encoder>> = vec![
            Arc::new(auto::AutoEncoder),
            Arc::new(nvenc::NvencEncoder),
            Arc::new(x265::X265Encoder),
            Arc::new(copy::CopyEncoder),
        ];
        let mut encoders = BTreeMap::new();
        let mut available = BTreeMap::new();
        for encoder in builtins {
            available.insert(encoder.id(), encoder.probe_available(ffmpeg_bin, ld_lib));
            encoders.insert(encoder.id(), encoder);
        }
        Self { encoders, available }
    }
    pub fn list_available(&self) -> Vec<EncoderInfo> {
        self.encoders
            .values()
            .map(|encoder| EncoderInfo {
                id: encoder.id().to_string(),
                display_name: encoder.display_name().to_string(),
                description: encoder.description().to_string(),
                available: self.available.get(encoder.id()).copied().unwrap_or(false),
                args: encoder.encode_args(&EncodeProfile::default()),
                supports_h265: encoder.supports_codec("hevc"),
            })
            .collect()
    }
    pub fn get(&self, id: &str) -> Option<Arc<dyn Encoder>> {
        self.encoders.get(id).cloned()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EncoderInfo {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub available: bool,
    pub args: Vec<String>,
    pub supports_h265: bool,
}

pub fn registry(paths: &FfmpegPaths) -> Vec<EncoderInfo> {
    let Some(ffmpeg) = paths.ffmpeg.as_deref() else {
        return Vec::new();
    };
    let ffmpeg = PathBuf::from(ffmpeg);
    let lib = paths.library_dir.as_deref().map(Path::new);
    let registry = EncoderRegistry::new_with_builtins(&ffmpeg, lib);
    let _auto = registry.get("auto");
    registry.list_available()
}
