use std::path::Path;

use crate::core::encoder::{EncodeProfile, Encoder};

pub struct X265Encoder;
impl Encoder for X265Encoder {
    fn id(&self) -> &'static str { "x265-veryslow" }
    fn display_name(&self) -> &'static str { "x265 veryslow" }
    fn description(&self) -> &'static str { "CPU libx265 veryslow CRF encode" }
    fn supports_codec(&self, _input_codec: &str) -> bool { true }
    fn probe_available(&self, ffmpeg_bin: &Path, _ld_lib: Option<&Path>) -> bool { ffmpeg_bin.exists() }
    fn encode_args(&self, profile: &EncodeProfile) -> Vec<String> { vec!["-c:v".into(), "libx265".into(), "-preset".into(), "veryslow".into(), "-crf".into(), profile.crf.to_string()] }
}
