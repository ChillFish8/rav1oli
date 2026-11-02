use std::path::Path;

use rav1oli_table::FrameRate;

use super::{encoder_options, write_constant_map, write_synthetic_map};
use crate::commands::options::EncoderOptions;
use crate::decode::tests::{copy_fixture, copy_ten_bit_fixture};
use crate::decode::{CacheLocation, Decoder};
use crate::encoding::{EncodeRequest, average_kbps, encode};
use crate::interrupt;
use crate::video::SourceVideo;

const CVVDP_TABLE: &str = r#"{"version": 1, "metric": "cvvdp"}"#;

struct ResetInterrupt;

impl Drop for ResetInterrupt {
    fn drop(&mut self) {
        interrupt::set_interrupted(false);
    }
}

fn file_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn request<'a>(
    input: &'a Path,
    map: &'a Path,
    output: &'a Path,
    tables: Option<&'a Path>,
    encoder: &'a EncoderOptions,
) -> EncodeRequest<'a> {
    EncodeRequest {
        input,
        map,
        output,
        tables,
        encoder,
        cache: CacheLocation::BesideVideo,
    }
}

#[test]
fn encode_writes_an_output_matching_the_source() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = write_synthetic_map(&video, &directory.path().join("map.parquet"), 40);
    let output = directory.path().join("final.ivf");
    let encoder = encoder_options(40);

    let encode_request = request(&video, &map, &output, None, &encoder);

    let outcome = encode(&encode_request).unwrap();

    let source = Decoder::open_uncached(&video).unwrap();
    let source_frames = source.properties().num_frames as u64;
    let encoded = Decoder::open_uncached(&output).unwrap();
    let properties = encoded.properties();
    assert_eq!(outcome.frames, source_frames);
    assert_eq!(properties.num_frames as u64, source_frames);
    assert_eq!((properties.ss_mod_width, properties.ss_mod_height), (64, 64));
    assert_eq!(outcome.bytes, std::fs::metadata(&output).unwrap().len());
    assert!(outcome.bytes > 0);
}

#[test]
fn higher_map_scores_change_the_encoded_size() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let low_map = write_constant_map(&video, &directory.path().join("low.parquet"), 40, 0);
    let high_map = write_constant_map(&video, &directory.path().join("high.parquet"), 40, 255);
    let low_output = directory.path().join("low.ivf");
    let high_output = directory.path().join("high.ivf");
    let encoder = encoder_options(40);

    let low_request = request(&video, &low_map, &low_output, None, &encoder);
    let high_request = request(&video, &high_map, &high_output, None, &encoder);

    let low = encode(&low_request).unwrap();
    let high = encode(&high_request).unwrap();

    assert!(high.bytes > low.bytes, "{} vs {}", high.bytes, low.bytes);
}

#[test]
fn a_map_from_another_source_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let other = copy_ten_bit_fixture(directory.path());
    let map = write_synthetic_map(&other, &directory.path().join("map.parquet"), 40);
    let output = directory.path().join("final.ivf");
    let encoder = encoder_options(40);

    let encode_request = request(&video, &map, &output, None, &encoder);

    let error = encode(&encode_request).unwrap_err();

    assert!(format!("{error:#}").contains("hash"), "{error:#}");
    assert!(!output.exists());
}

#[test]
fn a_differing_crf_still_encodes() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = write_synthetic_map(&video, &directory.path().join("map.parquet"), 40);
    let output = directory.path().join("final.ivf");
    let encoder = encoder_options(45);

    let encode_request = request(&video, &map, &output, None, &encoder);

    let outcome = encode(&encode_request).unwrap();

    assert!(outcome.frames > 0);
    assert!(output.exists());
}

#[test]
fn a_table_for_another_metric_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = write_synthetic_map(&video, &directory.path().join("map.parquet"), 40);
    let table = directory.path().join("table.json");
    std::fs::write(&table, CVVDP_TABLE).unwrap();
    let output = directory.path().join("final.ivf");
    let encoder = encoder_options(40);

    let encode_request = request(&video, &map, &output, Some(&table), &encoder);

    let error = encode(&encode_request).unwrap_err();

    let message = format!("{error:#}");
    assert!(
        message.contains("cvvdp") && message.contains("butteraugli"),
        "{message}"
    );
    assert!(!output.exists());
}

#[test]
fn an_interrupted_encode_leaves_no_output() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let map = write_synthetic_map(&video, &directory.path().join("map.parquet"), 40);
    let output = directory.path().join("final.ivf");
    let encoder = encoder_options(40);

    let _reset = ResetInterrupt;
    interrupt::set_interrupted(true);
    let encode_request = request(&video, &map, &output, None, &encoder);
    let error = encode(&encode_request).unwrap_err();

    assert!(format!("{error:#}").contains("interrupted"), "{error:#}");
    assert!(!output.exists());
    let names = file_names(directory.path());
    assert!(names.iter().all(|name| !name.ends_with(".ivf")), "{names:?}");
}

#[test]
fn an_existing_output_is_only_replaced_on_success() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let other = copy_ten_bit_fixture(directory.path());
    let good_map = write_synthetic_map(&video, &directory.path().join("good.parquet"), 40);
    let bad_map = write_synthetic_map(&other, &directory.path().join("bad.parquet"), 40);
    let output = directory.path().join("final.ivf");
    std::fs::write(&output, b"existing output").unwrap();
    let encoder = encoder_options(40);

    let encode_request = request(&video, &bad_map, &output, None, &encoder);

    encode(&encode_request).unwrap_err();
    assert_eq!(std::fs::read(&output).unwrap(), b"existing output");

    let encode_request = request(&video, &good_map, &output, None, &encoder);

    encode(&encode_request).unwrap();
    assert_ne!(std::fs::read(&output).unwrap(), b"existing output");
}

#[test]
fn average_kbps_uses_the_frame_rate() {
    let source = SourceVideo {
        width: 64,
        height: 64,
        bit_depth: 8,
        frame_count: 8,
        frame_rate: FrameRate {
            numerator: 25,
            denominator: 1,
        },
    };

    let kbps = average_kbps(4950, 8, &source);
    let empty = average_kbps(4950, 0, &source);

    assert!((kbps - 123.75).abs() < 1e-9, "{kbps}");
    assert_eq!(empty, 0.0);
}
