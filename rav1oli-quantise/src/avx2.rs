use std::arch::x86_64::*;

use crate::blocks::scalar_block;
use crate::quantisation::{BLOCK_SIZE, Quantisation};

const TILE_BLOCKS: usize = 4;
const TILE_WIDTH: usize = TILE_BLOCKS * BLOCK_SIZE as usize;

/// Scores every block of a map, four blocks at a time.
///
/// Blocks that do not fill a whole tile are scored one by one.
///
/// # Safety
///
/// The CPU must support AVX2.
///
/// # Panics
///
/// Panics if `values` or `blocks` is too short for the map size.
#[target_feature(enable = "avx2")]
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
        floor: _mm256_set1_ps(scaler.floor),
        cap: _mm256_set1_ps(scaler.cap),
        factor: _mm256_set1_ps(scaler.factor),
        half: _mm256_set1_ps(0.5),
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

/// Scores the four blocks of a tile whose top left pixel is at `start_row` and `start_column`.
///
/// The four codes are written into `codes`.
#[target_feature(enable = "avx2")]
fn tile_codes(
    values: &[f32],
    width: usize,
    start_row: usize,
    start_column: usize,
    scale: &Scale,
    codes: &mut [u8],
) {
    assert_eq!(codes.len(), TILE_BLOCKS);

    let mut maximums = _mm256_setzero_si256();

    for row in start_row..start_row + BLOCK_SIZE as usize {
        let offset = row * width + start_column;
        let pixels = &values[offset..offset + TILE_WIDTH];
        let row_maximums = row_codes(pixels, scale);
        maximums = _mm256_max_epu8(maximums, row_maximums);
    }

    // Each block's 8 codes sit in one 64-bit group after the reordering.
    let order = _mm256_set_epi32(7, 3, 6, 2, 5, 1, 4, 0);
    let ordered = _mm256_permutevar8x32_epi32(maximums, order);

    let shifted_by_four = _mm256_srli_epi64::<32>(ordered);
    let halved = _mm256_max_epu8(ordered, shifted_by_four);
    let shifted_by_two = _mm256_srli_epi64::<16>(halved);
    let quartered = _mm256_max_epu8(halved, shifted_by_two);
    let shifted_by_one = _mm256_srli_epi64::<8>(quartered);
    let reduced = _mm256_max_epu8(quartered, shifted_by_one);

    // Each block's code is byte 0 of its group. This moves the two codes in each 128-bit lane to
    // the bottom of that lane, then joins the two lanes so all four codes sit in the low 4 bytes.
    let pick = _mm256_setr_epi8(
        0, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, //
        0, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    );
    let picked = _mm256_shuffle_epi8(reduced, pick);
    let low_lane = _mm256_castsi256_si128(picked);
    let high_lane = _mm256_extracti128_si256::<1>(picked);
    let joined = _mm_unpacklo_epi16(low_lane, high_lane);

    let destination = codes.as_mut_ptr();
    // SAFETY: `codes` holds 4 bytes, checked by the `assert_eq!` at the top of this function, and
    // the store is unaligned.
    unsafe { _mm_storeu_si32(destination, joined) };
}

struct Scale {
    floor: __m256,
    cap: __m256,
    factor: __m256,
    half: __m256,
}

/// Quantises 32 pixels into 32 codes.
///
/// The packs work within each 128-bit lane, so the codes come out in groups of 4 that are not in
/// pixel order.
#[target_feature(enable = "avx2")]
fn row_codes(pixels: &[f32], scale: &Scale) -> __m256i {
    let first = quantise_eight(&pixels[0..8], scale);
    let second = quantise_eight(&pixels[8..16], scale);
    let third = quantise_eight(&pixels[16..24], scale);
    let fourth = quantise_eight(&pixels[24..32], scale);

    let first_pair = _mm256_packus_epi32(first, second);
    let second_pair = _mm256_packus_epi32(third, fourth);
    _mm256_packus_epi16(first_pair, second_pair)
}

/// Quantises 8 pixels into 8 codes held as 32-bit integers.
///
/// Rounds halves away from zero by truncating, then adding 1 when the dropped fraction is at least
/// a half. That matches [f32::round], which a plain conversion does not.
#[target_feature(enable = "avx2")]
fn quantise_eight(pixels: &[f32], scale: &Scale) -> __m256i {
    assert_eq!(pixels.len(), 8);

    // SAFETY: `pixels` holds 8 floats, checked by the `assert_eq!` at the top of this function, and
    // the load is unaligned.
    let value = unsafe { _mm256_loadu_ps(pixels.as_ptr()) };

    let at_or_above_floor = _mm256_cmp_ps::<_CMP_GE_OQ>(value, scale.floor);
    let clamped = _mm256_min_ps(value, scale.cap);
    let scaled = _mm256_mul_ps(clamped, scale.factor);

    let truncated = _mm256_cvttps_epi32(scaled);
    let truncated_float = _mm256_cvtepi32_ps(truncated);
    let fraction = _mm256_sub_ps(scaled, truncated_float);
    let round_up = _mm256_cmp_ps::<_CMP_GE_OQ>(fraction, scale.half);

    // A true compare is all ones, which is -1, so subtracting it adds 1.
    let round_up_bits = _mm256_castps_si256(round_up);
    let keep_bits = _mm256_castps_si256(at_or_above_floor);
    let rounded = _mm256_sub_epi32(truncated, round_up_bits);
    _mm256_and_si256(rounded, keep_bits)
}
