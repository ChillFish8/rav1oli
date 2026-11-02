use std::path::Path;

use rav1oli_svt::EncoderSettings;

use crate::decode::Decoder;
use crate::video::{SourceVideo, encode_video};

/// Encodes every frame of `decoder` to `output` with no distortion grids, returning the frames written.
pub fn encode_first_pass(
    decoder: &mut Decoder,
    source: &SourceVideo,
    settings: &EncoderSettings,
    output: &Path,
) -> anyhow::Result<u64> {
    encode_video(decoder, source, settings, output, "first pass", |_| Ok(None))
}
