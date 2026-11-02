use std::path::Path;

use clap::error::ErrorKind;

use super::parse_encode_args as parse_args;

fn path_text(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

fn existing_map(directory: &Path) -> String {
    let map = directory.join("map.parquet");
    std::fs::write(&map, b"map").unwrap();
    path_text(&map)
}

#[test]
fn the_output_cannot_be_the_input_or_the_map() {
    let directory = tempfile::tempdir().unwrap();
    let map = existing_map(directory.path());
    let as_input = parse_args(&["test", "encode", "-i", "a.mkv", "-m", &map, "-o", "a.mkv"]);
    let as_alias = parse_args(&["test", "encode", "-i", "a.mkv", "-m", &map, "-o", "./a.mkv"]);
    let as_map = parse_args(&["test", "encode", "-i", "a.mkv", "-m", &map, "-o", &map]);

    let input_error = as_input.validate_paths().expect_err("validation should fail");
    let alias_error = as_alias.validate_paths().expect_err("validation should fail");
    let map_error = as_map.validate_paths().expect_err("validation should fail");

    assert_eq!(input_error.kind(), ErrorKind::ArgumentConflict);
    assert!(input_error.to_string().contains("'--output' and '--input'"));
    assert_eq!(alias_error.kind(), ErrorKind::ArgumentConflict);
    assert_eq!(map_error.kind(), ErrorKind::ArgumentConflict);
    assert!(map_error.to_string().contains("'--output' and '--map'"));
}

#[test]
fn the_map_cannot_be_the_input() {
    let args = parse_args(&["test", "encode", "-i", "a.mkv", "-m", "a.mkv", "-o", "b.ivf"]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    assert!(error.to_string().contains("'--map' and '--input'"));
}

#[test]
fn the_output_cannot_be_a_directory() {
    let directory = tempfile::tempdir().unwrap();
    let map = existing_map(directory.path());
    let output = path_text(directory.path());
    let args = parse_args(&["test", "encode", "-i", "a.mkv", "-m", &map, "-o", &output]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains("'--output'"));
}

#[test]
fn the_map_must_exist() {
    let directory = tempfile::tempdir().unwrap();
    let map = path_text(&directory.path().join("missing.parquet"));
    let args = parse_args(&["test", "encode", "-i", "a.mkv", "-m", &map, "-o", "b.ivf"]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains("'--map'"));
    assert!(error.to_string().contains(&map));
}

#[test]
fn an_invalid_tables_file_is_rejected_before_any_work() {
    let directory = tempfile::tempdir().unwrap();
    let map = existing_map(directory.path());
    let tables = directory.path().join("tables.json");
    std::fs::write(&tables, r#"{"version": 2, "metric": "cvvdp"}"#).unwrap();
    let tables = path_text(&tables);
    let arguments = [
        "test", "encode", "-i", "a.mkv", "-m", &map, "-o", "b.ivf", "--tables", &tables,
    ];
    let args = parse_args(&arguments);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains("invalid value for '--tables'"));
    assert!(error.to_string().contains(&tables));
}

#[test]
fn tables_is_optional() {
    let directory = tempfile::tempdir().unwrap();
    let map = existing_map(directory.path());
    let args = parse_args(&["test", "encode", "-i", "a.mkv", "-m", &map, "-o", "b.ivf"]);

    assert_eq!(args.tables, None);
    args.validate_paths().expect("distinct paths are valid");
}

#[test]
fn the_output_in_a_missing_directory_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let map = existing_map(directory.path());
    let output = path_text(&directory.path().join("missing").join("out.ivf"));
    let args = parse_args(&["test", "encode", "-i", "a.mkv", "-m", &map, "-o", &output]);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(
        error
            .to_string()
            .contains("'--output' needs an existing directory")
    );
    assert!(error.to_string().contains("missing"));
}
