use std::path::Path;

use crate::core::encoder::{EncodeProfile, Encoder};

pub struct NvencEncoder;
impl Encoder for NvencEncoder {
    fn id(&self) -> &'static str {
        "nvenc-h265"
    }
    fn display_name(&self) -> &'static str {
        "NVIDIA NVENC H.265"
    }
    fn description(&self) -> &'static str {
        "Hardware H.265 encode with hevc_nvenc VBR/CQ profile"
    }
    fn supports_codec(&self, input_codec: &str) -> bool {
        matches!(input_codec, "h264" | "hevc" | "h265")
    }
    fn probe_available(&self, ffmpeg_bin: &Path, _ld_lib: Option<&Path>) -> bool {
        ffmpeg_bin.exists()
    }
    fn encode_args(&self, profile: &EncodeProfile) -> Vec<String> {
        vec![
            "-c:v".into(),
            "hevc_nvenc".into(),
            "-preset".into(),
            profile.preset.clone(),
            "-rc".into(),
            "vbr".into(),
            "-cq".into(),
            profile.cq.to_string(),
            "-b:v".into(),
            profile.bitrate.clone(),
            "-maxrate".into(),
            profile.maxrate.clone(),
            "-bufsize".into(),
            profile.bufsize.clone(),
            "-multipass".into(),
            "fullres".into(),
            "-rc-lookahead".into(),
            "32".into(),
            "-spatial-aq".into(),
            "1".into(),
            "-temporal-aq".into(),
            "1".into(),
            "-bf".into(),
            "4".into(),
            "-b_ref_mode".into(),
            "middle".into(),
        ]
    }
}
