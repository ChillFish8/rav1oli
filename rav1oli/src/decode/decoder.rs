use std::iter::FusedIterator;
use std::path::Path;

use anyhow::Context;
use rav1oli_bestsource::{Frame, IndexCache, OpenOptions, VideoProperties, VideoSource};

use super::cache::CacheLocation;
use super::lock::IndexLock;

/// Decodes a video's frames in order, reusing a cached index.
#[derive(Debug)]
pub struct Decoder {
    source: VideoSource,
}

impl Decoder {
    /// Opens the video, indexing it first if no index is cached.
    ///
    /// The index lock is only held while the video is opened.
    pub fn open(video: &Path, cache: &CacheLocation) -> anyhow::Result<Decoder> {
        let paths = cache.paths_for(video)?;
        let options = OpenOptions {
            cache: IndexCache::File(paths.index),
            ..Default::default()
        };

        let lock = IndexLock::acquire(&paths.lock)?;
        let opened = VideoSource::open(video, &options);
        drop(lock);

        let context = || format!("could not open the video {}", video.display());
        let source = opened.with_context(context)?;
        Ok(Decoder { source })
    }

    pub fn properties(&self) -> &VideoProperties {
        self.source.properties()
    }

    pub fn frames(&mut self) -> Frames<'_> {
        let total = self.source.properties().num_frames;
        Frames {
            source: &mut self.source,
            next: 0,
            total,
        }
    }
}

/// Yields every frame of a video in order, stopping after the first error.
#[derive(Debug)]
pub struct Frames<'a> {
    source: &'a mut VideoSource,
    next: i64,
    total: i64,
}

impl Iterator for Frames<'_> {
    type Item = anyhow::Result<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.total {
            return None;
        }

        let index = self.next;
        self.next += 1;
        let frame = self.source.frame(index);
        if frame.is_err() {
            self.next = self.total;
        }

        let context = || format!("could not decode frame {index}");
        Some(frame.with_context(context))
    }
}

impl FusedIterator for Frames<'_> {}
