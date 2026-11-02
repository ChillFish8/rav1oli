use rav1oli_bestsource::ColorFamily;
use rav1oli_vship::{ChromaLocation, Range, SampleType};

use crate::decode::tests::copy_fixture;
use crate::decode::{CacheLocation, Decoder};
use crate::metrics::{FrameDescription, PlaneBuffers, check_compatible, colour_space};

fn yuv420(bits: i32) -> FrameDescription {
    FrameDescription {
        width: 64,
        height: 64,
        family: ColorFamily::Yuv,
        bits,
        float: false,
        sub_sampling_w: 1,
        sub_sampling_h: 1,
        matrix: 1,
        transfer: 1,
        primaries: 1,
        range: 0,
        chroma_location: 0,
    }
}

#[test]
fn untagged_sd_yuv_uses_bt470bg() {
    let description = FrameDescription {
        width: 720,
        height: 576,
        matrix: 2,
        transfer: 2,
        primaries: 2,
        range: 0,
        chroma_location: 0,
        ..yuv420(8)
    };

    let space = colour_space(&description).unwrap();

    assert_eq!(space.matrix, 5);
    assert_eq!(space.transfer, 5);
    assert_eq!(space.primaries, 5);
    assert_eq!(space.range, Range::Limited);
    assert_eq!(space.chroma_location, ChromaLocation::Left);
}

#[test]
fn untagged_hd_yuv_uses_bt709() {
    let description = FrameDescription {
        width: 1920,
        height: 1080,
        matrix: 2,
        transfer: 2,
        primaries: 2,
        ..yuv420(8)
    };

    let space = colour_space(&description).unwrap();

    assert_eq!(space.matrix, 1);
    assert_eq!(space.transfer, 1);
    assert_eq!(space.primaries, 1);
}

#[test]
fn untagged_bt2020_defaults_to_pq() {
    let description = FrameDescription {
        matrix: 9,
        transfer: 2,
        primaries: 2,
        ..yuv420(10)
    };

    let space = colour_space(&description).unwrap();

    assert_eq!(space.transfer, 16);
    assert_eq!(space.primaries, 9);
}

#[test]
fn tagged_transfer_wins_over_matrix_defaults() {
    let description = FrameDescription {
        matrix: 9,
        transfer: 1,
        primaries: 2,
        ..yuv420(10)
    };

    let space = colour_space(&description).unwrap();

    assert_eq!(space.transfer, 1);
    assert_eq!(space.primaries, 9);
}

#[test]
fn untagged_rgb_uses_srgb_and_full_range() {
    let description = FrameDescription {
        family: ColorFamily::Rgb,
        matrix: 0,
        transfer: 2,
        primaries: 2,
        range: 0,
        sub_sampling_w: 0,
        sub_sampling_h: 0,
        ..yuv420(8)
    };

    let space = colour_space(&description).unwrap();

    assert_eq!(space.transfer, 13);
    assert_eq!(space.primaries, 1);
    assert_eq!(space.range, Range::Full);
}

#[test]
fn an_invalid_range_is_an_error() {
    let description = FrameDescription {
        range: 3,
        ..yuv420(8)
    };

    let error = colour_space(&description).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("range 3"), "{message}");
}

#[test]
fn explicit_colour_is_passed_through() {
    let description = FrameDescription {
        matrix: 9,
        transfer: 16,
        primaries: 9,
        range: 2,
        chroma_location: 3,
        ..yuv420(10)
    };

    let space = colour_space(&description).unwrap();

    assert_eq!(space.matrix, 9);
    assert_eq!(space.transfer, 16);
    assert_eq!(space.primaries, 9);
    assert_eq!(space.range, Range::Full);
    assert_eq!(space.chroma_location, ChromaLocation::TopLeft);
}

