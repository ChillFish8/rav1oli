use crate::{DistortionGrids, GridError};

const WIDTH: u32 = 136;
const HEIGHT: u32 = 72;
const DROP_CELLS: usize = 3 * 2;
const CUT_CELLS: usize = 17 * 9;

#[test]
fn grids_round_up_to_the_frame() {
    let drops = vec![0; DROP_CELLS];
    let cuts = vec![0; CUT_CELLS];
    let grids = DistortionGrids::new(WIDTH, HEIGHT, drops, cuts).unwrap();

    assert_eq!(grids.drop_columns(), 3);
    assert_eq!(grids.drop_rows(), 2);
    assert_eq!(grids.cut_columns(), 17);
    assert_eq!(grids.cut_rows(), 9);
}

#[test]
fn a_drop_above_80_is_an_error() {
    let mut drops = vec![80; DROP_CELLS];
    drops[4] = 81;
    let cuts = vec![0; CUT_CELLS];

    let error = DistortionGrids::new(WIDTH, HEIGHT, drops, cuts).unwrap_err();
    let message = error.to_string();

    let is_expected = matches!(
        error,
        GridError::OutOfRange {
            row: 1,
            column: 1,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
    assert!(message.contains("row 1, column 1"), "{message}");
    assert!(message.contains("drop"), "{message}");
}

#[test]
fn a_cut_above_230_is_an_error() {
    let drops = vec![0; DROP_CELLS];
    let mut cuts = vec![230; CUT_CELLS];
    cuts[17 + 3] = 231;

    let error = DistortionGrids::new(WIDTH, HEIGHT, drops, cuts).unwrap_err();
    let message = error.to_string();

    let is_expected = matches!(
        error,
        GridError::OutOfRange {
            row: 1,
            column: 3,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
    assert!(message.contains("cut"), "{message}");
}

#[test]
fn a_wrong_size_is_an_error() {
    let drops = vec![0; DROP_CELLS];
    let cuts = vec![0; CUT_CELLS - 1];

    let error = DistortionGrids::new(WIDTH, HEIGHT, drops, cuts).unwrap_err();
    let message = error.to_string();

    let is_expected = matches!(
        error,
        GridError::Size {
            expected: CUT_CELLS,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
    assert!(message.contains("cut"), "{message}");
    assert!(message.contains("152"), "{message}");
}
