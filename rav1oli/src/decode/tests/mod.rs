mod cache;
mod decoder;
mod lock;

use std::path::{Path, PathBuf};

fn copy_media(directory: &Path, file_name: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let media = manifest.join("../rav1oli-bestsource/tests/media");
    let source = media.join(file_name);
    let destination = directory.join(file_name);
    std::fs::copy(source, &destination).unwrap();
    destination
}

pub(crate) fn copy_fixture(directory: &Path) -> PathBuf {
    copy_media(directory, "yuv420p.mkv")
}

pub(crate) fn copy_ten_bit_fixture(directory: &Path) -> PathBuf {
    copy_media(directory, "yuv420p10.mkv")
}
