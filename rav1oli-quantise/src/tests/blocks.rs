use super::BUTTERAUGLI;
use crate::blocks::{block_scores, scalar_block, scalar_block_scores};

#[test]
fn block_scores_take_the_max_of_each_block() {
    let mut values = vec![0.0_f32; 16 * 8];

    for row in 0..8 {
        for column in 0..8 {
            values[row * 16 + column] = 1.0;
        }
    }

    values[3 * 16 + 5] = 4.0;

    let codes = block_scores(&values, 16, 8, &BUTTERAUGLI);

    let expected = BUTTERAUGLI.quantise(4.0);
    assert_eq!(codes, vec![expected, 0]);
}

#[test]
fn nan_pixels_are_ignored_by_the_block_max() {
    let mut values = vec![1.0_f32; 8 * 8];
    values[0] = f32::NAN;

    let codes = block_scores(&values, 8, 8, &BUTTERAUGLI);

    let expected = BUTTERAUGLI.quantise(1.0);
    assert_eq!(codes, vec![expected]);
}

#[test]
fn partial_edge_blocks_use_the_pixels_that_exist() {
    let mut values = vec![0.0_f32; 10 * 9];
    values[8 * 10 + 9] = 8.0;

    let codes = block_scores(&values, 10, 9, &BUTTERAUGLI);

    assert_eq!(codes, vec![0, 0, 0, 255]);
}

#[test]
fn an_empty_map_has_no_blocks() {
    let no_width = block_scores(&[], 0, 8, &BUTTERAUGLI);
    let no_height = block_scores(&[], 8, 0, &BUTTERAUGLI);
    assert!(no_width.is_empty());
    assert!(no_height.is_empty());
}

#[test]
#[should_panic]
fn a_map_shorter_than_its_size_panics() {
    let values = vec![0.0_f32; 63];
    block_scores(&values, 8, 8, &BUTTERAUGLI);
}

#[test]
fn scalar_block_matches_the_full_scalar_run() {
    let mut values = vec![0.0_f32; 10 * 9];
    values[8 * 10 + 9] = 8.0;

    let full = scalar_block_scores(&values, 10, 9, &BUTTERAUGLI);
    let scaler = BUTTERAUGLI.scaler();

    for block_row in 0..2 {
        for block_column in 0..2 {
            let code = scalar_block(&values, 10, 9, block_column, block_row, &scaler);
            assert_eq!(code, full[block_row * 2 + block_column]);
        }
    }
}
