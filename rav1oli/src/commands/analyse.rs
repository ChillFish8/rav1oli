use std::path::PathBuf;

use clap::Args;
use clap::error::ErrorKind;

use super::options::{AnalysisOptions, CacheOptions, EncoderOptions};
use crate::analysis::{AnalyseRequest, analyse};
use crate::interrupt;
use crate::video::{check_file_path, check_output_directory, is_same_file};

#[derive(Debug, Args)]
pub struct AnalyseArgs {
    /// The source video to encode.
    #[arg(short, long)]
    pub input: PathBuf,
    /// Where to write the distortion map.
    #[arg(short, long)]
    pub map: PathBuf,
    /// Keep the first-pass encode at this path instead of deleting it.
    #[arg(long)]
    pub keep_encode: Option<PathBuf>,

    #[command(flatten)]
    pub analysis: AnalysisOptions,

    #[command(flatten)]
    pub encoder: EncoderOptions,

    #[command(flatten)]
    pub cache: CacheOptions,
}

impl AnalyseArgs {
    /// Checks the output paths are files that do not overwrite the input or each other.
    pub fn validate_paths(&self) -> Result<(), clap::Error> {
        check_file_path("--map", &self.map)?;
        check_output_directory("--map", &self.map)?;
        if let Some(keep_encode) = &self.keep_encode {
            check_file_path("--keep-encode", keep_encode)?;
            check_output_directory("--keep-encode", keep_encode)?;
        }

        if is_same_file(&self.map, &self.input) {
            let message = "the arguments '--map' and '--input' must be different files";
            return Err(clap::Error::raw(ErrorKind::ArgumentConflict, message));
        }

        let Some(keep_encode) = &self.keep_encode else {
            return Ok(());
        };

        if is_same_file(keep_encode, &self.input) {
            let message = "the argument '--keep-encode' must not be the same file as '--input'";
            return Err(clap::Error::raw(ErrorKind::ArgumentConflict, message));
        }

        if is_same_file(keep_encode, &self.map) {
            let message = "the argument '--keep-encode' must not be the same file as '--map'";
            return Err(clap::Error::raw(ErrorKind::ArgumentConflict, message));
        }

        Ok(())
    }

    pub fn request(&self) -> AnalyseRequest<'_> {
        AnalyseRequest {
            input: &self.input,
            map: &self.map,
            keep_encode: self.keep_encode.as_deref(),
            analysis: &self.analysis,
            encoder: &self.encoder,
            cache: self.cache.location(),
        }
    }
}

pub fn execute(args: AnalyseArgs) -> anyhow::Result<()> {
    tracing::debug!(?args, "running analyse");
    interrupt::install()?;

    let request = args.request();
    analyse(&request)?;
    Ok(())
}
