use std::arch::x86_64::*;

use crate::blocks::scalar_block;
use crate::quantisation::{BLOCK_SIZE, Quantisation};

const TILE_BLOCKS: usize = 8;
const TILE_WIDTH: usize = TILE_BLOCKS * BLOCK_SIZE as usize;

/// Scores every block of a map, eight blocks at a time.
///
/// Blocks that do not fill a whole tile are scored one by one.
///
/// # Safety
///
/// The CPU must support AVX-512F and AVX-512BW.
///
/// # Panics
///
/// Panics if `values` or `blocks` is too short for the map size.
#[target_feature(enable = "avx512f,avx512bw")]
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
        floor: _mm512_set1_ps(scaler.floor),
        cap: _mm512_set1_ps(scaler.cap),
        factor: _mm512_set1_ps(scaler.factor),
        half: _mm512_set1_ps(0.5),
        one: _mm512_set1_epi32(1),
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

/// Scores the eight blocks of a tile whose top left pixel is at `start_row` and `start_column`.
///
/// The eight codes are written into `codes`.
#[target_feature(enable = "avx512f,avx512bw")]
fn tile_codes(
    values: &[f32],
    width: usize,
    start_row: usize,
    start_column: usize,
    scale: &Scale,
    codes: &mut [u8],
) {
    assert_eq!(codes.len(), TILE_BLOCKS);

    let mut maximums = _mm512_setzero_si512();

    for row in start_row..start_row + BLOCK_SIZE as usize {
        let offset = row * width + start_column;
        let pixels = &values[offset..offset + TILE_WIDTH];
        let row_maximums = row_codes(pixels, scale);
        maximums = _mm512_max_epu8(maximums, row_maximums);
    }

    // Each block's 8 codes already sit in one 64-bit group.
    let shifted_by_four = _mm512_srli_epi64::<32>(maximums);
    let halved = _mm512_max_epu8(maximums, shifted_by_four);
    let shifted_by_two = _mm512_srli_epi64::<16>(halved);
    let quartered = _mm512_max_epu8(halved, shifted_by_two);
    let shifted_by_one = _mm512_srli_epi64::<8>(quartered);
    let reduced = _mm512_max_epu8(quartered, shifted_by_one);

    // Each block's code is the low byte of its group, so truncating each group to a byte keeps it.
    let narrowed = _mm512_cvtepi64_epi8(reduced);

    let destination = codes.as_mut_ptr().cast::<__m128i>();
    // SAFETY: `codes` holds 8 bytes, checked by the `assert_eq!` at the top of this function, and
    // the store writes 8 bytes unaligned.
    unsafe { _mm_storel_epi64(destination, narrowed) };
}

struct Scale {
    floor: __m512,
    cap: __m512,
    factor: __m512,
    half: __m512,
    one: __m512i,
}

/// Quantises 64 pixels into 64 codes, kept in pixel order so each 8-byte group is one block.
#[target_feature(enable = "avx512f,avx512bw")]
fn row_codes(pixels: &[f32], scale: &Scale) -> __m512i {
    let first = quantise_sixteen(&pixels[0..16], scale);
    let second = quantise_sixteen(&pixels[16..32], scale);
    let third = quantise_sixteen(&pixels[32..48], scale);
    let fourth = quantise_sixteen(&pixels[48..64], scale);

    let first_bytes = _mm512_castsi128_si512(first);
    let with_second = _mm512_inserti32x4::<1>(first_bytes, second);
    let with_third = _mm512_inserti32x4::<2>(with_second, third);
    _mm512_inserti32x4::<3>(with_third, fourth)
}

/// Quantises 16 pixels into 16 codes held as bytes.
///
/// Rounds halves away from zero by truncating, then adding 1 when the dropped fraction is at least
/// a half. That matches [f32::round], which a plain conversion does not.
#[target_feature(enable = "avx512f,avx512bw")]
fn quantise_sixteen(pixels: &[f32], scale: &Scale) -> __m128i {
    assert_eq!(pixels.len(), 16);

    // SAFETY: `pixels` holds 16 floats, checked by the `assert_eq!` at the top of this function, and
    // the load is unaligned.
    let value = unsafe { _mm512_loadu_ps(pixels.as_ptr()) };

    let at_or_above_floor = _mm512_cmp_ps_mask::<_CMP_GE_OQ>(value, scale.floor);
    let clamped = _mm512_min_ps(value, scale.cap);
    let scaled = _mm512_mul_ps(clamped, scale.factor);

    let truncated = _mm512_cvttps_epi32(scaled);
    let truncated_float = _mm512_cvtepi32_ps(truncated);
    let fraction = _mm512_sub_ps(scaled, truncated_float);
    let round_up = _mm512_cmp_ps_mask::<_CMP_GE_OQ>(fraction, scale.half);

    let rounded = _mm512_mask_add_epi32(truncated, round_up, truncated, scale.one);
    let kept = _mm512_maskz_mov_epi32(at_or_above_floor, rounded);

    // Codes never exceed `top`, a `u8`, so the saturating narrow never clips.
    _mm512_cvtusepi32_epi8(kept)
}
