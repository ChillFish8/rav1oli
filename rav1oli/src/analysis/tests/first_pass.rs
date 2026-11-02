use std::path::Path;

use rav1oli_svt::Planes;

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
use crate::decode::tests::copy_fixture;
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

#[test]
fn the_fixture_encodes_with_the_same_frame_count() {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let output = dir.path().join("first.ivf");
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let first_frame = decoder.frames().next().unwrap().unwrap();
    let description = FrameDescription::from_frame(&first_frame);
    let colour = colour_description(&description).unwrap();
    let settings = encoder_settings(&source, colour, &encoder_options());

    let written = encode_first_pass(&mut decoder, &source, &settings, &output).unwrap();

    assert_eq!(written, u64::from(source.frame_count));
    let encoded = Decoder::open_uncached(Path::new(&output)).unwrap();
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
