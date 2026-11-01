use std::fs;
use std::path::Path;

use super::copy_fixture;
use crate::decode::cache::CacheLocation;

#[test]
fn beside_video_uses_a_rav1oli_folder_next_to_the_video() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());

    let paths = CacheLocation::BesideVideo.paths_for(&video).unwrap();

    let cache = dir.path().join(".rav1oli");
    assert_eq!(paths.index.parent().unwrap(), cache.join("indexes"));
    assert_eq!(paths.lock.parent().unwrap(), cache.join("locks"));
    assert!(cache.join("indexes").is_dir());
    assert!(cache.join("locks").is_dir());
}

#[test]
fn a_cache_directory_is_used_for_every_video() {
    let dir = tempfile::tempdir().unwrap();
    let first_folder = dir.path().join("first");
    let second_folder = dir.path().join("second");
    fs::create_dir_all(&first_folder).unwrap();
    fs::create_dir_all(&second_folder).unwrap();
    let first_video = copy_fixture(&first_folder);
    let second_video = copy_fixture(&second_folder);
    let cache = dir.path().join("cache");
    let location = CacheLocation::Directory(cache.clone());

    let first = location.paths_for(&first_video).unwrap();
    let second = location.paths_for(&second_video).unwrap();

    assert_eq!(first.index.parent().unwrap(), cache.join("indexes"));
    assert_eq!(second.index.parent().unwrap(), cache.join("indexes"));
    assert_eq!(first.lock.parent().unwrap(), cache.join("locks"));
    assert_eq!(second.lock.parent().unwrap(), cache.join("locks"));
}

#[test]
fn the_stem_is_the_file_name_and_a_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());

    let paths = CacheLocation::BesideVideo.paths_for(&video).unwrap();

    let stem = paths.index.file_name().unwrap().to_str().unwrap();
    let hash = stem.strip_prefix("yuv420p.mkv-").unwrap();
    assert_eq!(hash.len(), 16);
    assert!(
        hash.chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
    );
    let lock_name = paths.lock.file_name().unwrap().to_str().unwrap();
    let expected_lock_name = format!("{stem}.lock");
    assert_eq!(lock_name, expected_lock_name);
}

#[test]
fn rewriting_a_video_changes_its_stem() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("clip.mkv");

    fs::write(&video, b"first").unwrap();
    let first = CacheLocation::BesideVideo.paths_for(&video).unwrap();
    fs::write(&video, b"second").unwrap();
    let second = CacheLocation::BesideVideo.paths_for(&video).unwrap();

    assert_ne!(first.index, second.index);
}

#[test]
fn identical_names_in_different_folders_with_different_contents_do_not_collide() {
    let dir = tempfile::tempdir().unwrap();
    let first_folder = dir.path().join("first");
    let second_folder = dir.path().join("second");
    fs::create_dir_all(&first_folder).unwrap();
    fs::create_dir_all(&second_folder).unwrap();
    fs::write(first_folder.join("clip.mkv"), b"first").unwrap();
    fs::write(second_folder.join("clip.mkv"), b"second").unwrap();
    let location = CacheLocation::Directory(dir.path().join("cache"));

    let first_video = first_folder.join("clip.mkv");
    let second_video = second_folder.join("clip.mkv");

    let first = location.paths_for(&first_video).unwrap();
    let second = location.paths_for(&second_video).unwrap();

    assert_ne!(first.index, second.index);
    assert_ne!(first.lock, second.lock);
}

#[test]
fn a_bare_relative_path_resolves_next_to_the_working_directory() {
    let root = CacheLocation::BesideVideo.cache_root(Path::new("clip.mkv"));

    assert_eq!(root, Path::new(".").join(".rav1oli"));
}

#[test]
fn a_missing_video_names_the_video() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("missing.mkv");

    let error = CacheLocation::BesideVideo.paths_for(&video).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("missing.mkv"));
    assert!(!message.contains("hash"));
}

#[test]
fn an_unwritable_cache_folder_suggests_cache_dir() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let blocker = dir.path().join("blocker");
    fs::write(&blocker, b"not a folder").unwrap();

    let error = CacheLocation::Directory(blocker.clone())
        .paths_for(&video)
        .unwrap_err();

    let message = format!("{error:#}");
    let blocker_name = blocker.display().to_string();
    assert!(message.contains(&blocker_name));
    assert!(message.contains("--cache-dir"));
}

#[cfg(unix)]
#[test]
fn a_read_only_folder_beside_the_video_suggests_cache_dir() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("readonly");
    fs::create_dir_all(&folder).unwrap();
    let video = copy_fixture(&folder);
    let read_only = fs::Permissions::from_mode(0o555);
    let writable = fs::Permissions::from_mode(0o755);
    fs::set_permissions(&folder, read_only).unwrap();
    let probe = folder.join("probe");
    if fs::write(&probe, b"x").is_ok() {
        fs::set_permissions(&folder, writable.clone()).unwrap();
        return;
    }

    let result = CacheLocation::BesideVideo.paths_for(&video);
    fs::set_permissions(&folder, writable).unwrap();

    let error = result.unwrap_err();
    let message = format!("{error:#}");
    let cache_folder = folder.join(".rav1oli");
    let cache_folder = cache_folder.display().to_string();
    assert!(message.contains(&cache_folder));
    assert!(message.contains("--cache-dir"));
}

#[cfg(unix)]
#[test]
fn a_read_only_existing_indexes_folder_suggests_cache_dir() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let cache = dir.path().join(".rav1oli");
    let indexes = cache.join("indexes");
    let locks = cache.join("locks");
    fs::create_dir_all(&indexes).unwrap();
    fs::create_dir_all(&locks).unwrap();
    let read_only = fs::Permissions::from_mode(0o555);
    let writable = fs::Permissions::from_mode(0o755);
    fs::set_permissions(&indexes, read_only).unwrap();
    let probe = indexes.join("probe");
    if fs::write(&probe, b"x").is_ok() {
        fs::set_permissions(&indexes, writable).unwrap();
        return;
    }

    let result = CacheLocation::BesideVideo.paths_for(&video);
    fs::set_permissions(&indexes, writable).unwrap();

    let error = result.unwrap_err();
    let message = format!("{error:#}");
    let indexes_name = indexes.display().to_string();
    assert!(message.contains(&indexes_name));
    assert!(message.contains("--cache-dir"));
}
