use std::path::Path;

use crate::core::encoder::{EncodeProfile, Encoder};

pub struct AutoEncoder;
impl Encoder for AutoEncoder {
    fn id(&self) -> &'static str {
        "auto"
    }
    fn display_name(&self) -> &'static str {
        "Auto"
    }
    fn description(&self) -> &'static str {
        "Pipeline resolver: prefer NVENC/x265 when useful, otherwise copy"
    }
    fn supports_codec(&self, _input_codec: &str) -> bool {
        true
    }
    fn probe_available(&self, ffmpeg_bin: &Path, _ld_lib: Option<&Path>) -> bool {
        ffmpeg_bin.exists()
    }
    fn encode_args(&self, _profile: &EncodeProfile) -> Vec<String> {
        Vec::new()
    }
}
