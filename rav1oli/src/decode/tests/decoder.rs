use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, SystemTime};
use std::{fs, thread};

use super::copy_fixture;
use crate::decode::cache::CacheLocation;
use crate::decode::decoder::Decoder;
use crate::decode::lock::IndexLock;

fn index_files(directory: &Path) -> Vec<PathBuf> {
    let indexes = directory.join(".rav1oli").join("indexes");
    let entries = fs::read_dir(indexes).unwrap();
    let paths = entries.map(|entry| entry.unwrap().path());
    let is_index = |path: &PathBuf| {
        let extension = path.extension();
        extension.is_some_and(|extension| extension == "bsindex")
    };
    paths.filter(is_index).collect()
}

fn modified_time(path: &Path) -> SystemTime {
    fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn every_frame_is_decoded_then_none() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let mut decoder = Decoder::open(&video, &CacheLocation::BesideVideo).unwrap();

    assert_eq!(decoder.properties().num_frames, 8);

    let mut frames = decoder.frames();
    let mut decoded = 0;
    for _ in 0..8 {
        let frame = frames.next().unwrap().unwrap();
        assert_eq!(frame.width(), 64);
        assert_eq!(frame.height(), 64);
        decoded += 1;
    }

    assert_eq!(decoded, 8);
    assert!(frames.next().is_none());
}

#[test]
fn a_second_open_reuses_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    Decoder::open(&video, &CacheLocation::BesideVideo).unwrap();
    let indexes = index_files(dir.path());
    assert_eq!(indexes.len(), 1);
    let first_modified = modified_time(&indexes[0]);

    Decoder::open(&video, &CacheLocation::BesideVideo).unwrap();

    let indexes = index_files(dir.path());
    assert_eq!(indexes.len(), 1);
    assert_eq!(modified_time(&indexes[0]), first_modified);
}

#[test]
fn concurrent_opens_share_one_index() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());

    let handles: Vec<_> = (0..2)
        .map(|_| {
            let video = video.clone();
            thread::spawn(move || {
                let result = Decoder::open(&video, &CacheLocation::BesideVideo);
                result.is_ok()
            })
        })
        .collect();
    for handle in handles {
        let opened = handle.join().unwrap();
        assert!(opened);
    }

    assert_eq!(index_files(dir.path()).len(), 1);
}

#[test]
fn a_failed_open_releases_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("bad.mkv");
    fs::write(&video, b"not a video").unwrap();

    let error = Decoder::open(&video, &CacheLocation::BesideVideo).unwrap_err();

    let message = format!("{error:#}");
    let decoder_context = "could not open the video";
    assert!(message.contains("bad.mkv"));
    assert!(message.contains(decoder_context));

    let paths = CacheLocation::BesideVideo.paths_for(&video).unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let lock = IndexLock::acquire(&paths.lock).unwrap();
        sender.send(()).unwrap();
        drop(lock);
    });
    let acquired = receiver.recv_timeout(Duration::from_secs(5));
    assert!(acquired.is_ok());
}

#[test]
fn open_waits_for_a_held_index_lock() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let paths = CacheLocation::BesideVideo.paths_for(&video).unwrap();
    let lock = IndexLock::acquire(&paths.lock).unwrap();

    let (sender, receiver) = mpsc::channel();
    let opener = thread::spawn(move || {
        let result = Decoder::open(&video, &CacheLocation::BesideVideo);
        sender.send(result.is_ok()).unwrap();
    });

    let early = receiver.recv_timeout(Duration::from_millis(200));
    assert_eq!(early, Err(RecvTimeoutError::Timeout));

    drop(lock);

    let late = receiver.recv_timeout(Duration::from_secs(5));
    assert_eq!(late, Ok(true));
    opener.join().unwrap();
}

#[test]
fn open_uncached_writes_no_index() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());

    let uncached = Decoder::open_uncached(&video).unwrap();

    assert!(!dir.path().join(".rav1oli").exists());
    let has_index = fs::read_dir(dir.path()).unwrap().any(|entry| {
        let path = entry.unwrap().path();
        path.extension().is_some_and(|extension| extension == "bsindex")
    });
    assert!(!has_index);

    let cached = Decoder::open(&video, &CacheLocation::BesideVideo).unwrap();
    assert_eq!(uncached.properties().num_frames, cached.properties().num_frames);
}
