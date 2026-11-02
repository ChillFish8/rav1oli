use rav1oli_table::{FrameRate, SourceInfo};

use crate::encoding::{GridBuilder, check_frame_index};
use crate::tables::Lookups;

fn lookups() -> Lookups {
    let mut drops = [0; 256];
    let mut cuts = [0; 256];
    for score in 0..256 {
        drops[score] = score.min(80) as u8;
        cuts[score] = score.min(230) as u8;
    }

    Lookups { drops, cuts }
}

fn source(width: u32, height: u32) -> SourceInfo {
    SourceInfo {
        hash: String::new(),
        width,
        height,
        frame_count: 1,
        frame_rate: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    }
}

fn builder(width: u32, height: u32) -> GridBuilder {
    let source = source(width, height);
    GridBuilder::new(lookups(), &source).unwrap()
}

#[test]
fn cuts_are_the_per_block_lookup() {
    let mut grid_builder = builder(32, 16);
    let scores = [0, 1, 100, 255, 230, 7, 8, 9];

    let grids = grid_builder.build(&scores).unwrap();

    assert_eq!(grids.cuts(), [0, 1, 100, 230, 230, 7, 8, 9]);
}

#[test]
fn an_all_zero_superblock_drops_nothing() {
    let mut grid_builder = builder(64, 64);
    let scores = [0; 64];

    let grids = grid_builder.build(&scores).unwrap();

    assert_eq!(grids.drops(), [0]);
}

#[test]
fn one_hot_block_does_not_raise_the_superblock() {
    let mut grid_builder = builder(64, 64);
    let mut scores = [0; 64];
    scores[10] = 80;

    let grids = grid_builder.build(&scores).unwrap();

    assert_eq!(grids.drops(), [0]);
}

#[test]
fn a_cluster_of_seven_hot_blocks_raises_the_superblock() {
    let mut grid_builder = builder(64, 64);
    let mut scores = [0; 64];
    scores[20..27].fill(80);

    let grids = grid_builder.build(&scores).unwrap();

    assert_eq!(grids.drops(), [80]);
}

#[test]
fn partial_superblocks_use_only_blocks_inside_the_frame() {
    let mut grid_builder = builder(72, 72);
    let mut scores = [0; 81];
    scores[80] = 80;

    let grids = grid_builder.build(&scores).unwrap();

    assert_eq!(grids.drops(), [0, 0, 0, 80]);
}

#[test]
fn a_frame_smaller_than_a_superblock_has_one_drop() {
    let mut grid_builder = builder(40, 24);
    let scores = [80; 15];

    let grids = grid_builder.build(&scores).unwrap();

    assert_eq!(grids.drops(), [80]);
}

#[test]
fn grids_for_an_odd_size_match_svt() {
    let mut grid_builder = builder(64, 62);
    let scores = [3; 64];

    let grids = grid_builder.build(&scores).unwrap();

    assert_eq!(grids.width(), 64);
    assert_eq!(grids.height(), 62);
    assert_eq!(grids.cut_columns(), 8);
    assert_eq!(grids.cut_rows(), 8);
    assert_eq!(grids.drop_columns(), 1);
    assert_eq!(grids.drop_rows(), 1);
    assert_eq!(grids.drops().len(), grids.drop_columns() * grids.drop_rows());
    assert_eq!(grids.cuts().len(), grids.cut_columns() * grids.cut_rows());
}

#[test]
fn superblocks_are_row_major() {
    let mut grid_builder = builder(128, 128);

    let mut top_right = [0; 256];
    for row in 0..8 {
        top_right[row * 16 + 8..row * 16 + 15].fill(80);
    }
    let grids = grid_builder.build(&top_right).unwrap();
    assert_eq!(grids.drops(), [0, 80, 0, 0]);

    let mut bottom_left = [0; 256];
    for row in 8..16 {
        bottom_left[row * 16..row * 16 + 7].fill(80);
    }
    let grids = grid_builder.build(&bottom_left).unwrap();
    assert_eq!(grids.drops(), [0, 0, 80, 0]);
}

#[test]
fn a_wrong_score_count_is_an_error() {
    let mut grid_builder = builder(16, 16);

    let error = grid_builder.build(&[0; 3]).unwrap_err();

    let message = format!("{error:#}");
    assert_eq!(message, "the frame has 3 scores but a 2x2 block grid needs 4");
}

#[test]
fn a_mismatched_grid_size_is_an_error() {
    let source = source(64, 64);

    let lookups = lookups();

    let result = GridBuilder::with_expected_grid(lookups, &source, 9, 8);

    let error = result.err().expect("the grid should be rejected");
    let message = format!("{error:#}");
    assert!(message.contains("8x8"), "{message}");
    assert!(message.contains("9x8"), "{message}");
}

#[test]
fn a_map_frame_out_of_step_is_an_error() {
    let error = check_frame_index(4, 3).unwrap_err();

    assert_eq!(
        error.to_string(),
        "the map's frame 4 does not match source frame 3"
    );
    assert!(check_frame_index(3, 3).is_ok());
}
