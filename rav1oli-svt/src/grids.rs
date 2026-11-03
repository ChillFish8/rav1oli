use std::mem;

use crate::error::{GridError, GridKind};
use crate::sys;

/// The largest qindex drop SVT accepts for a 64x64 superblock in additive mode.
pub const MAX_DROP: u8 = 80;
/// The largest lambda cut SVT accepts for an 8x8 block in additive mode, in 1/256 steps.
pub const MAX_CUT: u8 = 230;
/// The smallest qindex offset SVT accepts for a 64x64 superblock in redistribute mode.
pub const MIN_OFFSET: i8 = -80;
/// The largest qindex offset SVT accepts for a 64x64 superblock in redistribute mode.
pub const MAX_OFFSET: i8 = 40;

const HEADER_BYTES: usize = mem::size_of::<sys::SvtAv1DistortionMap>();

/// How SVT reads a distortion map's values.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MapMode {
    /// Only adds bits, through a qindex drop per superblock and a lambda cut per block.
    Additive,
    /// Moves bits within each frame, through a signed qindex offset per superblock and a signed log2
    /// lambda weight per block, negative for more bits.
    Redistribute,
}

impl MapMode {
    /// The value of SVT's `distortion_map_mode` setting.
    pub(crate) fn svt_value(self) -> u8 {
        match self {
            MapMode::Additive => 0,
            MapMode::Redistribute => 1,
        }
    }

    pub(crate) fn from_svt_value(value: u8) -> Option<MapMode> {
        match value {
            0 => Some(MapMode::Additive),
            1 => Some(MapMode::Redistribute),
            _ => None,
        }
    }
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            MapMode::Additive => "additive",
            MapMode::Redistribute => "redistribute",
        };

        formatter.write_str(name)
    }
}

#[derive(Debug, Clone)]
enum Cells {
    Additive { drops: Vec<u8>, cuts: Vec<u8> },
    Redistribute { offsets: Vec<i8>, weights: Vec<i8> },
}

/// The per-picture distortion map, as one value per 64x64 superblock and one per 8x8 block.
///
/// Both grids are row-major and cover the frame, rounding partial blocks up.
#[derive(Debug, Clone)]
pub struct DistortionGrids {
    width: u32,
    height: u32,
    cells: Cells,
}

impl DistortionGrids {
    /// Checks additive grids, a drop per superblock and a cut per block, against the frame size and ranges.
    pub fn additive(width: u32, height: u32, drops: Vec<u8>, cuts: Vec<u8>) -> Result<Self, GridError> {
        let grids = Self {
            width,
            height,
            cells: Cells::Additive { drops, cuts },
        };

        if let Cells::Additive { drops, cuts } = &grids.cells {
            grids.check(GridKind::Drops, drops, 0, MAX_DROP)?;
            grids.check(GridKind::Cuts, cuts, 0, MAX_CUT)?;
        }

        Ok(grids)
    }

