use std::path::Path;

use crate::core::encoder::{EncodeProfile, Encoder};

pub struct CopyEncoder;
impl Encoder for CopyEncoder {
    fn id(&self) -> &'static str { "copy-only" }
    fn display_name(&self) -> &'static str { "Copy only" }
    fn description(&self) -> &'static str { "Stream-copy video and audio without transcoding" }
    fn supports_codec(&self, _input_codec: &str) -> bool { true }
    fn probe_available(&self, ffmpeg_bin: &Path, _ld_lib: Option<&Path>) -> bool { ffmpeg_bin.exists() }
    fn encode_args(&self, _profile: &EncodeProfile) -> Vec<String> { vec!["-c:v".into(), "copy".into(), "-c:a".into(), "copy".into()] }
}
