use crate::quantisation::{BLOCK_SIZE, Quantisation, Scaler};

/// Reduces a packed `width * height` metric map to one code per block.
///
/// Each block holds the highest value inside it, ignoring NaN. Blocks on the right and bottom
/// edges only use the pixels that exist. Codes are returned row by row.
///
/// # Panics
///
/// Panics if `values` does not hold exactly `width * height` values.
pub fn block_scores(values: &[f32], width: usize, height: usize, quantisation: &Quantisation) -> Vec<u8> {
    let pixels = width
        .checked_mul(height)
        .expect("the map size must not overflow usize");
    assert_eq!(values.len(), pixels);

    dispatch_block_scores(values, width, height, quantisation)
}

#[cfg(target_arch = "x86_64")]
fn dispatch_block_scores(
    values: &[f32],
    width: usize,
    height: usize,
    quantisation: &Quantisation,
) -> Vec<u8> {
    let has_avx512 = is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("avx512bw");
    let has_avx2 = is_x86_feature_detected!("avx2");
    if !has_avx512 && !has_avx2 {
        return scalar_block_scores(values, width, height, quantisation);
    }

    let block_size = BLOCK_SIZE as usize;
    let blocks_wide = width.div_ceil(block_size);
    let blocks_high = height.div_ceil(block_size);
    let mut blocks = vec![0; blocks_wide * blocks_high];

    if has_avx512 {
        // SAFETY: AVX-512F and AVX-512BW were just detected, and the lengths match the map size.
        unsafe { crate::avx512::block_scores(values, width, height, quantisation, &mut blocks) };
    } else {
        // SAFETY: AVX2 was just detected, and the lengths match the map size.
        unsafe { crate::avx2::block_scores(values, width, height, quantisation, &mut blocks) };
    }

    blocks
}

#[cfg(target_arch = "aarch64")]
fn dispatch_block_scores(
    values: &[f32],
    width: usize,
    height: usize,
    quantisation: &Quantisation,
) -> Vec<u8> {
    let block_size = BLOCK_SIZE as usize;
    let blocks_wide = width.div_ceil(block_size);
    let blocks_high = height.div_ceil(block_size);
    let mut blocks = vec![0; blocks_wide * blocks_high];

    // SAFETY: NEON is part of every aarch64 target, and the lengths match the map size.
    unsafe { crate::neon::block_scores(values, width, height, quantisation, &mut blocks) };

    blocks
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn dispatch_block_scores(
    values: &[f32],
    width: usize,
    height: usize,
    quantisation: &Quantisation,
) -> Vec<u8> {
    scalar_block_scores(values, width, height, quantisation)
}

#[cfg_attr(
    all(not(test), target_arch = "aarch64"),
    expect(
        dead_code,
        reason = "NEON handles every map, so only the tests call it on this architecture"
    )
)]
pub(crate) fn scalar_block_scores(
    values: &[f32],
    width: usize,
    height: usize,
    quantisation: &Quantisation,
) -> Vec<u8> {
    debug_assert_eq!(values.len(), width * height);

    if width == 0 || height == 0 {
        return Vec::new();
    }

    let block_size = BLOCK_SIZE as usize;
    let blocks_wide = width.div_ceil(block_size);
    let blocks_high = height.div_ceil(block_size);
    let mut maximums = vec![f32::NEG_INFINITY; blocks_wide * blocks_high];

    for (row_index, row) in values.chunks_exact(width).enumerate() {
        let block_row = row_index / block_size;

        for (block_column, pixels) in row.chunks(block_size).enumerate() {
            let slot = &mut maximums[block_row * blocks_wide + block_column];
            let row_maximum = pixels.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            *slot = slot.max(row_maximum);
        }
    }

    let scaler = quantisation.scaler();
    maximums
        .into_iter()
        .map(|maximum| scaler.quantise(maximum))
        .collect()
}

/// Returns the code for one block, using only the pixels that exist.
#[cfg_attr(
    all(not(test), not(any(target_arch = "x86_64", target_arch = "aarch64"))),
    expect(dead_code, reason = "only the tests call it on this architecture")
)]
pub(crate) fn scalar_block(
    values: &[f32],
    width: usize,
    height: usize,
    block_column: usize,
    block_row: usize,
    scaler: &Scaler,
) -> u8 {
    let block_size = BLOCK_SIZE as usize;
    let start_column = block_column * block_size;
    let end_column = (start_column + block_size).min(width);
    let start_row = block_row * block_size;
    let end_row = (start_row + block_size).min(height);

    let mut maximum = f32::NEG_INFINITY;

    for row in start_row..end_row {
        let row_start = row * width;
        let pixels = &values[row_start + start_column..row_start + end_column];
        let row_maximum = pixels.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        maximum = maximum.max(row_maximum);
    }

    scaler.quantise(maximum)
}
