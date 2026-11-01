use std::ptr;

use crate::error::{BestSourceError, ERROR_BUFFER_LEN};
use crate::format::VideoFormat;
use crate::sys;

/// One writable plane for [Frame::export_planar](crate::Frame::export_planar).
pub struct PlaneTarget<'a> {
    pub data: &'a mut [u8],
    /// Bytes from the start of one row to the start of the next.
    pub stride: usize,
}

/// The planes a frame is exported into.
///
/// `planes` holds one plane per colour plane of the frame's format, in BestSource's order. `alpha`
/// is only written when the frame has alpha.
pub struct PlanarTarget<'a> {
    pub planes: Vec<PlaneTarget<'a>>,
    pub alpha: Option<PlaneTarget<'a>>,
}

/// One decoded frame, which stays valid after its source is dropped.
#[derive(Debug)]
pub struct Frame {
    raw: *mut sys::BsFrame,
    props: sys::BsFrameProps,
}

// SAFETY: A BestVideoFrame owns its pixels and is not tied to the thread that decoded it. It is not
// safe to share, so it is not `Sync`.
unsafe impl Send for Frame {}

impl Frame {
    /// Takes ownership of a frame the shim handed out.
    ///
    /// # Safety
    ///
    /// `raw` must be a live frame from `bs_video_frame` that nothing else frees.
    pub(crate) unsafe fn from_raw(raw: *mut sys::BsFrame) -> Self {
        let mut props = sys::BsFrameProps::default();
        // SAFETY: `raw` is live per this function's contract and `props` is a valid out pointer.
        unsafe { sys::bs_frame_info(raw, &raw mut props) };

        Self { raw, props }
    }

    pub fn format(&self) -> VideoFormat {
        VideoFormat::from_raw(&self.props.format)
    }

    pub fn width(&self) -> i32 {
        self.props.width
    }

    pub fn height(&self) -> i32 {
        self.props.height
    }

    /// The width [export_planar](crate::Frame::export_planar) writes, a whole multiple of the chroma
    /// subsampling.
    pub fn ss_mod_width(&self) -> i32 {
        self.props.ss_mod_width
    }

    /// The height [export_planar](crate::Frame::export_planar) writes, a whole multiple of the
    /// chroma subsampling.
    pub fn ss_mod_height(&self) -> i32 {
        self.props.ss_mod_height
    }

    /// The H.273 matrix coefficients.
    pub fn matrix(&self) -> i32 {
        self.props.matrix
    }

    /// The H.273 colour primaries.
    pub fn primaries(&self) -> i32 {
        self.props.primaries
    }

    /// The H.273 transfer characteristics.
    pub fn transfer(&self) -> i32 {
        self.props.transfer
    }

    /// The FFmpeg chroma location.
    pub fn chroma_location(&self) -> i32 {
        self.props.chroma_location
    }

    /// The FFmpeg colour range, `1` for limited and `2` for full.
    pub fn range(&self) -> i32 {
        self.props.color_range
    }

    pub fn pts(&self) -> i64 {
        self.props.pts
    }

    pub fn duration(&self) -> i64 {
        self.props.duration
    }

    pub fn key_frame(&self) -> bool {
        self.props.key_frame != 0
    }

    pub fn interlaced(&self) -> bool {
        self.props.interlaced != 0
    }

    pub fn top_field_first(&self) -> bool {
        self.props.top_field_first != 0
    }

    /// Copies the frame into `target` as planar samples at its subsampling multiple size.
    ///
    /// Every plane is checked against the frame before BestSource writes to it.
    pub fn export_planar(&self, target: &mut PlanarTarget) -> Result<(), BestSourceError> {
        let format = self.format();
        self.check_target(&format, target)?;

        let mut planes = [ptr::null_mut(); 3];
        let mut strides = [0isize; 3];
        for (index, plane) in target.planes.iter_mut().enumerate() {
            planes[index] = plane.data.as_mut_ptr();
            strides[index] = stride_to_isize(plane.stride)?;
        }

        let (alpha, alpha_stride) = match target.alpha.as_mut() {
            Some(plane) if format.alpha => {
                let stride = stride_to_isize(plane.stride)?;
                (plane.data.as_mut_ptr(), stride)
            },
            _ => (ptr::null_mut(), 0),
        };

        let planes_pointer = planes.as_ptr();
        let strides_pointer = strides.as_ptr();
        let mut error = [0u8; ERROR_BUFFER_LEN];
        let error_pointer = error.as_mut_ptr().cast();
        // SAFETY: `check_target` proved every plane, and the alpha plane when it is passed, holds the
        // rows BestSource writes for this frame. The pointers outlive the call and `ERROR_BUFFER_LEN`
        // is the buffer's real size.
        let status = unsafe {
            sys::bs_frame_export_planar(
                self.raw,
                planes_pointer,
                strides_pointer,
                alpha,
                alpha_stride,
                error_pointer,
                ERROR_BUFFER_LEN,
            )
        };
        if status != 0 {
            let error = BestSourceError::from_buffer(&error);
            return Err(error);
        }

        Ok(())
    }

