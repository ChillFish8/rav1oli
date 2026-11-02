use clap::error::ErrorKind;

use super::parse_analyse_args as parse_args;

#[test]
fn the_map_cannot_be_the_input() {
    let args = parse_args(&["test", "analyse", "-i", "a.mkv", "-m", "a.mkv"]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    assert!(
        error
            .to_string()
            .contains("the arguments '--map' and '--input' must be different files")
    );
}

#[test]
fn keep_encode_cannot_be_the_input_or_the_map() {
    let as_input = parse_args(&[
        "test",
        "analyse",
        "-i",
        "a.mkv",
        "-m",
        "b",
        "--keep-encode",
        "a.mkv",
    ]);
    let as_map = parse_args(&["test", "analyse", "-i", "a.mkv", "-m", "b", "--keep-encode", "b"]);

    let input_error = as_input.validate_paths().expect_err("validation should fail");
    let map_error = as_map.validate_paths().expect_err("validation should fail");

    assert_eq!(input_error.kind(), ErrorKind::ArgumentConflict);
    assert!(
        input_error
            .to_string()
            .contains("the argument '--keep-encode' must not be the same file as '--input'")
    );
    assert_eq!(map_error.kind(), ErrorKind::ArgumentConflict);
    assert!(
        map_error
            .to_string()
            .contains("the argument '--keep-encode' must not be the same file as '--map'")
    );
}

#[test]
fn keep_encode_is_optional() {
    let args = parse_args(&["test", "analyse", "-i", "a.mkv", "-m", "b"]);

    assert_eq!(args.keep_encode, None);
    args.validate_paths().expect("distinct paths are valid");
}
