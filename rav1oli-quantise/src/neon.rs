use std::arch::aarch64::*;

use crate::blocks::scalar_block;
use crate::quantisation::{BLOCK_SIZE, Quantisation};

const TILE_BLOCKS: usize = 2;
const TILE_WIDTH: usize = TILE_BLOCKS * BLOCK_SIZE as usize;

/// Scores every block of a map, two blocks at a time.
///
/// Blocks that do not fill a whole tile are scored one by one.
///
/// # Safety
///
/// The CPU must support NEON.
///
/// # Panics
///
/// Panics if `values` or `blocks` is too short for the map size.
#[target_feature(enable = "neon")]
pub(crate) unsafe fn block_scores(
    values: &[f32],
    width: usize,
    height: usize,
    quantisation: &Quantisation,
    blocks: &mut [u8],
) {
    debug_assert!(quantisation.check().is_ok());

    let block_size = BLOCK_SIZE as usize;
    let blocks_wide = width.div_ceil(block_size);
    let blocks_high = height.div_ceil(block_size);
    debug_assert_eq!(values.len(), width * height);
    debug_assert_eq!(blocks.len(), blocks_wide * blocks_high);

    let scaler = quantisation.scaler();
    let scale = Scale {
        floor: vdupq_n_f32(scaler.floor),
        cap: vdupq_n_f32(scaler.cap),
        factor: vdupq_n_f32(scaler.factor),
    };
    let full_bands = height / block_size;
    let tiles = width / TILE_WIDTH;

    for block_row in 0..blocks_high {
        let mut first_scalar_block = 0;

        if block_row < full_bands {
            let start_row = block_row * block_size;

            for tile in 0..tiles {
                let column = tile * TILE_WIDTH;
                let first_block = block_row * blocks_wide + tile * TILE_BLOCKS;
                let codes = &mut blocks[first_block..first_block + TILE_BLOCKS];
                tile_codes(values, width, start_row, column, &scale, codes);
            }

            first_scalar_block = tiles * TILE_BLOCKS;
        }

        for block_column in first_scalar_block..blocks_wide {
            let code = scalar_block(values, width, height, block_column, block_row, &scaler);
            blocks[block_row * blocks_wide + block_column] = code;
        }
    }
}

struct Scale {
    floor: float32x4_t,
    cap: float32x4_t,
    factor: float32x4_t,
}

/// Scores the two blocks of a tile whose top left pixel is at `start_row` and `start_column`.
///
/// The two codes are written into `codes`.
#[target_feature(enable = "neon")]
fn tile_codes(
    values: &[f32],
    width: usize,
    start_row: usize,
    start_column: usize,
    scale: &Scale,
    codes: &mut [u8],
) {
    assert_eq!(codes.len(), TILE_BLOCKS);

    let mut maximums = vdupq_n_u8(0);

    for row in start_row..start_row + BLOCK_SIZE as usize {
        let offset = row * width + start_column;
        let pixels = &values[offset..offset + TILE_WIDTH];
        let row_maximums = row_codes(pixels, scale);
        maximums = vmaxq_u8(maximums, row_maximums);
    }

    // The low half holds the first block's 8 columns and the high half the second block's.
    let first_block = vget_low_u8(maximums);
    let second_block = vget_high_u8(maximums);

    // Each step halves the bytes left in each block, leaving the first block in lane 0 and the second
    // block in lane 1.
    let halved = vpmax_u8(first_block, second_block);
    let quartered = vpmax_u8(halved, halved);
    let reduced = vpmax_u8(quartered, quartered);

    // Lanes 0 and 1 together form the low 16-bit lane.
    let pairs = vreinterpret_u16_u8(reduced);
    let both_codes = vget_lane_u16::<0>(pairs);

    let destination = codes.as_mut_ptr().cast::<u16>();
    // SAFETY: `codes` holds 2 bytes, checked by the `assert_eq!` at the top of this function, and
    // the write does not need alignment.
    unsafe { destination.write_unaligned(both_codes) };
}

/// Quantises 16 pixels into 16 codes, in the same order as the pixels.
#[target_feature(enable = "neon")]
fn row_codes(pixels: &[f32], scale: &Scale) -> uint8x16_t {
    let first = quantise_four(&pixels[0..4], scale);
    let second = quantise_four(&pixels[4..8], scale);
    let third = quantise_four(&pixels[8..12], scale);
    let fourth = quantise_four(&pixels[12..16], scale);

    // Codes are at most 255, so the saturating narrows never clip.
    let first_narrow = vqmovn_u32(first);
    let second_narrow = vqmovn_u32(second);
    let third_narrow = vqmovn_u32(third);
    let fourth_narrow = vqmovn_u32(fourth);
    let low_pair = vcombine_u16(first_narrow, second_narrow);
    let high_pair = vcombine_u16(third_narrow, fourth_narrow);

    let low_bytes = vqmovn_u16(low_pair);
    let high_bytes = vqmovn_u16(high_pair);
    vcombine_u8(low_bytes, high_bytes)
}

/// Quantises 4 pixels into 4 codes held as 32-bit integers.
///
/// The conversion rounds halves away from zero, which matches [f32::round].
#[target_feature(enable = "neon")]
fn quantise_four(pixels: &[f32], scale: &Scale) -> uint32x4_t {
    assert_eq!(pixels.len(), 4);

    // SAFETY: `pixels` holds 4 floats, checked by the `assert_eq!` at the top of this function.
    let value = unsafe { vld1q_f32(pixels.as_ptr()) };

    // The compare is false for NaN, so NaN scores 0.
    let at_or_above_floor = vcgeq_f32(value, scale.floor);
    let clamped = vminq_f32(value, scale.cap);
    let scaled = vmulq_f32(clamped, scale.factor);

    let rounded = vcvtaq_u32_f32(scaled);
    vandq_u32(rounded, at_or_above_floor)
}
