use std::path::{Path, PathBuf};

use clap::Args;
use clap::error::ErrorKind;

use super::options::{AnalysisOptions, CacheOptions, EncoderOptions};
use crate::analysis::{analyse, sibling_directory};
use crate::interrupt;

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
        if let Some(keep_encode) = &self.keep_encode {
            check_file_path("--keep-encode", keep_encode)?;
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
}

/// Rejects an output path that is an existing directory or ends in a path separator.
fn check_file_path(flag: &str, path: &Path) -> Result<(), clap::Error> {
    let text = path.as_os_str().to_string_lossy();
    let has_trailing_separator = text.ends_with(std::path::is_separator);
    if !path.is_dir() && !has_trailing_separator {
        return Ok(());
    }

    let message = format!("the argument '{flag}' must be a file, but {text} names a directory");
    Err(clap::Error::raw(ErrorKind::ValueValidation, message))
}

/// Compares the paths as given, then as resolved through the file system.
///
/// A path that does not exist yet is resolved through its parent directory.
fn is_same_file(first: &Path, second: &Path) -> bool {
    if first == second {
        return true;
    }

    let first = resolve(first);
    let second = resolve(second);
    match (first, second) {
        (Some(first), Some(second)) => first == second,
        _ => false,
    }
}

fn resolve(path: &Path) -> Option<PathBuf> {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return Some(resolved);
    }

    let file_name = path.file_name()?;
    let directory = sibling_directory(path);
    let resolved_directory = std::fs::canonicalize(directory).ok()?;
    Some(resolved_directory.join(file_name))
}

pub fn execute(args: AnalyseArgs) -> anyhow::Result<()> {
    tracing::debug!(?args, "running analyse");
    interrupt::install()?;

    analyse(&args)?;
    Ok(())
}
