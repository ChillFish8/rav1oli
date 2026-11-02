use super::{BUTTERAUGLI, CVVDP};
use crate::Quantisation;
use crate::blocks::scalar_block_scores;

const CUSTOM: Quantisation = Quantisation {
    cap: 8.0,
    floor: 1.25,
    top: 255,
};

struct TestRandom(u32);

impl TestRandom {
    fn next_f32(&mut self, low: f32, high: f32) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;

        let unit = (self.0 >> 8) as f32 / (1 << 24) as f32;
        low + unit * (high - low)
    }
}

fn assert_paths_match(values: &[f32], width: usize, height: usize, quantisation: &Quantisation) {
    let reference = scalar_block_scores(values, width, height, quantisation);
    #[cfg_attr(
        not(any(target_arch = "x86_64", target_arch = "aarch64")),
        expect(unused_mut, reason = "no SIMD path is added on this architecture")
    )]
    let mut paths: Vec<(&str, Vec<u8>)> = Vec::new();

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        let mut blocks = vec![0; reference.len()];
        // SAFETY: AVX2 support was just detected, and the lengths match the map size.
        unsafe { crate::avx2::block_scores(values, width, height, quantisation, &mut blocks) };
        paths.push(("avx2", blocks));
    }

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("avx512bw") {
        let mut blocks = vec![0; reference.len()];
        // SAFETY: AVX-512F and AVX-512BW support was just detected, and the lengths match the map size.
        unsafe { crate::avx512::block_scores(values, width, height, quantisation, &mut blocks) };
        paths.push(("avx512", blocks));
    }

    #[cfg(target_arch = "aarch64")]
    {
        let mut blocks = vec![0; reference.len()];
        // SAFETY: NEON is part of every aarch64 target, and the lengths match the map size.
        unsafe { crate::neon::block_scores(values, width, height, quantisation, &mut blocks) };
        paths.push(("neon", blocks));
    }

    for (name, blocks) in paths {
        let first_difference = blocks
            .iter()
            .zip(&reference)
            .position(|(found, expected)| found != expected);
        let message = format!(
            "{name} differs at block {first_difference:?} for {width}x{height} with {quantisation:?}"
        );
        assert!(
            first_difference.is_none() && blocks.len() == reference.len(),
            "{message}"
        );
    }
}

/// Builds a random map where each block draws its pixels below its own random ceiling.
fn random_map(random: &mut TestRandom, width: usize, height: usize, quantisation: &Quantisation) -> Vec<f32> {
    let blocks_wide = width.div_ceil(8);
    let blocks_high = height.div_ceil(8);
    let highest = quantisation.cap * 0.6;
    let ceilings: Vec<f32> = (0..blocks_wide * blocks_high)
        .map(|_| random.next_f32(0.0, highest))
        .collect();

    let mut values = Vec::with_capacity(width * height);

    for row in 0..height {
        for column in 0..width {
            let block = (row / 8) * blocks_wide + column / 8;
            let value = random.next_f32(-1.0, ceilings[block]);
            values.push(value);
        }
    }

    values
}

#[test]
fn random_maps_match_scalar() {
    let widths = [1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 135, 1918];
    let heights = [1, 7, 8, 9, 17, 24];
    let mut random = TestRandom(0x5eed);

    for quantisation in [BUTTERAUGLI, CVVDP, CUSTOM] {
        for width in widths {
            for height in heights {
                let values = random_map(&mut random, width, height, &quantisation);
                assert_paths_match(&values, width, height, &quantisation);
            }
        }
    }
}

#[test]
fn special_values_match_scalar() {
    let width = 72;

    for quantisation in [BUTTERAUGLI, CVVDP, CUSTOM] {
        let specials = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0,
            0.0,
            -1.0,
            quantisation.floor,
            quantisation.floor.next_down(),
            quantisation.cap,
            quantisation.cap.next_up(),
        ];
        let blocks_wide = width / 8;
        let height = specials.len() * 8;

        for fills_block in [false, true] {
            let mut values = vec![0.0_f32; width * height];

            for row in 0..height {
                for column in 0..width {
                    let block_row = row / 8;
                    let block_column = column / 8;
                    let block = block_row * blocks_wide + block_column;
                    let hot_pixel = block % 64;
                    let is_hot = (row % 8) * 8 + column % 8 == hot_pixel;

                    if fills_block || is_hot {
                        let special = (block_row + block_column) % specials.len();
                        values[row * width + column] = specials[special];
                    }
                }
            }

            assert_paths_match(&values, width, height, &quantisation);
        }
    }
}

#[test]
fn a_hot_pixel_lights_up_only_its_own_block() {
    let quantisation = BUTTERAUGLI;
    let height = 8;

    for width in [16, 32, 64] {
        let pixels = width * height;

        for row in 0..height {
            for column in 0..width {
                let index = row * width + column;
                let share = (index + 1) as f32 / pixels as f32;
                let value = quantisation.floor + (quantisation.cap - quantisation.floor) * share;

                let mut values = vec![0.0_f32; pixels];
                values[index] = value;

                let mut expected = vec![0; width / 8];
                expected[column / 8] = quantisation.quantise(value);

                let reference = scalar_block_scores(&values, width, height, &quantisation);
                assert_eq!(reference, expected, "hot pixel at column {column} and row {row}");
                assert_paths_match(&values, width, height, &quantisation);
            }
        }
    }
}

#[test]
fn halfway_values_match_scalar() {
    for quantisation in [BUTTERAUGLI, CVVDP, CUSTOM] {
        let mut halfway = Vec::new();

        for code in 0..255_u32 {
            let value = (code as f32 + 0.5) * quantisation.cap / f32::from(quantisation.top);
            halfway.push(value);
            halfway.push(value.next_down());
        }

        let blocks_high = halfway.len().div_ceil(8);
        let width = 64;
        let height = blocks_high * 8;
        let mut values = vec![0.0_f32; width * height];

        for (index, value) in halfway.iter().enumerate() {
            let block_column = index % 8;
            let block_row = index / 8;

            for row in 0..8 {
                for column in 0..8 {
                    let offset = (block_row * 8 + row) * width + block_column * 8 + column;
                    values[offset] = *value;
                }
            }
        }

        assert_paths_match(&values, width, height, &quantisation);
    }
}

#[test]
fn largest_value_below_a_half_rounds_down() {
    let quantisation = Quantisation {
        cap: 1.0,
        floor: 0.0,
        top: 1,
    };
    let below = vec![0.5_f32.next_down(); 64];
    let exact = vec![0.5_f32; 64];

    let below_codes = scalar_block_scores(&below, 8, 8, &quantisation);
    let exact_codes = scalar_block_scores(&exact, 8, 8, &quantisation);

    assert_eq!(below_codes, vec![0]);
    assert_eq!(exact_codes, vec![1]);
    assert_paths_match(&below, 8, 8, &quantisation);
    assert_paths_match(&exact, 8, 8, &quantisation);
}
