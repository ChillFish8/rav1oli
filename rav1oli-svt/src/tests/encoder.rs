use std::fs;
use std::path::Path;

use super::ivf::read_ivf;
use crate::encoder::widen_plane;
use crate::{
    ChromaPosition,
    ColourDescription,
    CpuCap,
    DistortionGrids,
    EncodeError,
    Encoder,
    EncoderSettings,
    GridError,
    MapMode,
    ParamError,
    Planes,
};

const WIDTH: u32 = 136;
const HEIGHT: u32 = 72;
pub(super) const FRAMES: u64 = 4;
const CHROMA_WIDTH: usize = WIDTH as usize / 2;
const CHROMA_HEIGHT: usize = HEIGHT as usize / 2;

pub(super) fn settings(bit_depth: u8, svt_args: &[&str]) -> EncoderSettings {
    let svt_args = svt_args.iter().map(|arg| arg.to_string()).collect();

    EncoderSettings {
        width: WIDTH,
        height: HEIGHT,
        bit_depth,
        colour: ColourDescription {
            matrix: 1,
            transfer: 1,
            primaries: 1,
            full_range: false,
            chroma_position: ChromaPosition::Left,
        },
        fps_numerator: 30,
        fps_denominator: 1,
        preset: 12,
        crf: 40,
        svt_args,
        cpu_cap: CpuCap::Default,
        map_mode: Some(MapMode::Additive),
    }
}

/// A plane of a gradient that moves `frame` samples right each frame.
fn gradient(width: usize, height: usize, frame: u64, max: u16) -> Vec<u16> {
    let mut plane = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            let position = (row + column + frame as usize * 3) % (width + height);
            let sample = position * usize::from(max) / (width + height);
            plane.push(sample as u16);
        }
    }

    plane
}

pub(super) fn mixed_grids(frame: u64) -> DistortionGrids {
    let drops = (0..6).map(|cell| ((cell + frame) * 17 % 81) as u8).collect();
    let cuts = (0..17 * 9)
        .map(|cell| ((cell + frame) * 37 % 231) as u8)
        .collect();
    DistortionGrids::additive(WIDTH, HEIGHT, drops, cuts).unwrap()
}

/// An 8-bit version of [gradient].
fn gradient_eight(width: usize, height: usize, frame: u64, max: u8) -> Vec<u8> {
    let wide = gradient(width, height, frame, u16::from(max));
    wide.iter().map(|&sample| sample as u8).collect()
}

pub(super) fn encode_eight_bit(encoder: &mut Encoder, frame: u64, grids: Option<&DistortionGrids>) {
    let y_plane = gradient_eight(WIDTH as usize, HEIGHT as usize, frame, 255);
    let u_plane = gradient_eight(CHROMA_WIDTH, CHROMA_HEIGHT, frame, 200);
    let v_plane = gradient_eight(CHROMA_WIDTH, CHROMA_HEIGHT, frame + 5, 180);
    let planes = Planes::Eight {
        y: &y_plane,
        u: &u_plane,
        v: &v_plane,
        strides: [WIDTH as usize, CHROMA_WIDTH, CHROMA_WIDTH],
    };

    encoder.send_frame(&planes, grids).unwrap();
}

pub(super) fn check_output(path: &Path) {
    let bytes = fs::read(path).unwrap();
    let file = read_ivf(&bytes);

    assert_eq!(&file.fourcc, b"AV01");
    assert_eq!((file.width, file.height), (136, 72));
    assert_eq!((file.rate, file.scale), (30, 1));
    assert_eq!(u64::from(file.frame_count), FRAMES);
    assert_eq!(file.frames.len() as u64, FRAMES);

    for frame in &file.frames {
        assert!(frame.size > 0, "an empty packet was written");
    }
}

#[test]
fn eight_bit_frames_with_grids_encode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("eight.ivf");
    let mut encoder = Encoder::new(&settings(8, &[]), &path).unwrap();

    for frame in 0..FRAMES {
        let grids = mixed_grids(frame);
        encode_eight_bit(&mut encoder, frame, Some(&grids));
    }

    let summary = encoder.finish().unwrap();
    assert_eq!(summary.frames, FRAMES);
    check_output(&path);
}

