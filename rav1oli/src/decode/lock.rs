use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

use anyhow::Context;

/// An exclusive lock on a video's lock file, held until dropped.
///
/// The lock file is left on disk when released.
#[derive(Debug)]
pub struct IndexLock {
    _file: File,
}

impl IndexLock {
    /// Locks the file at `path`, waiting if another process already holds it.
    pub fn acquire(path: &Path) -> anyhow::Result<IndexLock> {
        let lock_path = path.display();
        let open_context = format!("could not open the lock file {lock_path}");
        let lock_context = format!("could not lock {lock_path}");

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)
            .context(open_context)?;

        match file.try_lock() {
            Ok(()) => {},
            Err(TryLockError::WouldBlock) => {
                tracing::info!("waiting for another process to finish indexing, lock: {lock_path}");
                file.lock().context(lock_context)?;
            },
            Err(TryLockError::Error(error)) => {
                let error = anyhow::Error::new(error).context(lock_context);
                return Err(error);
            },
        }

        Ok(IndexLock { _file: file })
    }
}
