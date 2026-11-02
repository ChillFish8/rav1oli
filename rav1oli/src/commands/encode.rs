use std::path::PathBuf;

use clap::Args;
use clap::error::ErrorKind;

use super::options::{CacheOptions, EncoderOptions};
use crate::encoding::{EncodeRequest, encode};
use crate::interrupt;
use crate::tables::Tables;
use crate::video::{check_file_path, is_same_file};

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
    /// A table file replacing the default drop and cut curves.
    #[arg(long)]
    pub tables: Option<PathBuf>,

    #[command(flatten)]
    pub encoder: EncoderOptions,

    #[command(flatten)]
    pub cache: CacheOptions,
}

impl EncodeArgs {
    /// Checks the output is a file that does not overwrite an input, the map exists and the tables are valid.
    pub fn validate_paths(&self) -> Result<(), clap::Error> {
        check_file_path("--output", &self.output)?;

        if is_same_file(&self.output, &self.input) {
            let message = "the arguments '--output' and '--input' must be different files";
            return Err(clap::Error::raw(ErrorKind::ArgumentConflict, message));
        }

        if is_same_file(&self.output, &self.map) {
            let message = "the arguments '--output' and '--map' must be different files";
            return Err(clap::Error::raw(ErrorKind::ArgumentConflict, message));
        }

        if is_same_file(&self.map, &self.input) {
            let message = "the arguments '--map' and '--input' must be different files";
            return Err(clap::Error::raw(ErrorKind::ArgumentConflict, message));
        }

        if !self.map.is_file() {
            let map = self.map.display();
            let message = format!("invalid value for '--map': {map} is not an existing file");
            return Err(clap::Error::raw(ErrorKind::ValueValidation, message));
        }

        let Some(tables) = &self.tables else {
            return Ok(());
        };

        if let Err(error) = Tables::load(tables) {
            let message = format!("invalid value for '--tables': {error:#}");
            return Err(clap::Error::raw(ErrorKind::ValueValidation, message));
        }

        Ok(())
    }

    pub fn request(&self) -> EncodeRequest<'_> {
        EncodeRequest {
            input: &self.input,
            map: &self.map,
            output: &self.output,
            tables: self.tables.as_deref(),
            encoder: &self.encoder,
            cache: self.cache.location(),
        }
    }
}

pub fn execute(args: EncodeArgs) -> anyhow::Result<()> {
    tracing::debug!(?args, "running encode");
    interrupt::install()?;

    let request = args.request();
    encode(&request)?;
    Ok(())
}
