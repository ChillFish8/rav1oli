use rav1oli_table::{BLOCK_SIZE, Quantisation};

pub const BUTTERAUGLI_QUANTISATION: Quantisation = Quantisation {
    cap: 8.0,
    floor: 0.5,
    top: 255,
};

pub const CVVDP_QUANTISATION: Quantisation = Quantisation {
    cap: 1.0,
    floor: 0.0,
    top: 255,
};

/// Converts a metric value into a score code.
///
/// Values below the floor, negative values and NaN score 0. Everything else is clamped to the
/// cap and scaled so the cap scores the top code.
pub fn quantise(value: f32, quantisation: &Quantisation) -> u8 {
    if value.is_nan() || value < quantisation.floor {
        return 0;
    }

    let clamped = value.clamp(0.0, quantisation.cap);
    let scaled = clamped / quantisation.cap * f32::from(quantisation.top);
    scaled.round() as u8
}

/// Reduces a packed `width * height` metric map to one code per block.
///
/// Each block holds the highest value inside it, ignoring NaN. Blocks on the right and bottom
/// edges only use the pixels that exist. Codes are returned row by row.
pub fn block_scores(values: &[f32], width: usize, height: usize, quantisation: &Quantisation) -> Vec<u8> {
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

    maximums
        .into_iter()
        .map(|maximum| quantise(maximum, quantisation))
        .collect()
}
