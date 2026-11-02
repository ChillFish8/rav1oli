use divan::Bencher;
use divan::counter::BytesCount;
use rav1oli_quantise::{Quantisation, block_scores};

const BUTTERAUGLI: Quantisation = Quantisation {
    cap: 8.0,
    floor: 0.5,
    top: 255,
};

const SIZES: &[(usize, usize)] = &[(1920, 1080), (3840, 2160)];

fn main() {
    divan::main();
}

/// Builds a map of values spread across the quantised range, from a fixed seed.
fn sample_map(width: usize, height: usize) -> Vec<f32> {
    let mut state = 0x5eed_u32;
    let scale = BUTTERAUGLI.cap / u32::MAX as f32;

    let mut values = Vec::with_capacity(width * height);
    for _ in 0..width * height {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;

        let value = state as f32 * scale;
        values.push(value);
    }

    values
}

#[divan::bench(args = SIZES)]
fn dispatched(bencher: Bencher, size: (usize, usize)) {
    let (width, height) = size;
    let values = sample_map(width, height);
    let input_bytes = BytesCount::of_slice(&values);

    bencher
        .counter(input_bytes)
        .bench(|| block_scores(divan::black_box(&values), width, height, &BUTTERAUGLI));
}
