use rav1oli_bestsource::{Frame, PlanarTarget, PlaneTarget};
use rav1oli_svt::{EncoderSettings, Planes};

use crate::analysis::{
    EncoderPlanes,
    SourceVideo,
    colour_description,
    describe_source,
    encode_first_pass,
    encoder_settings,
};
use crate::commands::options::EncoderOptions;
use crate::decode::Decoder;
use crate::decode::tests::{copy_fixture, copy_ten_bit_fixture};
use crate::metrics::FrameDescription;

fn fixture_source(bit_depth: u8) -> SourceVideo {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    SourceVideo { bit_depth, ..source }
}

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

/// Reads the 10-bit luma of `frame` without going through [EncoderPlanes].
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

    let written = encode_first_pass(&mut decoder, &source, &settings, &output).unwrap();

    assert_eq!(written, u64::from(source.frame_count));
    let encoded = Decoder::open_uncached(&output).unwrap();
    let properties = encoded.properties();
    assert_eq!(properties.num_frames, i64::from(source.frame_count));
    assert_eq!(properties.ss_mod_width, 64);
    assert_eq!(properties.ss_mod_height, 64);
}

#[test]
fn ten_bit_planes_carry_ten_bit_samples() {
    let planes = EncoderPlanes::new(&fixture_source(10));

    let (lengths, strides) = planes.lengths_and_strides();

    assert!(planes.is_ten_bit());
    assert_eq!(lengths, [64 * 64, 32 * 32, 32 * 32]);
    assert_eq!(strides, [64, 32, 32]);
}

#[test]
fn eight_bit_planes_match_the_frame_size() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let frame = decoder.frames().next().unwrap().unwrap();
    let mut planes = EncoderPlanes::new(&source);

    let exported = planes.export(&frame).unwrap();

    let Planes::Eight { y, u, v, strides } = exported else {
        panic!("expected eight bit planes");
    };
    assert_eq!(y.len(), 64 * 64);
    assert_eq!(u.len(), 32 * 32);
    assert_eq!(v.len(), 32 * 32);
    assert_eq!(strides, [64, 32, 32]);
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

    let written = encode_first_pass(&mut decoder, &source, &settings, &output).unwrap();

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

    let error = encode_first_pass(&mut decoder, &longer, &settings, &output).unwrap_err();

    assert!(error.to_string().contains("frames but the source has"));
    assert!(!output.exists());
}
