use std::path::PathBuf;

use clap::Args;

use super::options::{AnalysisOptions, CacheOptions, EncoderOptions};

#[derive(Debug, Args)]
pub struct RunArgs {
    /// The source video to encode.
    #[arg(short, long)]
    pub input: PathBuf,
    /// Where to write the encoded video.
    #[arg(short, long)]
    pub output: PathBuf,
    /// Where to keep the distortion map.
    ///
    /// A temporary file is used if this is not set.
    #[arg(short, long)]
    pub map: Option<PathBuf>,

    #[command(flatten)]
    pub analysis: AnalysisOptions,

    #[command(flatten)]
    pub encoder: EncoderOptions,

    #[command(flatten)]
    pub cache: CacheOptions,
}

pub fn execute(args: RunArgs) -> anyhow::Result<()> {
    tracing::debug!(?args, "running end to end");

    unimplemented!()
}
