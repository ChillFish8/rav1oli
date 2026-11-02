use std::path::PathBuf;

use clap::Args;
use clap::error::ErrorKind;
use rav1oli_table::Metric;

use super::options::{AnalysisOptions, CacheOptions, EncoderOptions, tables_error};
use crate::analysis::{AnalyseRequest, analyse};
use crate::encoding::{EncodeOutcome, EncodeRequest, encode};
use crate::interrupt;
use crate::tables::Tables;
use crate::video::{check_file_path, check_output_directory, is_same_file, temporary_sibling};

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
    /// Keep the first-pass encode at this path instead of deleting it.
    #[arg(long)]
    pub keep_encode: Option<PathBuf>,
    /// A table file replacing the default drop and cut curves.
    #[arg(long)]
    pub tables: Option<PathBuf>,

    #[command(flatten)]
    pub analysis: AnalysisOptions,

    #[command(flatten)]
    pub encoder: EncoderOptions,

    #[command(flatten)]
    pub cache: CacheOptions,
}

impl RunArgs {
    /// Checks the outputs are files in existing directories that do not overwrite each other or the input,
    /// and the tables are valid.
    pub fn validate_paths(&self) -> Result<(), clap::Error> {
        check_file_path("--output", &self.output)?;
        check_output_directory("--output", &self.output)?;

        if let Some(map) = &self.map {
            check_file_path("--map", map)?;
            check_output_directory("--map", map)?;
        }

        if let Some(keep_encode) = &self.keep_encode {
            check_file_path("--keep-encode", keep_encode)?;
            check_output_directory("--keep-encode", keep_encode)?;
        }

        let mut paths = vec![("--input", &self.input), ("--output", &self.output)];
        if let Some(map) = &self.map {
            paths.push(("--map", map));
        }

        if let Some(keep_encode) = &self.keep_encode {
            paths.push(("--keep-encode", keep_encode));
        }

        for (index, (first_name, first_path)) in paths.iter().enumerate() {
            for (second_name, second_path) in &paths[index + 1..] {
                if is_same_file(first_path, second_path) {
                    let message =
                        format!("the arguments '{first_name}' and '{second_name}' must be different files");
                    return Err(clap::Error::raw(ErrorKind::ArgumentConflict, message));
                }
            }
        }

        let Some(tables) = &self.tables else {
            return Ok(());
        };

        let loaded = match Tables::load(tables) {
            Ok(loaded) => loaded,
            Err(error) => return Err(tables_error(&error)),
        };

        let metric = Metric::from(self.analysis.metric);
        if let Err(error) = loaded.check_metric(metric, "--metric") {
            return Err(tables_error(&error));
        }

        Ok(())
    }
}

/// Runs the first pass and then the final encode, without installing the Ctrl-C handler.
pub fn run(args: &RunArgs) -> anyhow::Result<EncodeOutcome> {
    // The temporary map is held until the end so it is deleted whether the run succeeds or fails.
    let (map_path, _temporary_map) = match &args.map {
        Some(path) => (path.clone(), None),
        None => {
            let temporary = temporary_sibling(&args.output, ".parquet")?;
            (temporary.to_path_buf(), Some(temporary))
        },
    };

    let analyse_request = AnalyseRequest {
        input: &args.input,
        map: &map_path,
        keep_encode: args.keep_encode.as_deref(),
        analysis: &args.analysis,
        encoder: &args.encoder,
        cache: args.cache.location(),
    };
    analyse(&analyse_request)?;
    interrupt::check()?;

    let encode_request = EncodeRequest {
        input: &args.input,
        map: &map_path,
        output: &args.output,
        tables: args.tables.as_deref(),
        encoder: &args.encoder,
        cache: args.cache.location(),
    };

    encode(&encode_request)
}

pub fn execute(args: RunArgs) -> anyhow::Result<()> {
    tracing::debug!(?args, "running end to end");
    interrupt::install()?;

    run(&args)?;
    Ok(())
}
