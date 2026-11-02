use std::path::{Path, PathBuf};

use clap::error::ErrorKind;

use super::output::sibling_directory;

/// Rejects an output path that is an existing directory or ends in a path separator.
pub fn check_file_path(flag: &str, path: &Path) -> Result<(), clap::Error> {
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
pub fn is_same_file(first: &Path, second: &Path) -> bool {
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
