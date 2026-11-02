use rav1oli_table::{FrameRate, SourceInfo};

use crate::metrics::{BUTTERAUGLI_QUANTISATION, CVVDP_QUANTISATION, block_scores, quantise};

#[test]
fn values_below_the_floor_score_zero() {
    let code = quantise(0.4, &BUTTERAUGLI_QUANTISATION);
    assert_eq!(code, 0);
}

#[test]
fn values_at_the_floor_quantise_normally() {
    let code = quantise(0.5, &BUTTERAUGLI_QUANTISATION);
    assert_eq!(code, 16);
}

#[test]
fn values_at_or_above_the_cap_score_the_top_code() {
    let at_cap = quantise(8.0, &BUTTERAUGLI_QUANTISATION);
    let above_cap = quantise(12.0, &BUTTERAUGLI_QUANTISATION);
    assert_eq!(at_cap, 255);
    assert_eq!(above_cap, 255);
}

#[test]
fn cvvdp_halfway_scores_128() {
    let code = quantise(0.5, &CVVDP_QUANTISATION);
    assert_eq!(code, 128);
}

#[test]
fn cvvdp_zero_scores_zero() {
    let code = quantise(0.0, &CVVDP_QUANTISATION);
    assert_eq!(code, 0);
}

#[test]
fn negative_and_nan_values_score_zero() {
    let negative = quantise(-1.0, &CVVDP_QUANTISATION);
    let not_a_number = quantise(f32::NAN, &CVVDP_QUANTISATION);
    assert_eq!(negative, 0);
    assert_eq!(not_a_number, 0);
}

#[test]
fn block_scores_take_the_max_of_each_block() {
    let mut values = vec![0.0_f32; 16 * 8];
    for row in 0..8 {
        for column in 0..8 {
            values[row * 16 + column] = 1.0;
        }
    }
    values[3 * 16 + 5] = 4.0;

    let codes = block_scores(&values, 16, 8, &BUTTERAUGLI_QUANTISATION);

    let expected = quantise(4.0, &BUTTERAUGLI_QUANTISATION);
    assert_eq!(codes, vec![expected, 0]);
}

#[test]
fn nan_pixels_are_ignored_by_the_block_max() {
    let mut values = vec![1.0_f32; 8 * 8];
    values[0] = f32::NAN;

    let codes = block_scores(&values, 8, 8, &BUTTERAUGLI_QUANTISATION);

    let expected = quantise(1.0, &BUTTERAUGLI_QUANTISATION);
    assert_eq!(codes, vec![expected]);
}

#[test]
fn partial_edge_blocks_use_the_pixels_that_exist() {
    let mut values = vec![0.0_f32; 10 * 9];
    values[8 * 10 + 9] = 8.0;

    let codes = block_scores(&values, 10, 9, &BUTTERAUGLI_QUANTISATION);

    assert_eq!(codes, vec![0, 0, 0, 255]);
}

#[test]
fn grid_matches_the_table_layout() {
    let (width, height) = (1918_usize, 1080_usize);
    let values = vec![0.0_f32; width * height];
    let info = SourceInfo {
        hash: String::new(),
        width: width as u32,
        height: height as u32,
        frame_count: 1,
        frame_rate: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    };

    let codes = block_scores(&values, width, height, &BUTTERAUGLI_QUANTISATION);

    assert_eq!(codes.len(), info.blocks_per_frame());
}

#[test]
fn an_empty_map_has_no_blocks() {
    let no_width = block_scores(&[], 0, 8, &BUTTERAUGLI_QUANTISATION);
    let no_height = block_scores(&[], 8, 0, &BUTTERAUGLI_QUANTISATION);
    assert!(no_width.is_empty());
    assert!(no_height.is_empty());
}
