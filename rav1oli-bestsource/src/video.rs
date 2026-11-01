use std::ffi::{CString, c_int};
use std::path::{Path, PathBuf};
use std::ptr;

use crate::error::{BestSourceError, ERROR_BUFFER_LEN};
use crate::format::VideoFormat;
use crate::frame::Frame;
use crate::sys;

/// BestSource's cache mode that never reads or writes an index.
const CACHE_MODE_DISABLED: c_int = 0;

/// BestSource's cache mode that always writes an index under a directory, keyed by the source path.
const CACHE_MODE_ALWAYS_WRITE_SUBTREE: c_int = 2;

/// BestSource's cache mode that always writes an index at a path, beside the source when it has none.
const CACHE_MODE_ALWAYS_WRITE_ABSOLUTE: c_int = 4;

/// Where BestSource keeps the index it builds for a source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IndexCache {
    #[default]
    Disabled,
    /// Written to `<file>.<track>.bsindex` beside the source and read back on later opens.
    BesideSource,
    /// Written to `<directory>/<source path>.<track>.bsindex` and read back on later opens.
    Directory(PathBuf),
}

/// How [VideoSource::open](crate::VideoSource::open) picks and decodes a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOptions {
    /// The stream index of the track, or `-1` for the first video track.
    pub track: i32,
    /// Decoder threads, or `0` to let BestSource choose.
    pub threads: i32,
    pub cache: IndexCache,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            track: -1,
            threads: 0,
            cache: IndexCache::Disabled,
        }
    }
}

/// Stream wide properties of a video track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoProperties {
    pub num_frames: i64,
    pub fps: (i64, i64),
    pub time_base: (i64, i64),
    pub width: i32,
    pub height: i32,
    /// The largest width that is a whole multiple of the chroma subsampling.
    pub ss_mod_width: i32,
    /// The largest height that is a whole multiple of the chroma subsampling.
    pub ss_mod_height: i32,
    pub sar: (i32, i32),
    pub format: VideoFormat,
}

impl VideoProperties {
    fn from_raw(raw: &sys::BsVideoProps) -> Self {
        Self {
            num_frames: raw.num_frames,
            fps: (i64::from(raw.fps_num), i64::from(raw.fps_den)),
            time_base: (i64::from(raw.time_base_num), i64::from(raw.time_base_den)),
            width: raw.width,
            height: raw.height,
            ss_mod_width: raw.ss_mod_width,
            ss_mod_height: raw.ss_mod_height,
            sar: (raw.sar_num, raw.sar_den),
            format: VideoFormat::from_raw(&raw.format),
        }
    }
}

/// An open video track of one file.
#[derive(Debug)]
pub struct VideoSource {
    raw: *mut sys::BsVideo,
    properties: VideoProperties,
}

// SAFETY: A BestVideoSource is not tied to the thread that made it. It is not safe to share, so it is
// not `Sync`.
unsafe impl Send for VideoSource {}

impl VideoSource {
    /// Opens a video track of `path`, indexing it or loading a cached index.
    pub fn open(path: &Path, options: &OpenOptions) -> Result<Self, BestSourceError> {
        let source_path = c_path(path)?;
        let cache_path = match &options.cache {
            IndexCache::Directory(directory) => Some(c_path(directory)?),
            IndexCache::Disabled | IndexCache::BesideSource => None,
        };
        let cache_mode = match &options.cache {
            IndexCache::Disabled => CACHE_MODE_DISABLED,
            IndexCache::BesideSource => CACHE_MODE_ALWAYS_WRITE_ABSOLUTE,
            IndexCache::Directory(_) => CACHE_MODE_ALWAYS_WRITE_SUBTREE,
        };
        let cache_pointer = match &cache_path {
            Some(cache_path) => cache_path.as_ptr(),
            None => ptr::null(),
        };

        let source_pointer = source_path.as_ptr();
        let mut raw = ptr::null_mut();
        let mut error = [0u8; ERROR_BUFFER_LEN];
        let error_pointer = error.as_mut_ptr().cast();
        // SAFETY: Both paths are NUL terminated and outlive the call. `raw` and `error_pointer` are
        // valid out pointers and `ERROR_BUFFER_LEN` is the buffer's real size.
        let status = unsafe {
            sys::bs_video_open(
                source_pointer,
                options.track,
                options.threads,
                cache_mode,
                cache_pointer,
                &raw mut raw,
                error_pointer,
                ERROR_BUFFER_LEN,
            )
        };
        if status != 0 {
            let error = BestSourceError::from_buffer(&error);
            return Err(error);
        }

        let mut raw_properties = sys::BsVideoProps::default();
        // SAFETY: `raw` was just opened and `raw_properties` is a valid out pointer.
        unsafe { sys::bs_video_properties(raw, &raw mut raw_properties) };
        let properties = VideoProperties::from_raw(&raw_properties);

        Ok(Self { raw, properties })
    }

    pub fn properties(&self) -> &VideoProperties {
        &self.properties
    }

    /// Decodes the frame at `index`, counting from zero.
    pub fn frame(&mut self, index: i64) -> Result<Frame, BestSourceError> {
        let mut raw = ptr::null_mut();
        let mut error = [0u8; ERROR_BUFFER_LEN];
        let error_pointer = error.as_mut_ptr().cast();
        // SAFETY: `self.raw` is live and borrowed mutably, so nothing else decodes at once. `raw` and
        // `error_pointer` are valid out pointers and `ERROR_BUFFER_LEN` is the buffer's real size.
        let status =
            unsafe { sys::bs_video_frame(self.raw, index, &raw mut raw, error_pointer, ERROR_BUFFER_LEN) };
        if status != 0 {
            let error = BestSourceError::from_buffer(&error);
            return Err(error);
        }

        // SAFETY: The shim handed over a live frame that only the returned `Frame` frees.
        let frame = unsafe { Frame::from_raw(raw) };

        Ok(frame)
    }
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `bs_video_open` and only this value frees it.
        unsafe { sys::bs_video_free(self.raw) };
    }
}

/// Converts a path for BestSource, which needs UTF-8 with no NUL byte.
fn c_path(path: &Path) -> Result<CString, BestSourceError> {
    let Some(text) = path.to_str() else {
        let error = non_utf8_path_error(path);
        return Err(error);
    };

    CString::new(text).map_err(|_| nul_path_error(text))
}

fn non_utf8_path_error(path: &Path) -> BestSourceError {
    let display = path.display();
    let message = format!("{display} is not valid utf-8, which bestsource requires");

    BestSourceError::new(message)
}

fn nul_path_error(text: &str) -> BestSourceError {
    let printable = text.replace('\0', "\\0");
    let message = format!("{printable} carries a nul byte, which bestsource cannot take");

    BestSourceError::new(message)
}
