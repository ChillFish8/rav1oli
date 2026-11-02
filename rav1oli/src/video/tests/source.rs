use rav1oli_bestsource::{ColorFamily, VideoFormat, VideoProperties};
use rav1oli_svt::{ChromaPosition, ColourDescription, CpuCap};
use rav1oli_table::FrameRate;

use crate::commands::options::EncoderOptions;
use crate::metrics::FrameDescription;
use crate::video::{SourceVideo, colour_description, describe_source, encoder_settings};

fn format(bits: i32) -> VideoFormat {
    VideoFormat {
        color_family: ColorFamily::Yuv,
        alpha: false,
        float: false,
        bits,
        sub_sampling_w: 1,
        sub_sampling_h: 1,
    }
}

fn properties(format: VideoFormat, width: i32, height: i32) -> VideoProperties {
    VideoProperties {
        num_frames: 48,
        fps: (24000, 1001),
        time_base: (1, 1000),
        width,
        height,
        ss_mod_width: width,
        ss_mod_height: height,
        sar: (1, 1),
        format,
    }
}

fn frame() -> FrameDescription {
    FrameDescription {
        width: 64,
        height: 64,
        encoded_height: 64,
        family: ColorFamily::Yuv,
        bits: 8,
        float: false,
        sub_sampling_w: 1,
        sub_sampling_h: 1,
        matrix: 1,
        transfer: 1,
        primaries: 1,
        range: 1,
        chroma_location: 1,
    }
}

fn source(width: u32, height: u32) -> SourceVideo {
    SourceVideo {
        width,
        height,
        bit_depth: 10,
        frame_count: 48,
        frame_rate: FrameRate {
            numerator: 24000,
            denominator: 1001,
        },
    }
}

fn encoder_options(svt_args: &[&str]) -> EncoderOptions {
    EncoderOptions {
        preset: 6,
        crf: 28,
        svt_args: svt_args.iter().map(|arg| arg.to_string()).collect(),
    }
}

fn error_message(properties: &VideoProperties) -> String {
    let error = describe_source(properties).expect_err("the source should be rejected");

    format!("{error:#}")
}

#[test]
fn describe_source_accepts_eight_and_ten_bit_420() {
    for bits in [8, 10] {
        let described = describe_source(&properties(format(bits), 64, 64)).unwrap();

        assert_eq!(i32::from(described.bit_depth), bits);
    }
}

#[test]
fn describe_source_uses_the_ss_mod_size() {
    let mut odd = properties(format(8), 65, 63);
    odd.ss_mod_width = 64;
    odd.ss_mod_height = 62;

    let described = describe_source(&odd).unwrap();

    assert_eq!(described.width, 64);
    assert_eq!(described.height, 62);
}

#[test]
fn describe_source_rejects_444() {
    let mut found = format(8);
    found.sub_sampling_w = 0;
    found.sub_sampling_h = 0;

    assert!(error_message(&properties(found, 64, 64)).contains("4:4:4"));
}

#[test]
fn describe_source_rejects_twelve_bit() {
    assert!(error_message(&properties(format(12), 64, 64)).contains("12-bit"));
}

#[test]
fn describe_source_rejects_alpha() {
    let mut found = format(8);
    found.alpha = true;

    assert!(error_message(&properties(found, 64, 64)).contains("alpha"));
}

#[test]
fn describe_source_rejects_float() {
    let mut found = format(32);
    found.float = true;

    assert!(error_message(&properties(found, 64, 64)).contains("floating point"));
}

#[test]
fn describe_source_rejects_rgb() {
    let mut found = format(8);
    found.color_family = ColorFamily::Rgb;

    assert!(error_message(&properties(found, 64, 64)).contains("RGB"));
}

#[test]
fn describe_source_rejects_zero_frames() {
    let mut empty = properties(format(8), 64, 64);
    empty.num_frames = 0;

    assert_eq!(error_message(&empty), "the source has no frames");
}

#[test]
fn describe_source_rejects_a_zero_frame_rate_denominator() {
    let mut stalled = properties(format(8), 64, 64);
    stalled.fps = (24000, 0);

    assert_eq!(
        error_message(&stalled),
        "the source frame rate denominator 0 is invalid"
    );
}

#[test]
fn describe_source_rejects_a_zero_ss_mod_size() {
    let mut empty = properties(format(8), 64, 64);
    empty.ss_mod_height = 0;

    assert_eq!(error_message(&empty), "the source size 64x0 is invalid");
}

#[test]
fn frame_rate_is_carried_over() {
    let described = describe_source(&properties(format(8), 64, 64)).unwrap();

    assert_eq!(
        described.frame_rate,
        FrameRate {
            numerator: 24000,
            denominator: 1001
        }
    );
}

#[test]
fn range_codes_map_to_full_range() {
    for (range, expected) in [(0, false), (1, false), (2, true)] {
        let description = FrameDescription { range, ..frame() };

        let colour = colour_description(&description).unwrap();

        assert_eq!(colour.full_range, expected);
    }
}

#[test]
fn chroma_locations_map_to_positions() {
    let expected = [
        (0, ChromaPosition::Left),
        (1, ChromaPosition::Left),
        (2, ChromaPosition::Center),
        (3, ChromaPosition::TopLeft),
        (4, ChromaPosition::Top),
    ];

    for (chroma_location, position) in expected {
        let description = FrameDescription {
            chroma_location,
            ..frame()
        };

        let colour = colour_description(&description).unwrap();

        assert_eq!(colour.chroma_position, position);
    }

    for chroma_location in [5, 6] {
        let description = FrameDescription {
            chroma_location,
            ..frame()
        };

        let error = colour_description(&description).unwrap_err();

        assert!(error.to_string().contains("bottom"));
    }
}

#[test]
fn colour_codes_outside_a_byte_are_rejected() {
    for matrix in [-1, 256] {
        let description = FrameDescription { matrix, ..frame() };

        assert!(colour_description(&description).is_err());
    }
}

#[test]
fn encoder_settings_take_options_and_source() {
    let colour = ColourDescription {
        matrix: 1,
        transfer: 1,
        primaries: 1,
        full_range: false,
        chroma_position: ChromaPosition::Left,
    };
    let options = encoder_options(&["--tune", "0"]);

    let settings = encoder_settings(&source(64, 62), colour, &options);

    assert_eq!((settings.width, settings.height), (64, 62));
    assert_eq!(settings.bit_depth, 10);
    assert_eq!(settings.colour, colour);
    assert_eq!(settings.fps_numerator, 24000);
    assert_eq!(settings.fps_denominator, 1001);
    assert_eq!(settings.preset, 6);
    assert_eq!(settings.crf, 28);
    assert_eq!(settings.svt_args, ["--tune", "0"]);
    assert_eq!(settings.cpu_cap, CpuCap::Default);
}