#[test]
fn ten_bit_frames_encode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ten.ivf");
    let mut encoder = Encoder::new(&settings(10, &[]), &path).unwrap();

    for frame in 0..FRAMES {
        let grids = mixed_grids(frame);
        let y_plane = gradient(WIDTH as usize, HEIGHT as usize, frame, 1023);
        let u_plane = gradient(CHROMA_WIDTH, CHROMA_HEIGHT, frame, 800);
        let v_plane = gradient(CHROMA_WIDTH, CHROMA_HEIGHT, frame + 5, 700);
        let planes = Planes::Ten {
            y: &y_plane,
            u: &u_plane,
            v: &v_plane,
            strides: [WIDTH as usize, CHROMA_WIDTH, CHROMA_WIDTH],
        };

        encoder.send_frame(&planes, Some(&grids)).unwrap();
    }

    let summary = encoder.finish().unwrap();
    assert_eq!(summary.frames, FRAMES);
    check_output(&path);
}

#[test]
fn frames_without_grids_encode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plain.ivf");
    let mut encoder = Encoder::new(&settings(8, &[]), &path).unwrap();

    for frame in 0..FRAMES {
        encode_eight_bit(&mut encoder, frame, None);
    }

    encoder.finish().unwrap();
    check_output(&path);
}

#[test]
fn widening_shifts_eight_bit_samples() {
    let plane = [0, 1, 255, 99];
    let mut widened = Vec::new();

    widen_plane(&plane, 4, 3, 1, &mut widened);

    assert_eq!(widened, [0, 4, 1020]);
}

#[test]
fn four_two_two_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("422.ivf");
    let mut encoder = Encoder::new(&settings(8, &[]), &path).unwrap();

    let y_plane = vec![0; WIDTH as usize * HEIGHT as usize];
    let chroma_plane = vec![0; CHROMA_WIDTH * HEIGHT as usize];
    let planes = Planes::Eight {
        y: &y_plane,
        u: &chroma_plane,
        v: &chroma_plane,
        strides: [WIDTH as usize, CHROMA_WIDTH, CHROMA_WIDTH],
    };

    let error = encoder.send_frame(&planes, None).unwrap_err();
    let is_expected = matches!(error, EncodeError::UnsupportedFormat { .. });
    assert!(is_expected, "{error:?}");
}

#[test]
fn ten_bit_planes_for_an_eight_bit_encoder_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mismatch.ivf");
    let mut encoder = Encoder::new(&settings(8, &[]), &path).unwrap();

    let y_plane = vec![0; WIDTH as usize * HEIGHT as usize];
    let chroma_plane = vec![0; CHROMA_WIDTH * CHROMA_HEIGHT];
    let planes = Planes::Ten {
        y: &y_plane,
        u: &chroma_plane,
        v: &chroma_plane,
        strides: [WIDTH as usize, CHROMA_WIDTH, CHROMA_WIDTH],
    };

    let error = encoder.send_frame(&planes, None).unwrap_err();
    let is_expected = matches!(error, EncodeError::UnsupportedFormat { .. });
    assert!(is_expected, "{error:?}");
}

#[test]
fn grids_for_another_frame_size_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("grids.ivf");
    let mut encoder = Encoder::new(&settings(8, &[]), &path).unwrap();

    let grids = DistortionGrids::additive(64, 64, vec![0], vec![0; 64]).unwrap();
    let y_plane = vec![0; WIDTH as usize * HEIGHT as usize];
    let chroma_plane = vec![0; CHROMA_WIDTH * CHROMA_HEIGHT];
    let planes = Planes::Eight {
        y: &y_plane,
        u: &chroma_plane,
        v: &chroma_plane,
        strides: [WIDTH as usize, CHROMA_WIDTH, CHROMA_WIDTH],
    };

    let error = encoder.send_frame(&planes, Some(&grids)).unwrap_err();
    let is_expected = matches!(error, EncodeError::Grid(_));
    assert!(is_expected, "{error:?}");
}

#[test]
fn a_twelve_bit_depth_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("twelve.ivf");

    let error = Encoder::new(&settings(12, &[]), &path).unwrap_err();
    let is_expected = matches!(error, EncodeError::UnsupportedFormat { .. });
    assert!(is_expected, "{error:?}");
}

#[test]
fn an_svt_flag_is_applied() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tune.ivf");
    let mut encoder = Encoder::new(&settings(8, &["--tune", "0"]), &path).unwrap();

    for frame in 0..FRAMES {
        encode_eight_bit(&mut encoder, frame, None);
    }

    encoder.finish().unwrap();
    check_output(&path);

    let aq_path = directory.path().join("aq.ivf");
    let error = Encoder::new(&settings(8, &["--aq-mode", "1"]), &aq_path).unwrap_err();
    let is_requirement = matches!(error, EncodeError::Param(ParamError::Requirement { .. }));
    assert!(is_requirement, "{error:?}");
}

