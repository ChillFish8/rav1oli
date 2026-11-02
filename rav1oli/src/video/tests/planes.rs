use rav1oli_svt::Planes;

use crate::decode::Decoder;
use crate::decode::tests::copy_fixture;
use crate::video::planes::EncoderPlanes;
use crate::video::{SourceVideo, describe_source};

fn fixture_source(bit_depth: u8) -> SourceVideo {
    let dir = tempfile::tempdir().unwrap();
    let video = copy_fixture(dir.path());
    let decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    SourceVideo { bit_depth, ..source }
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
