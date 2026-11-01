mod cache;
mod decoder;
mod lock;

use std::path::{Path, PathBuf};

fn copy_fixture(directory: &Path) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = manifest.join("../rav1oli-bestsource/tests/media/yuv420p.mkv");
    let destination = directory.join("yuv420p.mkv");
    std::fs::copy(source, &destination).unwrap();
    destination
}
