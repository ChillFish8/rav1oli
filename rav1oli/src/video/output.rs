use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Context;
use tempfile::{Builder, TempPath};

/// A temporary file in the directory holding `target`, named `.<target file name>.<random><suffix>`.
pub fn temporary_sibling(target: &Path, suffix: &str) -> anyhow::Result<TempPath> {
    let file_name = target
        .file_name()
        .with_context(|| format!("{} has no file name", target.display()))?;

    let mut prefix = OsString::from(".");
    prefix.push(file_name);
    prefix.push(".");

    let mut builder = Builder::new();
    builder.prefix(&prefix).suffix(suffix);

    #[cfg(unix)]
    {
        let permissions = std::fs::Permissions::from_mode(0o666);
        builder.permissions(permissions);
    }

    let directory = sibling_directory(target);
    let temporary = builder
        .tempfile_in(directory)
        .with_context(|| format!("could not create a temporary file in {}", directory.display()))?;

    Ok(temporary.into_temp_path())
}

/// Renames `temporary` to `target`, replacing any existing file.
pub fn persist(temporary: TempPath, target: &Path) -> anyhow::Result<()> {
    let temporary_path = temporary.display();
    let message = format!("could not move {temporary_path} to {}", target.display());

    temporary
        .persist(target)
        .map_err(|persist_error| persist_error.error)
        .context(message)
}

/// The directory holding `target`, which is the current directory for a bare file name.
pub fn sibling_directory(target: &Path) -> &Path {
    match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}