#[test]
fn chroma_locations_follow_ffmpeg_values() {
    let locations = [
        (1, ChromaLocation::Left),
        (2, ChromaLocation::Center),
        (3, ChromaLocation::TopLeft),
        (4, ChromaLocation::Top),
        (0, ChromaLocation::Left),
    ];

    for (value, expected) in locations {
        let description = FrameDescription {
            chroma_location: value,
            ..yuv420(8)
        };
        let space = colour_space(&description).unwrap();
        assert_eq!(space.chroma_location, expected, "value {value}");
    }
}

#[test]
fn ten_bit_yuv_maps_to_uint10() {
    let space = colour_space(&yuv420(10)).unwrap();

    assert_eq!(space.sample, SampleType::Uint10);
    assert_eq!(space.width, 64);
    assert_eq!(space.sub_sampling_w, 1);
}

#[test]
fn sample_types_follow_bit_depth_and_float() {
    let integers = [
        (8, SampleType::Uint8),
        (9, SampleType::Uint9),
        (12, SampleType::Uint12),
        (14, SampleType::Uint14),
        (16, SampleType::Uint16),
    ];
    for (bits, expected) in integers {
        let space = colour_space(&yuv420(bits)).unwrap();
        assert_eq!(space.sample, expected, "{bits} bit");
    }

    let half = FrameDescription {
        float: true,
        ..yuv420(16)
    };
    let float = FrameDescription {
        float: true,
        ..yuv420(32)
    };
    let half_sample = colour_space(&half).unwrap().sample;
    let float_sample = colour_space(&float).unwrap().sample;
    assert_eq!(half_sample, SampleType::Half);
    assert_eq!(float_sample, SampleType::Float);
}

#[test]
fn unsupported_bit_depths_are_errors() {
    let integer_eleven = yuv420(11);
    let float_eight = FrameDescription {
        float: true,
        ..yuv420(8)
    };

    assert!(colour_space(&integer_eleven).is_err());
    assert!(colour_space(&float_eight).is_err());
}

#[test]
fn rgb_uses_matrix_zero() {
    let description = FrameDescription {
        family: ColorFamily::Rgb,
        matrix: 1,
        sub_sampling_w: 0,
        sub_sampling_h: 0,
        ..yuv420(8)
    };

    let space = colour_space(&description).unwrap();

    assert_eq!(space.matrix, 0);
}

#[test]
fn gray_is_unsupported() {
    let description = FrameDescription {
        family: ColorFamily::Gray,
        ..yuv420(8)
    };

    let error = colour_space(&description).unwrap_err();

    assert!(error.to_string().contains("Gray"), "{error}");
}

#[test]
fn bayer_and_unknown_are_unsupported() {
    let bayer = FrameDescription {
        family: ColorFamily::Bayer,
        ..yuv420(8)
    };
    let unknown = FrameDescription {
        family: ColorFamily::Unknown,
        ..yuv420(8)
    };

    assert!(colour_space(&bayer).is_err());
    assert!(colour_space(&unknown).is_err());
}