    /// Checks redistribute grids, an offset per superblock and a weight per block, against the frame size
    /// and ranges.
    ///
    /// Every weight is in range, as SVT recentres and clamps them itself.
    pub fn redistribute(
        width: u32,
        height: u32,
        offsets: Vec<i8>,
        weights: Vec<i8>,
    ) -> Result<Self, GridError> {
        let grids = Self {
            width,
            height,
            cells: Cells::Redistribute { offsets, weights },
        };

        if let Cells::Redistribute { offsets, weights } = &grids.cells {
            grids.check(GridKind::Offsets, offsets, MIN_OFFSET, MAX_OFFSET)?;
            grids.check(GridKind::Weights, weights, i8::MIN, i8::MAX)?;
        }

        Ok(grids)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn mode(&self) -> MapMode {
        match self.cells {
            Cells::Additive { .. } => MapMode::Additive,
            Cells::Redistribute { .. } => MapMode::Redistribute,
        }
    }

    /// The superblock drops of additive grids.
    pub fn drops(&self) -> Option<&[u8]> {
        match &self.cells {
            Cells::Additive { drops, .. } => Some(drops),
            Cells::Redistribute { .. } => None,
        }
    }

    /// The block cuts of additive grids.
    pub fn cuts(&self) -> Option<&[u8]> {
        match &self.cells {
            Cells::Additive { cuts, .. } => Some(cuts),
            Cells::Redistribute { .. } => None,
        }
    }

    /// The superblock offsets of redistribute grids.
    pub fn offsets(&self) -> Option<&[i8]> {
        match &self.cells {
            Cells::Additive { .. } => None,
            Cells::Redistribute { offsets, .. } => Some(offsets),
        }
    }

    /// The block weights of redistribute grids.
    pub fn weights(&self) -> Option<&[i8]> {
        match &self.cells {
            Cells::Additive { .. } => None,
            Cells::Redistribute { weights, .. } => Some(weights),
        }
    }

    pub fn superblock_columns(&self) -> usize {
        cells_across(self.width, 64)
    }

    pub fn superblock_rows(&self) -> usize {
        cells_across(self.height, 64)
    }

    pub fn block_columns(&self) -> usize {
        cells_across(self.width, 8)
    }

    pub fn block_rows(&self) -> usize {
        cells_across(self.height, 8)
    }

    /// Writes the `SvtAv1DistortionMap` block into `payload` and returns its length in bytes.
    ///
    /// The block is built in `u32` words so the header SVT reads in place is aligned.
    pub(crate) fn write_payload(&self, payload: &mut Vec<u32>) -> usize {
        let header = sys::SvtAv1DistortionMap {
            version: sys::SVT_AV1_DISTORTION_MAP_VERSION,
            b64_columns: self.superblock_columns() as u32,
            b64_rows: self.superblock_rows() as u32,
            columns: self.block_columns() as u32,
            rows: self.block_rows() as u32,
        };

        let superblock_cells = self.superblock_columns() * self.superblock_rows();
        let block_cells = self.block_columns() * self.block_rows();
        let byte_len = HEADER_BYTES + superblock_cells + block_cells;
        let word_len = byte_len.div_ceil(mem::size_of::<u32>());
        payload.clear();
        payload.resize(word_len, 0);

        let words = payload.as_mut_ptr();
        // SAFETY: The buffer holds at least `byte_len` bytes, and any byte pattern is a valid `u8`.
        let bytes = unsafe { std::slice::from_raw_parts_mut(words.cast::<u8>(), byte_len) };

        let (header_bytes, grid_bytes) = bytes.split_at_mut(HEADER_BYTES);
        let (superblock_bytes, block_bytes) = grid_bytes.split_at_mut(superblock_cells);
        match &self.cells {
            Cells::Additive { drops, cuts } => {
                superblock_bytes.copy_from_slice(drops);
                block_bytes.copy_from_slice(cuts);
            },
            Cells::Redistribute { offsets, weights } => {
                copy_signed(superblock_bytes, offsets);
                copy_signed(block_bytes, weights);
            },
        }

        let header_pointer = header_bytes.as_mut_ptr().cast::<sys::SvtAv1DistortionMap>();
        // SAFETY: The header sits at the start of a `u32` buffer, which meets its alignment, and fits.
        unsafe { header_pointer.write(header) };

        byte_len
    }

    fn check<T>(&self, grid: GridKind, cells: &[T], min: T, max: T) -> Result<(), GridError>
    where
        T: Copy + PartialOrd + Into<i16>,
    {
        let (columns, rows) = match grid {
            GridKind::Drops | GridKind::Offsets => (self.superblock_columns(), self.superblock_rows()),
            GridKind::Cuts | GridKind::Weights => (self.block_columns(), self.block_rows()),
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

        let first_outside = cells.iter().position(|&value| value < min || value > max);
        if let Some(index) = first_outside {
            return Err(GridError::OutOfRange {
                grid,
                row: index / columns,
                column: index % columns,
                value: cells[index].into(),
                min: min.into(),
                max: max.into(),
            });
        }

        Ok(())
    }
}

/// Copies signed cells as the `int8_t` bytes SVT reads.
fn copy_signed(bytes: &mut [u8], cells: &[i8]) {
    for (byte, cell) in bytes.iter_mut().zip(cells) {
        *byte = cell.cast_unsigned();
    }
}

fn cells_across(length: u32, block: u32) -> usize {
    length.div_ceil(block) as usize
}