    fn check_target(&self, format: &VideoFormat, target: &PlanarTarget) -> Result<(), BestSourceError> {
        let plane_count = format.plane_count();
        if plane_count == 0 {
            let error = BestSourceError::new("the frame's colour family has no planar layout");
            return Err(error);
        }

        let given_count = target.planes.len();
        if given_count != plane_count {
            let error = plane_count_error(plane_count, given_count);
            return Err(error);
        }

        let width = dimension(self.props.ss_mod_width)?;
        let height = dimension(self.props.ss_mod_height)?;

        for (index, plane) in target.planes.iter().enumerate() {
            let name = format!("plane {index}");
            let extent = format.plane_extent(index > 0, width, height);
            let Some((row_bytes, rows)) = extent else {
                let error = plane_extent_error(&name);
                return Err(error);
            };
            check_plane(plane, row_bytes, rows, &name)?;
        }

        if format.alpha
            && let Some(alpha) = &target.alpha
        {
            let extent = format.plane_extent(false, width, height);
            let Some((row_bytes, rows)) = extent else {
                let error = plane_extent_error("the alpha plane");
                return Err(error);
            };
            check_plane(alpha, row_bytes, rows, "the alpha plane")?;
        }

        Ok(())
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `bs_video_frame` and only this value frees it.
        unsafe { sys::bs_frame_free(self.raw) };
    }
}

fn check_plane(
    plane: &PlaneTarget,
    row_bytes: usize,
    rows: usize,
    name: &str,
) -> Result<(), BestSourceError> {
    if plane.stride < row_bytes {
        let error = narrow_stride_error(name, plane.stride, row_bytes);
        return Err(error);
    }

    let needed = required_len(plane.stride, row_bytes, rows);
    let Some(needed) = needed else {
        let error = plane_too_large_error(name);
        return Err(error);
    };

    let length = plane.data.len();
    if length < needed {
        let error = short_plane_error(name, length, needed);
        return Err(error);
    }

    Ok(())
}

/// The bytes a plane needs, with no padding required after its last row.
fn required_len(stride: usize, row_bytes: usize, rows: usize) -> Option<usize> {
    if rows == 0 {
        return Some(0);
    }

    let leading_rows = stride.checked_mul(rows - 1)?;

    leading_rows.checked_add(row_bytes)
}

fn dimension(value: i32) -> Result<usize, BestSourceError> {
    usize::try_from(value).map_err(|_| negative_size_error(value))
}

fn stride_to_isize(stride: usize) -> Result<isize, BestSourceError> {
    isize::try_from(stride).map_err(|_| large_stride_error(stride))
}

fn negative_size_error(value: i32) -> BestSourceError {
    let message = format!("the frame has a negative size of {value}");
    BestSourceError::new(message)
}

fn large_stride_error(stride: usize) -> BestSourceError {
    let message = format!("a stride of {stride} bytes is too large");
    BestSourceError::new(message)
}

fn plane_too_large_error(name: &str) -> BestSourceError {
    let message = format!("{name} is too large to address");
    BestSourceError::new(message)
}

fn plane_extent_error(name: &str) -> BestSourceError {
    let message = format!("{name} is too large to address or has an invalid subsampling");
    BestSourceError::new(message)
}

fn plane_count_error(expected: usize, given: usize) -> BestSourceError {
    let message = format!("the frame has {expected} planes but {given} were given");
    BestSourceError::new(message)
}

fn narrow_stride_error(name: &str, stride: usize, row_bytes: usize) -> BestSourceError {
    let message = format!("{name} has a stride of {stride} bytes, below its {row_bytes} byte rows");
    BestSourceError::new(message)
}

fn short_plane_error(name: &str, length: usize, needed: usize) -> BestSourceError {
    let message = format!("{name} holds {length} bytes but needs {needed}");
    BestSourceError::new(message)
}
