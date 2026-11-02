use std::path::Path;

use clap::error::ErrorKind;

use super::parse_analyse_args as parse_args;

fn path_text(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

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

#[test]
fn a_map_that_is_a_directory_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let map = path_text(directory.path());
    let args = parse_args(&["test", "analyse", "-i", "a.mkv", "-m", &map]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains("'--map'"));
    assert!(error.to_string().contains(&map));
}

#[test]
fn a_map_with_a_trailing_separator_is_rejected() {
    let map = format!("missing{}", std::path::MAIN_SEPARATOR);
    let args = parse_args(&["test", "analyse", "-i", "a.mkv", "-m", &map]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains(&map));
}

#[test]
fn a_kept_encode_that_is_a_directory_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let kept = path_text(directory.path());
    let arguments = [
        "test",
        "analyse",
        "-i",
        "a.mkv",
        "-m",
        "b",
        "--keep-encode",
        &kept,
    ];
    let args = parse_args(&arguments);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains("'--keep-encode'"));
    assert!(error.to_string().contains(&kept));
}

#[test]
fn a_dotted_alias_of_the_input_is_rejected() {
    // Tests run from the crate directory, which holds `Cargo.toml`.
    let as_map = parse_args(&["test", "analyse", "-i", "Cargo.toml", "-m", "./Cargo.toml"]);
    let arguments = [
        "test",
        "analyse",
        "-i",
        "Cargo.toml",
        "-m",
        "b",
        "--keep-encode",
        "./Cargo.toml",
    ];
    let as_kept = parse_args(&arguments);

    let map_error = as_map.validate_paths().expect_err("validation should fail");
    let kept_error = as_kept.validate_paths().expect_err("validation should fail");

    assert!(
        map_error
            .to_string()
            .contains("the arguments '--map' and '--input' must be different files")
    );
    assert!(
        kept_error
            .to_string()
            .contains("the argument '--keep-encode' must not be the same file as '--input'")
    );
}

#[test]
fn a_dotted_alias_of_a_new_map_is_rejected() {
    let arguments = [
        "test",
        "analyse",
        "-i",
        "a.mkv",
        "-m",
        "missing-map.parquet",
        "--keep-encode",
        "./missing-map.parquet",
    ];
    let args = parse_args(&arguments);

    let error = args.validate_paths().expect_err("validation should fail");

    assert!(
        error
            .to_string()
            .contains("the argument '--keep-encode' must not be the same file as '--map'")
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_to_the_input_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("in.mkv");
    let link = directory.path().join("link.mkv");
    std::fs::write(&input, b"video").unwrap();
    std::os::unix::fs::symlink(&input, &link).unwrap();
    let input = path_text(&input);
    let link = path_text(&link);
    let args = parse_args(&["test", "analyse", "-i", &input, "-m", &link]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert!(
        error
            .to_string()
            .contains("the arguments '--map' and '--input' must be different files")
    );
}

#[test]
fn a_map_in_a_missing_directory_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let map = path_text(&directory.path().join("missing").join("map.parquet"));
    let args = parse_args(&["test", "analyse", "-i", "a.mkv", "-m", &map]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains("'--map' needs an existing directory"));
    assert!(error.to_string().contains("missing"));
}

#[test]
fn a_kept_encode_in_a_missing_directory_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let kept = path_text(&directory.path().join("missing").join("kept.ivf"));
    let arguments = [
        "test",
        "analyse",
        "-i",
        "a.mkv",
        "-m",
        "b",
        "--keep-encode",
        &kept,
    ];
    let args = parse_args(&arguments);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(
        error
            .to_string()
            .contains("'--keep-encode' needs an existing directory")
    );
}