#[test]
fn a_config_svt_rejects_leaves_no_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rejected.ivf");

    let error = Encoder::new(&settings(8, &["--buf-sz", "5"]), &path).unwrap_err();
    let is_expected = matches!(error, EncodeError::Svt { .. });
    assert!(is_expected, "{error:?}");
    assert!(!path.exists());
}

#[test]
fn an_encoder_dropped_without_finish_is_released() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dropped.ivf");
    let mut encoder = Encoder::new(&settings(8, &[]), &path).unwrap();

    encode_eight_bit(&mut encoder, 0, None);
    drop(encoder);
}

#[test]
fn a_ten_bit_sample_above_1023_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("p010.ivf");
    let mut encoder = Encoder::new(&settings(10, &[]), &path).unwrap();

    let y_plane = vec![0; WIDTH as usize * HEIGHT as usize];
    let mut u_plane = vec![0; CHROMA_WIDTH * CHROMA_HEIGHT];
    let v_plane = vec![0; CHROMA_WIDTH * CHROMA_HEIGHT];
    u_plane[2 * CHROMA_WIDTH + 5] = 1024;
    let planes = Planes::Ten {
        y: &y_plane,
        u: &u_plane,
        v: &v_plane,
        strides: [WIDTH as usize, CHROMA_WIDTH, CHROMA_WIDTH],
    };

    let error = encoder.send_frame(&planes, None).unwrap_err();
    let message = error.to_string();

    let is_expected = matches!(error, EncodeError::UnsupportedFormat { .. });
    assert!(is_expected, "{error:?}");
    assert!(message.contains("U plane"), "{message}");
    assert!(message.contains("row 2, column 5"), "{message}");
}

#[test]
fn an_output_in_a_missing_directory_is_an_io_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing").join("out.ivf");

    let error = Encoder::new(&settings(8, &[]), &path).unwrap_err();
    let message = error.to_string();

    let is_expected = matches!(error, EncodeError::Io { .. });
    assert!(is_expected, "{error:?}");
    assert!(message.contains("out.ivf"), "{message}");
}

fn colour_error(matrix: u8, transfer: u8, primaries: u8) -> EncodeError {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("colour.ivf");
    let mut colour_settings = settings(8, &[]);
    colour_settings.colour.matrix = matrix;
    colour_settings.colour.transfer = transfer;
    colour_settings.colour.primaries = primaries;

    Encoder::new(&colour_settings, &path).unwrap_err()
}