#[test]
fn different_bit_depths_are_compatible() {
    let result = check_compatible(&yuv420(8), &yuv420(10));

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn different_sizes_are_incompatible() {
    let wider = FrameDescription {
        width: 128,
        ..yuv420(8)
    };

    let error = check_compatible(&yuv420(8), &wider).unwrap_err();

    assert!(error.to_string().contains("size"), "{error}");
}

#[test]
fn different_subsampling_is_incompatible() {
    let full_chroma = FrameDescription {
        sub_sampling_w: 0,
        sub_sampling_h: 0,
        ..yuv420(8)
    };

    let error = check_compatible(&yuv420(8), &full_chroma).unwrap_err();

    assert!(error.to_string().contains("subsampling"), "{error}");
}

#[test]
fn different_families_are_incompatible() {
    let rgb = FrameDescription {
        family: ColorFamily::Rgb,
        ..yuv420(8)
    };

    let error = check_compatible(&yuv420(8), &rgb).unwrap_err();

    assert!(error.to_string().contains("colour family"), "{error}");
}

#[test]
fn buffers_are_sized_from_the_subsampling() {
    let buffers = PlaneBuffers::new(&yuv420(10)).unwrap();

    let planes = buffers.planes();

    assert_eq!(planes.strides, [128, 64, 64]);
    assert_eq!(planes.data[0].len(), 128 * 64);
    assert_eq!(planes.data[1].len(), 64 * 32);
    assert_eq!(planes.data[2].len(), 64 * 32);
}

#[test]
fn rgb_buffers_are_all_full_size() {
    let description = FrameDescription {
        family: ColorFamily::Rgb,
        sub_sampling_w: 0,
        sub_sampling_h: 0,
        ..yuv420(8)
    };

    let buffers = PlaneBuffers::new(&description).unwrap();

    assert_eq!(buffers.planes().strides, [64, 64, 64]);
}

#[test]
fn the_fixture_frame_exports_into_buffers() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let mut decoder = Decoder::open(&video, &CacheLocation::BesideVideo).unwrap();
    let frame = decoder.frames().next().unwrap().unwrap();
    let description = FrameDescription::from_frame(&frame);
    let mut buffers = PlaneBuffers::new(&description).unwrap();

    buffers.export(&frame).unwrap();

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected_path = manifest.join("../rav1oli-bestsource/tests/media/yuv420p-frame0.yuv");
    let expected = std::fs::read(expected_path).unwrap();
    let planes = buffers.planes();
    assert_eq!(planes.strides, [64, 32, 32]);
    assert_eq!(planes.data[0], &expected[..4096]);
    assert_eq!(planes.data[1], &expected[4096..5120]);
    assert_eq!(planes.data[2], &expected[5120..6144]);
}

#[test]
fn a_frame_of_another_layout_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let mut decoder = Decoder::open(&video, &CacheLocation::BesideVideo).unwrap();
    let frame = decoder.frames().next().unwrap().unwrap();
    let mut buffers = PlaneBuffers::new(&yuv420(10)).unwrap();

    let error = buffers.export(&frame).unwrap_err();

    assert!(error.to_string().contains("bit depth"), "{error}");
}

#[test]
fn unsupported_matrix_is_an_error() {
    let description = FrameDescription {
        matrix: 4,
        ..yuv420(8)
    };

    let error = colour_space(&description).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("matrix 4"), "{message}");
}

#[test]
fn unsupported_transfer_is_an_error() {
    let description = FrameDescription {
        transfer: 11,
        ..yuv420(8)
    };

    let error = colour_space(&description).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("transfer 11"), "{message}");
}

#[test]
fn unsupported_primaries_is_an_error() {
    let description = FrameDescription {
        primaries: 10,
        ..yuv420(8)
    };

    let error = colour_space(&description).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("primaries 10"), "{message}");
}

#[test]
fn bt2020_transfer_maps_to_bt709() {
    for value in [14, 15] {
        let description = FrameDescription {
            transfer: value,
            ..yuv420(10)
        };
        let space = colour_space(&description).unwrap();
        assert_eq!(space.transfer, 1, "value {value}");
    }
}

#[test]
fn bottom_chroma_sitings_are_errors() {
    for value in [5, 6] {
        let description = FrameDescription {
            chroma_location: value,
            ..yuv420(8)
        };
        let error = colour_space(&description).unwrap_err();
        let message = error.to_string();
        assert!(message.contains(&value.to_string()), "{message}");
    }
}

#[test]
fn invalid_chroma_location_is_an_error() {
    let description = FrameDescription {
        chroma_location: 7,
        ..yuv420(8)
    };

    let error = colour_space(&description).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("chroma location 7"), "{message}");
}

#[test]
fn yuv_with_rgb_matrix_is_an_error() {
    let description = FrameDescription {
        matrix: 0,
        ..yuv420(8)
    };

    let error = colour_space(&description).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("matrix 0"), "{message}");
}
