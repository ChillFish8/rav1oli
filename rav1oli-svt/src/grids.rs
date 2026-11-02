use std::mem;

use crate::error::{GridError, GridKind};
use crate::sys;

/// The largest qindex drop SVT accepts for a 64x64 superblock.
pub const MAX_DROP: u8 = 80;
/// The largest lambda cut SVT accepts for an 8x8 block, in 1/256 steps.
pub const MAX_CUT: u8 = 230;

const HEADER_BYTES: usize = mem::size_of::<sys::SvtAv1DistortionMap>();

/// The per-picture distortion map, as a qindex drop per 64x64 superblock and a lambda cut per 8x8 block.
///
/// Both grids are row-major and cover the frame, rounding partial blocks up.
#[derive(Debug, Clone)]
pub struct DistortionGrids {
    width: u32,
    height: u32,
    drops: Vec<u8>,
    cuts: Vec<u8>,
}

impl DistortionGrids {
    /// Checks both grids against the frame size and their value ranges.
    pub fn new(width: u32, height: u32, drops: Vec<u8>, cuts: Vec<u8>) -> Result<Self, GridError> {
        let grids = Self {
            width,
            height,
            drops,
            cuts,
        };

        grids.check(GridKind::Drops, &grids.drops, MAX_DROP)?;
        grids.check(GridKind::Cuts, &grids.cuts, MAX_CUT)?;

        Ok(grids)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn drop_columns(&self) -> usize {
        cells_across(self.width, 64)
    }

    pub fn drop_rows(&self) -> usize {
        cells_across(self.height, 64)
    }

    pub fn cut_columns(&self) -> usize {
        cells_across(self.width, 8)
    }

    pub fn cut_rows(&self) -> usize {
        cells_across(self.height, 8)
    }

    /// Writes the `SvtAv1DistortionMap` block into `payload` and returns its length in bytes.
    ///
    /// The block is built in `u32` words so the header SVT reads in place is aligned.
    pub(crate) fn write_payload(&self, payload: &mut Vec<u32>) -> usize {
        let header = sys::SvtAv1DistortionMap {
            version: sys::SVT_AV1_DISTORTION_MAP_VERSION,
            b64_columns: self.drop_columns() as u32,
            b64_rows: self.drop_rows() as u32,
            columns: self.cut_columns() as u32,
            rows: self.cut_rows() as u32,
        };

        let byte_len = HEADER_BYTES + self.drops.len() + self.cuts.len();
        let word_len = byte_len.div_ceil(mem::size_of::<u32>());
        payload.clear();
        payload.resize(word_len, 0);

        let words = payload.as_mut_ptr();
        // SAFETY: The buffer holds at least `byte_len` bytes, and any byte pattern is a valid `u8`.
        let bytes = unsafe { std::slice::from_raw_parts_mut(words.cast::<u8>(), byte_len) };

        let (header_bytes, grid_bytes) = bytes.split_at_mut(HEADER_BYTES);
        let (drop_bytes, cut_bytes) = grid_bytes.split_at_mut(self.drops.len());
        drop_bytes.copy_from_slice(&self.drops);
        cut_bytes.copy_from_slice(&self.cuts);

        let header_pointer = header_bytes.as_mut_ptr().cast::<sys::SvtAv1DistortionMap>();
        // SAFETY: The header sits at the start of a `u32` buffer, which meets its alignment, and fits.
        unsafe { header_pointer.write(header) };

        byte_len
    }

    fn check(&self, grid: GridKind, cells: &[u8], max: u8) -> Result<(), GridError> {
        let (columns, rows) = match grid {
            GridKind::Drops => (self.drop_columns(), self.drop_rows()),
            GridKind::Cuts => (self.cut_columns(), self.cut_rows()),
        };

        let expected = columns * rows;
        if cells.len() != expected {
            return Err(GridError::Size {
                grid,
                width: self.width,
                height: self.height,
                columns,
                rows,
                expected,
                actual: cells.len(),
            });
        }

        let first_above = cells.iter().position(|&value| value > max);
        if let Some(index) = first_above {
            return Err(GridError::OutOfRange {
                grid,
                row: index / columns,
                column: index % columns,
                value: cells[index],
                max,
            });
        }

        Ok(())
    }
}

fn cells_across(length: u32, block: u32) -> usize {
    length.div_ceil(block) as usize
}
