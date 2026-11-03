use crate::{DistortionGrids, GridError, MapMode};

const WIDTH: u32 = 136;
const HEIGHT: u32 = 72;
const DROP_CELLS: usize = 3 * 2;
const CUT_CELLS: usize = 17 * 9;

#[test]
fn grids_round_up_to_the_frame() {
    let drops = vec![0; DROP_CELLS];
    let cuts = vec![0; CUT_CELLS];
    let grids = DistortionGrids::additive(WIDTH, HEIGHT, drops, cuts).unwrap();

    assert_eq!(grids.superblock_columns(), 3);
    assert_eq!(grids.superblock_rows(), 2);
    assert_eq!(grids.block_columns(), 17);
    assert_eq!(grids.block_rows(), 9);
}

#[test]
fn a_drop_above_80_is_an_error() {
    let mut drops = vec![80; DROP_CELLS];
    drops[4] = 81;
    let cuts = vec![0; CUT_CELLS];

    let error = DistortionGrids::additive(WIDTH, HEIGHT, drops, cuts).unwrap_err();
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

    let error = DistortionGrids::additive(WIDTH, HEIGHT, drops, cuts).unwrap_err();
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

    let error = DistortionGrids::additive(WIDTH, HEIGHT, drops, cuts).unwrap_err();
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

#[test]
fn redistribute_grids_accept_any_weight() {
    let offsets = vec![-80; DROP_CELLS];
    let weights = (0..CUT_CELLS).map(|cell| (cell as i32 * 7 - 128) as i8).collect();
    let grids = DistortionGrids::redistribute(WIDTH, HEIGHT, offsets, weights).unwrap();

    assert_eq!(grids.mode(), MapMode::Redistribute);
    assert_eq!(grids.offsets().unwrap()[0], -80);
    assert_eq!(grids.drops(), None);
}

#[test]
fn an_offset_above_40_is_an_error() {
    let mut offsets = vec![40; DROP_CELLS];
    offsets[2] = 41;
    let weights = vec![0; CUT_CELLS];

    let error = DistortionGrids::redistribute(WIDTH, HEIGHT, offsets, weights).unwrap_err();
    let message = error.to_string();

    let is_expected = matches!(
        error,
        GridError::OutOfRange {
            value: 41,
            min: -80,
            max: 40,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
    assert!(message.contains("offset"), "{message}");
    assert!(message.contains("-80 to 40"), "{message}");
}

#[test]
fn an_offset_below_minus_80_is_an_error() {
    let mut offsets = vec![0; DROP_CELLS];
    offsets[5] = -81;
    let weights = vec![0; CUT_CELLS];

    let error = DistortionGrids::redistribute(WIDTH, HEIGHT, offsets, weights).unwrap_err();

    let is_expected = matches!(
        error,
        GridError::OutOfRange {
            value: -81,
            row: 1,
            column: 2,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
}

#[test]
fn redistribute_payload_holds_signed_bytes() {
    let offsets = vec![-32; DROP_CELLS];
    let weights = vec![-1; CUT_CELLS];
    let grids = DistortionGrids::redistribute(WIDTH, HEIGHT, offsets, weights).unwrap();

    let mut payload = Vec::new();
    let byte_len = grids.write_payload(&mut payload);

    // SAFETY: The payload holds at least `byte_len` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(payload.as_ptr().cast::<u8>(), byte_len) };
    let grid_bytes = &bytes[byte_len - DROP_CELLS - CUT_CELLS..];
    assert_eq!(grid_bytes[0], 0xE0);
    assert_eq!(grid_bytes[DROP_CELLS], 0xFF);
}
