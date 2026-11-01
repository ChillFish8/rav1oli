use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::decode::lock::IndexLock;

#[test]
fn acquiring_creates_the_lock_file_and_leaves_it_after_release() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("video.lock");

    let lock = IndexLock::acquire(&path).unwrap();
    drop(lock);

    assert!(path.exists());
}

#[test]
fn a_second_acquire_waits_for_the_first_to_release() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("video.lock");
    let first = IndexLock::acquire(&path).unwrap();

    let (sender, receiver) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let second = IndexLock::acquire(&path).unwrap();
        sender.send(()).unwrap();
        drop(second);
    });

    let early = receiver.recv_timeout(Duration::from_millis(200));
    let timed_out = matches!(early, Err(mpsc::RecvTimeoutError::Timeout));
    assert!(timed_out);

    drop(first);

    let late = receiver.recv_timeout(Duration::from_secs(5));
    assert!(late.is_ok());
    waiter.join().unwrap();
}
