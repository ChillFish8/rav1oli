use std::path::PathBuf;

use clap::Args;

use super::options::EncoderOptions;

#[derive(Debug, Args)]
pub struct EncodeArgs {
    /// The source video to encode.
    #[arg(short, long)]
    pub input: PathBuf,
    /// The distortion map produced by the `analyse` command.
    #[arg(short, long)]
    pub map: PathBuf,
    /// Where to write the encoded video.
    #[arg(short, long)]
    pub output: PathBuf,

    #[command(flatten)]
    pub encoder: EncoderOptions,
}

pub fn execute(args: EncodeArgs) -> anyhow::Result<()> {
    tracing::debug!(?args, "running encode");

    unimplemented!()
}
