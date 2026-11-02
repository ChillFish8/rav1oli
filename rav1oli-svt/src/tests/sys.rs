const SVT_RELEASE_FLAGS: &str = include_str!(concat!(env!("OUT_DIR"), "/svt-release-flags.txt"));
#[cfg(svt_default_cpu)]
const SVT_C_FLAGS: &str = include_str!(concat!(env!("OUT_DIR"), "/svt-c-flags.txt"));
#[cfg(feature = "lto")]
const SVT_BUILD_MODE: &str = include_str!(concat!(env!("OUT_DIR"), "/svt-build-mode.txt"));

#[test]
fn the_vendored_fork_reports_its_version() {
    let version = crate::version();
    assert!(version.starts_with("v4.0.1"), "{version}");
}

#[cfg(svt_default_cpu)]
#[test]
fn the_default_build_targets_no_specific_cpu() {
    let flags = SVT_C_FLAGS;
    assert!(!flags.contains("-march"), "{flags}");
    assert!(!flags.contains("-mcpu"), "{flags}");
    assert!(!flags.contains("-mavx"), "{flags}");
}

#[test]
fn the_build_is_a_release_build() {
    let flags = SVT_RELEASE_FLAGS;
    assert!(flags.contains("NDEBUG"), "{flags}");
}

#[cfg(feature = "lto")]
#[test]
fn the_lto_build_links_and_encodes() {
    use super::encoder::{FRAMES, check_output, encode_eight_bit, mixed_grids, settings};
    use crate::Encoder;

    let build_mode = SVT_BUILD_MODE;
    assert_eq!(build_mode, "lto");

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lto.ivf");
    let mut encoder = Encoder::new(&settings(8, &[]), &path).unwrap();

    for frame in 0..FRAMES {
        let grids = mixed_grids(frame);
        encode_eight_bit(&mut encoder, frame, Some(&grids));
    }

    let summary = encoder.finish().unwrap();
    assert_eq!(summary.frames, FRAMES);
    check_output(&path);
}
