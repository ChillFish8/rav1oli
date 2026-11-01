use std::path::{Path, PathBuf};

use anyhow::Context;

const CACHE_FOLDER: &str = ".rav1oli";
const HASH_LENGTH: usize = 16;

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
            let context = || {
                let path = folder.display();
                let advice = "pass --cache-dir to use a writable location";
                format!("could not create the cache folder {path}, {advice}")
            };
            std::fs::create_dir_all(folder).with_context(context)?;
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

fn content_stem(video: &Path) -> anyhow::Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher
        .update_mmap(video)
        .with_context(|| format!("could not hash the video {}", video.display()))?;
    let hex = hasher.finalize().to_hex();

    let file_name = video
        .file_name()
        .with_context(|| format!("the video path {} has no file name", video.display()))?
        .to_string_lossy();
    Ok(format!("{file_name}-{}", &hex[..HASH_LENGTH]))
}
