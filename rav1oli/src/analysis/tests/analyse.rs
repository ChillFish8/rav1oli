use std::path::Path;

use rav1oli_table::{MapReader, MapSummary};

use crate::analysis::analyse;
use crate::commands::analyse::AnalyseArgs;
use crate::commands::tests::parse_analyse_args as parse_args;
use crate::decode::Decoder;
use crate::decode::tests::copy_fixture;
use crate::metrics::tests::gpu_id;

fn path_text(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

fn file_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let file_name = entry.file_name();
            file_name.to_string_lossy().into_owned()
        })
        .collect();
    names.sort();
    names
}

fn butteraugli_args(video: &Path, map: &Path, gpu: i32, extra: &[&str]) -> AnalyseArgs {
    let input = path_text(video);
    let map = path_text(map);
    let gpu = gpu.to_string();
    let mut arguments = vec![
        "test",
        "analyse",
        "-i",
        &input,
        "-m",
        &map,
        "--metric",
        "butteraugli",
        "--gpu",
        &gpu,
        "--preset",
        "12",
        "--crf",
        "40",
    ];
    arguments.extend_from_slice(extra);

    parse_args(&arguments)
}

#[test]
fn analyse_writes_a_map_matching_the_source() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = directory.path().join("map.parquet");
    let kept = directory.path().join("kept.ivf");
    let kept_text = path_text(&kept);
    let args = butteraugli_args(&video, &map, gpu_id(), &["--keep-encode", &kept_text]);

    let summary = analyse(&args).unwrap();

    assert!(matches!(summary, MapSummary::Butteraugli { .. }));
    let reader = MapReader::open(&map).unwrap();
    let metadata = reader.metadata();
    assert_eq!(metadata.source.width, 64);
    assert_eq!(metadata.source.height, 64);
    assert_eq!(metadata.encoder.preset, 12);
    assert_eq!(metadata.encoder.crf, 40);
    let fixture = Decoder::open_uncached(&video).unwrap();
    let fixture_frames = fixture.properties().num_frames as usize;
    assert_eq!(metadata.source.frame_count as usize, fixture_frames);
    assert_eq!(reader.count(), fixture_frames);
    assert!(kept.exists());
    let names = file_names(directory.path());
    assert_eq!(names, [".rav1oli", "kept.ivf", "map.parquet", "yuv420p.mkv"]);
}

#[test]
fn a_failed_analyse_leaves_no_files() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = directory.path().join("map.parquet");
    std::fs::write(&map, b"existing map").unwrap();
    let input = path_text(&video);
    let map_text = path_text(&map);
    let gpu = gpu_id().to_string();
    let arguments = [
        "test",
        "analyse",
        "-i",
        &input,
        "-m",
        &map_text,
        "--preset",
        "12",
        "--gpu",
        &gpu,
        "--display-model",
        "trix_fhd",
    ];
    let args = parse_args(&arguments);

    analyse(&args).unwrap_err();

    assert_eq!(std::fs::read(&map).unwrap(), b"existing map");
    let names = file_names(directory.path());
    assert!(
        names
            .iter()
            .all(|name| !name.ends_with(".tmp") && !name.ends_with(".ivf"))
    );
}

#[test]
fn a_failure_after_the_temporary_map_exists_leaves_no_files() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = directory.path().join("map.parquet");
    std::fs::write(&map, b"existing map").unwrap();
    let kept = directory.path().join("missing").join("kept.ivf");
    let kept_text = path_text(&kept);
    let args = butteraugli_args(&video, &map, gpu_id(), &["--keep-encode", &kept_text]);

    let error = analyse(&args).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("could not start the first-pass encode")
    );
    assert_eq!(std::fs::read(&map).unwrap(), b"existing map");
    let names = file_names(directory.path());
    assert!(
        names
            .iter()
            .all(|name| !name.ends_with(".tmp") && !name.ends_with(".ivf"))
    );
}

#[test]
fn a_temporary_encode_is_removed() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = directory.path().join("map.parquet");
    let args = butteraugli_args(&video, &map, gpu_id(), &[]);

    analyse(&args).unwrap();

    let names = file_names(directory.path());
    assert!(names.iter().all(|name| !name.ends_with(".ivf")));
    assert!(map.exists());
}

#[test]
fn a_missing_map_directory_fails_before_the_encode() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = directory.path().join("missing").join("map.parquet");
    let kept = directory.path().join("kept.ivf");
    let kept_text = path_text(&kept);
    let args = butteraugli_args(&video, &map, gpu_id(), &["--keep-encode", &kept_text]);

    analyse(&args).unwrap_err();

    assert!(!kept.exists());
}
