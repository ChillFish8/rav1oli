use std::path::PathBuf;

use clap::Args;

use super::options::{AnalysisOptions, EncoderOptions};

#[derive(Debug, Args)]
pub struct AnalyseArgs {
    /// The source video to encode.
    #[arg(short, long)]
    pub input: PathBuf,
    /// Where to write the distortion map.
    #[arg(short, long)]
    pub map: PathBuf,

    #[command(flatten)]
    pub analysis: AnalysisOptions,

    #[command(flatten)]
    pub encoder: EncoderOptions,
}

pub fn execute(args: AnalyseArgs) -> anyhow::Result<()> {
    tracing::debug!(?args, "running analyse");

    unimplemented!()
}