#[test]
fn a_reserved_matrix_code_is_rejected() {
    let error = colour_error(3, 1, 1);
    let is_expected = matches!(
        error,
        EncodeError::InvalidColourCode {
            field: "matrix",
            code: 3,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
}

#[test]
fn a_reserved_transfer_code_is_rejected() {
    let error = colour_error(1, 19, 1);
    let is_expected = matches!(
        error,
        EncodeError::InvalidColourCode {
            field: "transfer",
            code: 19,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
}

#[test]
fn a_reserved_primaries_code_is_rejected() {
    let error = colour_error(1, 1, 13);
    let is_expected = matches!(
        error,
        EncodeError::InvalidColourCode {
            field: "primaries",
            code: 13,
            ..
        }
    );
    assert!(is_expected, "{error:?}");
}

#[test]
fn odd_dimensions_with_grids_encode() {
    let width = 135;
    let height = 71;
    let chroma_width = 68;
    let chroma_height = 36;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("odd.ivf");
    let mut odd_settings = settings(8, &[]);
    odd_settings.width = width as u32;
    odd_settings.height = height as u32;
    let mut encoder = Encoder::new(&odd_settings, &path).unwrap();

    for frame in 0..FRAMES {
        let drops = vec![(frame * 20) as u8; 3 * 2];
        let cuts = vec![(frame * 50) as u8; 17 * 9];
        let grids = DistortionGrids::additive(width as u32, height as u32, drops, cuts).unwrap();

        let y_plane = gradient_eight(width, height, frame, 255);
        let u_plane = gradient_eight(chroma_width, chroma_height, frame, 200);
        let v_plane = gradient_eight(chroma_width, chroma_height, frame + 5, 180);
        let planes = Planes::Eight {
            y: &y_plane,
            u: &u_plane,
            v: &v_plane,
            strides: [width, chroma_width, chroma_width],
        };

        encoder.send_frame(&planes, Some(&grids)).unwrap();
    }

    let summary = encoder.finish().unwrap();
    assert_eq!(summary.frames, FRAMES);

    let bytes = fs::read(&path).unwrap();
    let file = read_ivf(&bytes);
    assert_eq!((file.width, file.height), (135, 71));
    assert_eq!(file.frames.len() as u64, FRAMES);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn an_avx2_cap_encodes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("avx2.ivf");
    let capped = settings(8, &[]).with_cpu_cap(CpuCap::Avx2);
    let mut encoder = Encoder::new(&capped, &path).unwrap();

    for frame in 0..FRAMES {
        let grids = mixed_grids(frame);
        encode_eight_bit(&mut encoder, frame, Some(&grids));
    }

    encoder.finish().unwrap();
    check_output(&path);
}

#[cfg(not(target_arch = "x86_64"))]
#[test]
fn an_avx2_cap_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("avx2.ivf");
    let capped = settings(8, &[]).with_cpu_cap(CpuCap::Avx2);

    let error = Encoder::new(&capped, &path).unwrap_err();
    let is_requirement = matches!(error, EncodeError::Param(ParamError::Requirement { .. }));
    assert!(is_requirement, "{error:?}");
}

#[test]
fn asm_cannot_be_passed_as_an_svt_flag() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("asm.ivf");

    let asm_settings = settings(8, &["--asm", "avx2"]);

    let error = Encoder::new(&asm_settings, &path).unwrap_err();
    let is_encoder_owned = matches!(error, EncodeError::Param(ParamError::EncoderOwned { .. }));
    assert!(is_encoder_owned, "{error:?}");
}

fn redistribute_settings(svt_args: &[&str]) -> EncoderSettings {
    let base = settings(8, svt_args);
    EncoderSettings {
        map_mode: Some(MapMode::Redistribute),
        ..base
    }
}

fn mixed_redistribute_grids(frame: u64) -> DistortionGrids {
    let offsets = (0..6)
        .map(|cell| ((cell + frame) * 17 % 121) as i8 - 80)
        .collect();
    let weights = (0..17 * 9)
        .map(|cell| ((cell + frame) * 37 % 256) as u8 as i8)
        .collect();
    DistortionGrids::redistribute(WIDTH, HEIGHT, offsets, weights).unwrap()
}

#[test]
fn redistribute_grids_encode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("redistribute.ivf");
    let settings = redistribute_settings(&[]);
    let mut encoder = Encoder::new(&settings, &path).unwrap();

    for frame in 0..FRAMES {
        let grids = mixed_redistribute_grids(frame);
        encode_eight_bit(&mut encoder, frame, Some(&grids));
    }

    let summary = encoder.finish().unwrap();
    assert_eq!(summary.frames, FRAMES);
    check_output(&path);
}

#[test]
fn additive_grids_are_rejected_in_redistribute_mode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mismatch.ivf");
    let settings = redistribute_settings(&[]);
    let mut encoder = Encoder::new(&settings, &path).unwrap();

    let grids = mixed_grids(0);
    let y_plane = vec![0; WIDTH as usize * HEIGHT as usize];
    let chroma_plane = vec![0; CHROMA_WIDTH * CHROMA_HEIGHT];
    let planes = Planes::Eight {
        y: &y_plane,
        u: &chroma_plane,
        v: &chroma_plane,
        strides: [WIDTH as usize, CHROMA_WIDTH, CHROMA_WIDTH],
    };

    let error = encoder.send_frame(&planes, Some(&grids)).unwrap_err();
    let is_expected = matches!(
        error,
        EncodeError::Grid(GridError::ModeMismatch {
            grid_mode: MapMode::Additive,
            encoder_mode: MapMode::Redistribute,
        })
    );
    assert!(is_expected, "{error:?}");
}

#[test]
fn a_map_mode_flag_that_disagrees_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("conflict.ivf");
    let settings = redistribute_settings(&["--distortion-map-mode", "0"]);

    let error = Encoder::new(&settings, &path).unwrap_err();
    let message = error.to_string();

    let is_expected = matches!(
        error,
        EncodeError::Param(ParamError::MapModeConflict { flag_value: 0, .. })
    );
    assert!(is_expected, "{error:?}");
    assert!(message.contains("redistribute"), "{message}");
}

#[test]
fn a_map_mode_flag_that_agrees_is_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agree.ivf");
    let settings = redistribute_settings(&["--distortion-map-mode", "1", "--distortion-map-max-raise", "12"]);

    Encoder::new(&settings, &path).unwrap();
}
