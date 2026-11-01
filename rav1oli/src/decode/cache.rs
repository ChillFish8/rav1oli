use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;

const CACHE_FOLDER: &str = ".rav1oli";
const HASH_LENGTH: usize = 16;

static PROBE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Where index and lock files for a video are stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CacheLocation {
    /// A `.rav1oli` folder in the directory holding the video.
    #[default]
    BesideVideo,
    /// One shared folder for every video.
    Directory(PathBuf),
}

/// The files used to index a single video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePaths {
    /// The index path without an extension.
    ///
    /// BestSource appends the track and suffix.
    pub index: PathBuf,
    pub lock: PathBuf,
}

impl CacheLocation {
    /// Hashes the video, creates the cache folders and returns the paths for its index and lock.
    pub fn paths_for(&self, video: &Path) -> anyhow::Result<CachePaths> {
        let stem = content_stem(video)?;
        let cache_root = self.cache_root(video);
        let indexes = cache_root.join("indexes");
        let locks = cache_root.join("locks");

        for folder in [&indexes, &locks] {
            let context = || unwritable_folder_message(folder);
            std::fs::create_dir_all(folder).with_context(context)?;
            check_writable(folder).with_context(context)?;
        }

        Ok(CachePaths {
            index: indexes.join(&stem),
            lock: locks.join(format!("{stem}.lock")),
        })
    }

    pub(super) fn cache_root(&self, video: &Path) -> PathBuf {
        match self {
            Self::Directory(directory) => directory.clone(),
            Self::BesideVideo => {
                let parent = match video.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => parent,
                    _ => Path::new("."),
                };
                parent.join(CACHE_FOLDER)
            },
        }
    }
}

fn unwritable_folder_message(folder: &Path) -> String {
    let path = folder.display();
    let advice = "pass --cache-dir to use a writable location";
    format!("could not create or write to the cache folder {path}, {advice}")
}

fn check_writable(folder: &Path) -> std::io::Result<()> {
    let process = std::process::id();
    let sequence = PROBE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let probe_name = format!(".rav1oli-write-probe-{process}-{sequence}");
    let probe = folder.join(probe_name);

    std::fs::write(&probe, b"")?;

    if let Err(error) = std::fs::remove_file(&probe) {
        let probe_path = probe.display();
        tracing::debug!("could not remove the write probe {probe_path}: {error}");
    }

    Ok(())
}

fn content_stem(video: &Path) -> anyhow::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let read_context = || format!("could not read the video {}", video.display());
    hasher.update_mmap_rayon(video).with_context(read_context)?;
    let hex = hasher.finalize().to_hex();

    let name_context = || format!("the video path {} has no file name", video.display());
    let file_name = video.file_name().with_context(name_context)?;
    let file_name = file_name.to_string_lossy();
    Ok(format!("{file_name}-{}", &hex[..HASH_LENGTH]))
}
