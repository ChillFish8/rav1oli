use anyhow::{Context, bail};
use rav1oli_svt::DistortionGrids;
use rav1oli_table::SourceInfo;

use crate::tables::Lookups;

/// The share of a superblock's blocks that its drop must cover.
pub const DROP_PERCENTILE: f64 = 0.9;

const BLOCK_SIZE: u32 = rav1oli_table::BLOCK_SIZE;
const BLOCKS_PER_SUPERBLOCK: usize = 8;

/// Turns each map frame's scores into the grids SVT-AV1 reads.
pub struct GridBuilder {
    lookups: Lookups,
    width: u32,
    height: u32,
    grid_width: usize,
    grid_height: usize,
    scratch: Vec<u8>,
}

impl GridBuilder {
    /// Checks the map's 8x8 grid matches the grid SVT expects for the map's size.
    pub fn new(lookups: Lookups, source: &SourceInfo) -> anyhow::Result<GridBuilder> {
        let expected_width = source.width.div_ceil(BLOCK_SIZE);
        let expected_height = source.height.div_ceil(BLOCK_SIZE);

        Self::with_expected_grid(lookups, source, expected_width, expected_height)
    }

    // Split out of `new` so the check can be tested with a grid that disagrees.
    pub(super) fn with_expected_grid(
        lookups: Lookups,
        source: &SourceInfo,
        expected_width: u32,
        expected_height: u32,
    ) -> anyhow::Result<GridBuilder> {
        let grid_width = source.grid_width();
        let grid_height = source.grid_height();
        if grid_width != expected_width || grid_height != expected_height {
            bail!(
                "the map's block grid is {grid_width}x{grid_height} but SVT expects \
                 {expected_width}x{expected_height} for a {}x{} frame",
                source.width,
                source.height,
            );
        }

        Ok(GridBuilder {
            lookups,
            width: source.width,
            height: source.height,
            grid_width: grid_width as usize,
            grid_height: grid_height as usize,
            scratch: Vec::with_capacity(BLOCKS_PER_SUPERBLOCK * BLOCKS_PER_SUPERBLOCK),
        })
    }

    /// Builds the grids for one frame's scores, laid out row by row.
    ///
    /// Each superblock's drop is the nearest-rank percentile of its blocks' drops.
    /// Superblocks at the frame edge only use the blocks inside the frame.
    pub fn build(&mut self, scores: &[u8]) -> anyhow::Result<DistortionGrids> {
        let expected = self.grid_width * self.grid_height;
        if scores.len() != expected {
            bail!(
                "the frame has {} scores but a {}x{} block grid needs {expected}",
                scores.len(),
                self.grid_width,
                self.grid_height,
            );
        }

        let cuts: Vec<u8> = scores
            .iter()
            .map(|&score| self.lookups.cuts[usize::from(score)])
            .collect();

        let superblock_columns = self.grid_width.div_ceil(BLOCKS_PER_SUPERBLOCK);
        let superblock_rows = self.grid_height.div_ceil(BLOCKS_PER_SUPERBLOCK);
        let mut drops = Vec::with_capacity(superblock_columns * superblock_rows);
        for superblock_y in 0..superblock_rows {
            for superblock_x in 0..superblock_columns {
                let drop = self.superblock_drop(scores, superblock_x, superblock_y);
                drops.push(drop);
            }
        }

        let grids = DistortionGrids::new(self.width, self.height, drops, cuts);

        grids.context("the built grids were rejected")
    }

    fn superblock_drop(&mut self, scores: &[u8], superblock_x: usize, superblock_y: usize) -> u8 {
        let first_column = superblock_x * BLOCKS_PER_SUPERBLOCK;
        let end_column = (first_column + BLOCKS_PER_SUPERBLOCK).min(self.grid_width);
        let first_row = superblock_y * BLOCKS_PER_SUPERBLOCK;
        let end_row = (first_row + BLOCKS_PER_SUPERBLOCK).min(self.grid_height);

        self.scratch.clear();
        for row in first_row..end_row {
            let row_start = row * self.grid_width;
            let row_scores = &scores[row_start + first_column..row_start + end_column];
            let row_drops = row_scores
                .iter()
                .map(|&score| self.lookups.drops[usize::from(score)]);
            self.scratch.extend(row_drops);
        }

        let count = self.scratch.len();
        let rank = (DROP_PERCENTILE * count as f64).ceil() as usize;
        let index = rank.saturating_sub(1);
        let (_, drop, _) = self.scratch.select_nth_unstable(index);

        *drop
    }
}

/// Errors naming both indices when the map frame is not the source frame.
pub fn check_frame_index(map_index: u32, source_index: usize) -> anyhow::Result<()> {
    if map_index as usize != source_index {
        bail!("the map's frame {map_index} does not match source frame {source_index}");
    }

    Ok(())
}
