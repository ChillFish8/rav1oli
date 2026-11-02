use rav1oli_bestsource::{Frame, PlanarTarget, PlaneTarget};
use rav1oli_svt::{DistortionGrids, EncoderSettings};

use crate::commands::options::EncoderOptions;
use crate::decode::Decoder;
use crate::decode::tests::{copy_fixture, copy_ten_bit_fixture};
use crate::metrics::FrameDescription;
use crate::video::{SourceVideo, colour_description, describe_source, encode_video, encoder_settings};

fn encoder_options() -> EncoderOptions {
    EncoderOptions {
        preset: 12,
        crf: 40,
        svt_args: vec![],
    }
}

fn first_pass_settings(decoder: &mut Decoder, source: &SourceVideo) -> (Frame, EncoderSettings) {
    let first_frame = decoder.frames().next().unwrap().unwrap();
    let description = FrameDescription::from_frame(&first_frame);
    let colour = colour_description(&description).unwrap();
    let settings = encoder_settings(source, colour, &encoder_options());
    (first_frame, settings)
}

/// Reads the 10-bit luma of `frame` without going through [EncoderPlanes](crate::video::planes::EncoderPlanes).
fn ten_bit_luma(frame: &Frame) -> Vec<u16> {
    let width = frame.ss_mod_width() as usize;
    let height = frame.ss_mod_height() as usize;
    let chroma_length = (width / 2) * (height / 2) * 2;
    let mut luma = vec![0u8; width * height * 2];
    let mut u_plane = vec![0u8; chroma_length];
    let mut v_plane = vec![0u8; chroma_length];
    let planes = vec![
        PlaneTarget {
            data: &mut luma,
            stride: width * 2,
        },
        PlaneTarget {
            data: &mut u_plane,
            stride: width,
        },
        PlaneTarget {
            data: &mut v_plane,
            stride: width,
        },
    ];
    let mut target = PlanarTarget { planes, alpha: None };
    frame.export_planar(&mut target).unwrap();

    let (pairs, _) = luma.as_chunks::<2>();
    pairs.iter().map(|pair| u16::from_ne_bytes(*pair)).collect()
}

#[test]
fn the_fixture_encodes_with_the_same_frame_count() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let output = dir.path().join("first.ivf");
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let (_, settings) = first_pass_settings(&mut decoder, &source);

    let written = encode_video(&mut decoder, &source, &settings, &output, "first pass", |_| {
        Ok(None)
    })
    .unwrap();

    assert_eq!(written, u64::from(source.frame_count));
    let encoded = Decoder::open_uncached(&output).unwrap();
    let properties = encoded.properties();
    assert_eq!(properties.num_frames, i64::from(source.frame_count));
    assert_eq!(properties.ss_mod_width, 64);
    assert_eq!(properties.ss_mod_height, 64);
}

#[test]
fn the_ten_bit_fixture_encodes_close_to_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_ten_bit_fixture(dir.path());
    let output = dir.path().join("first.ivf");
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let (first_frame, settings) = first_pass_settings(&mut decoder, &source);
    let source_luma = ten_bit_luma(&first_frame);

    let written = encode_video(&mut decoder, &source, &settings, &output, "first pass", |_| {
        Ok(None)
    })
    .unwrap();

    assert_eq!(source.bit_depth, 10);
    assert_eq!(written, u64::from(source.frame_count));
    assert!(source_luma.iter().any(|sample| *sample > 255));
    let mut encoded = Decoder::open_uncached(&output).unwrap();
    let properties = encoded.properties();
    assert_eq!(properties.num_frames, i64::from(source.frame_count));
    assert_eq!(properties.ss_mod_width, 64);
    assert_eq!(properties.ss_mod_height, 64);
    assert_eq!(properties.format.bits, 10);
    let encoded_frame = encoded.frames().next().unwrap().unwrap();
    let encoded_luma = ten_bit_luma(&encoded_frame);
    let total_difference: u64 = source_luma
        .iter()
        .zip(&encoded_luma)
        .map(|(source_sample, encoded_sample)| u64::from(source_sample.abs_diff(*encoded_sample)))
        .sum();
    let mean_difference = total_difference as f64 / source_luma.len() as f64;
    assert!(
        mean_difference < 8.0,
        "the mean luma difference is {mean_difference}"
    );
}

#[test]
fn a_failed_first_pass_removes_its_output() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let output = dir.path().join("kept.ivf");
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let (_, settings) = first_pass_settings(&mut decoder, &source);
    let longer = SourceVideo {
        frame_count: source.frame_count + 1,
        ..source
    };

    let error = encode_video(&mut decoder, &longer, &settings, &output, "first pass", |_| {
        Ok(None)
    })
    .unwrap_err();

    assert!(error.to_string().contains("frames but the source has"));
    assert!(!output.exists());
}

fn zero_grids() -> DistortionGrids {
    DistortionGrids::new(64, 64, vec![0; 1], vec![0; 64]).unwrap()
}

#[test]
fn grids_are_requested_for_every_frame_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let output = dir.path().join("final.ivf");
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let (_, settings) = first_pass_settings(&mut decoder, &source);
    let mut requested = Vec::new();
    let grids_for = |index: usize| {
        requested.push(index);
        Ok(Some(zero_grids()))
    };

    let written = encode_video(&mut decoder, &source, &settings, &output, "final pass", grids_for).unwrap();

    let expected: Vec<usize> = (0..source.frame_count as usize).collect();
    assert_eq!(requested, expected);
    assert_eq!(written, u64::from(source.frame_count));
    let encoded = Decoder::open_uncached(&output).unwrap();
    assert_eq!(encoded.properties().num_frames, i64::from(source.frame_count));
}

#[test]
fn a_failing_grids_callback_stops_and_removes_the_output() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let output = dir.path().join("final.ivf");
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let (_, settings) = first_pass_settings(&mut decoder, &source);
    let grids_for = |index: usize| {
        if index == 2 {
            anyhow::bail!("no grids for frame two");
        }
        Ok(Some(zero_grids()))
    };

    let error = encode_video(&mut decoder, &source, &settings, &output, "final pass", grids_for).unwrap_err();

    assert!(format!("{error:#}").contains("no grids for frame two"));
    assert!(!output.exists());
}
