use std::fs;
use std::path::Path;

use crate::video::output::sibling_directory;
use crate::video::{persist, temporary_sibling};

#[test]
fn temporary_sibling_is_in_the_target_directory() {
    let directory = tempfile::tempdir().expect("the temporary directory should be created");
    let target = directory.path().join("map.parquet");

    let temporary = temporary_sibling(&target, ".tmp").expect("the sibling should be created");
    let file_name = temporary
        .file_name()
        .expect("the sibling should have a file name")
        .to_string_lossy()
        .into_owned();

    assert_eq!(temporary.parent(), Some(directory.path()));
    assert!(file_name.starts_with(".map.parquet."));
    assert!(file_name.ends_with(".tmp"));
}

#[test]
fn sibling_directory_of_a_bare_name_is_the_current_directory() {
    assert_eq!(sibling_directory(Path::new("map.parquet")), Path::new("."));
    assert_eq!(sibling_directory(Path::new("out/map.parquet")), Path::new("out"));
}

#[test]
fn a_target_without_a_file_name_is_an_error() {
    let error = temporary_sibling(Path::new(".."), ".tmp").expect_err("'..' has no file name");

    assert!(format!("{error:#}").contains(".."));
}

#[test]
fn dropping_a_temporary_path_deletes_it() {
    let directory = tempfile::tempdir().expect("the temporary directory should be created");
    let target = directory.path().join("map.parquet");

    let temporary = temporary_sibling(&target, ".tmp").expect("the sibling should be created");
    let temporary_path = temporary.to_path_buf();
    fs::write(&temporary, b"data").expect("the sibling should be written");
    drop(temporary);

    assert!(!temporary_path.exists());
}

#[test]
fn persist_moves_the_file_into_place() {
    let directory = tempfile::tempdir().expect("the temporary directory should be created");
    let target = directory.path().join("map.parquet");

    let temporary = temporary_sibling(&target, ".tmp").expect("the sibling should be created");
    let temporary_path = temporary.to_path_buf();
    fs::write(&temporary, b"new map").expect("the sibling should be written");
    persist(temporary, &target).expect("the sibling should be persisted");

    assert_eq!(fs::read(&target).expect("the target should exist"), b"new map");
    assert!(!temporary_path.exists());
}

#[test]
fn persist_replaces_an_existing_file() {
    let directory = tempfile::tempdir().expect("the temporary directory should be created");
    let target = directory.path().join("map.parquet");
    fs::write(&target, b"old map").expect("the existing target should be written");

    let temporary = temporary_sibling(&target, ".tmp").expect("the sibling should be created");
    fs::write(&temporary, b"new map").expect("the sibling should be written");
    persist(temporary, &target).expect("the sibling should be persisted");

    assert_eq!(fs::read(&target).expect("the target should exist"), b"new map");
}

#[test]
fn a_missing_target_directory_is_an_error() {
    let directory = tempfile::tempdir().expect("the temporary directory should be created");
    let missing = directory.path().join("missing");
    let target = missing.join("map.parquet");

    let error = temporary_sibling(&target, ".tmp").expect_err("the directory does not exist");

    assert!(format!("{error:#}").contains(&missing.display().to_string()));
}

#[test]
fn a_persist_into_a_missing_directory_leaves_no_temporary_file() {
    let directory = tempfile::tempdir().expect("the temporary directory should be created");
    let target = directory.path().join("missing").join("map.parquet");

    let temporary = temporary_sibling(&directory.path().join("map.parquet"), ".tmp")
        .expect("the sibling should be created");
    let temporary_path = temporary.to_path_buf();
    let error = persist(temporary, &target).expect_err("the target directory does not exist");

    assert!(format!("{error:#}").contains("could not move"));
    assert!(!temporary_path.exists());
}

#[cfg(unix)]
#[test]
fn persisted_files_have_the_mode_of_a_plain_file() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("the temporary directory should be created");
    let plain = directory.path().join("plain");
    fs::File::create(&plain).expect("the plain file should be created");
    let target = directory.path().join("map.parquet");

    let temporary = temporary_sibling(&target, ".tmp").expect("the sibling should be created");
    persist(temporary, &target).expect("the sibling should be persisted");

    let plain_mode = fs::metadata(&plain)
        .expect("the plain file should exist")
        .permissions()
        .mode()
        & 0o777;
    let target_mode = fs::metadata(&target)
        .expect("the target should exist")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(target_mode, plain_mode);
}
