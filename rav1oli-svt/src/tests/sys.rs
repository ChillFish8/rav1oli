use std::fs;
use std::path::PathBuf;

use crate::pgo::{
    ProfileMode,
    check_rustc_profile,
    content_hash,
    profile_flags,
    profile_runtime,
    resolve_profile_mode,
};

const SVT_RELEASE_FLAGS: &str = include_str!(concat!(env!("OUT_DIR"), "/svt-release-flags.txt"));
#[cfg(any(svt_default_cpu, svt_pgo_use))]
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
    assert!(!flags.contains("-mtune"), "{flags}");
}

#[cfg(svt_default_cpu)]
#[test]
fn the_build_compiles_with_the_codegen_flags() {
    let flags = SVT_C_FLAGS;
    let words: Vec<&str> = flags.split_whitespace().collect();

    for codegen_flag in crate::cpu_flag::SVT_CODEGEN_FLAGS {
        assert!(words.contains(&codegen_flag), "{flags}");
    }
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

#[cfg(svt_pgo_use)]
#[test]
fn the_profiled_build_compiles_with_the_profile() {
    let flags = SVT_C_FLAGS;
    assert!(flags.contains("-fprofile-use="), "{flags}");
    assert!(flags.contains("-Werror=backend-plugin"), "{flags}");
    assert!(flags.contains("-DRAV1OLI_SVT_PROFILE_HASH="), "{flags}");
}

#[test]
fn a_missing_profile_fails_the_build() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing.profdata");

    let error = resolve_profile_mode(None, Some(profile.as_os_str())).unwrap_err();
    let shown_profile = profile.display().to_string();
    assert!(error.contains(&shown_profile), "{error}");
}

#[test]
fn generate_and_use_together_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("svt.profdata");
    fs::write(&profile, b"profile").unwrap();

    let raw_dir = directory.path().join("raw");
    let resolved = resolve_profile_mode(Some(raw_dir.as_os_str()), Some(profile.as_os_str()));
    let error = resolved.unwrap_err();
    assert!(error.contains("RAV1OLI_SVT_PGO_GENERATE"), "{error}");
    assert!(error.contains("RAV1OLI_SVT_PGO_PROFILE"), "{error}");
}

fn use_mode(content_hash: u64) -> ProfileMode {
    ProfileMode::Use {
        profile: PathBuf::from("svt.profdata"),
        content_hash,
    }
}

#[test]
fn a_mismatched_profile_fails() {
    let mode = use_mode(0);
    let flags = profile_flags(&mode, false);

    assert!(
        flags.contains(&"-fprofile-use=svt.profdata".to_string()),
        "{flags:?}"
    );
    assert!(flags.contains(&"-Werror=backend-plugin".to_string()), "{flags:?}");
    assert!(
        flags.contains(&"-Werror=profile-instr-out-of-date".to_string()),
        "{flags:?}"
    );
}

#[test]
fn a_readable_profile_is_used() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("svt.profdata");
    fs::write(&profile, b"profile").unwrap();

    let mode = resolve_profile_mode(None, Some(profile.as_os_str())).unwrap();
    let expected = ProfileMode::Use {
        profile,
        content_hash: content_hash(b"profile"),
    };
    assert_eq!(mode, expected);
}

#[test]
fn a_retrained_profile_changes_the_flags() {
    let first_hash = content_hash(b"first training run");
    let second_hash = content_hash(b"second training run");
    let first_flags = profile_flags(&use_mode(first_hash), false);
    let second_flags = profile_flags(&use_mode(second_hash), false);
    assert_ne!(first_flags, second_flags);

    let repeated_hash = content_hash(b"first training run");
    let repeated_flags = profile_flags(&use_mode(repeated_hash), false);
    assert_eq!(first_flags, repeated_flags);
}

#[test]
fn a_profile_that_is_a_directory_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path();

    let error = resolve_profile_mode(None, Some(profile.as_os_str())).unwrap_err();
    let shown_profile = profile.display().to_string();
    assert!(error.contains(&shown_profile), "{error}");
}

#[test]
fn relative_profile_paths_are_errors() {
    let error = resolve_profile_mode(None, Some("svt.profdata".as_ref())).unwrap_err();
    assert!(error.contains("svt.profdata"), "{error}");

    let error = resolve_profile_mode(Some("raw".as_ref()), None).unwrap_err();
    assert!(error.contains("raw"), "{error}");
}

#[test]
fn empty_variables_are_unset() {
    let mode = resolve_profile_mode(Some("".as_ref()), Some("".as_ref())).unwrap();
    assert_eq!(mode, ProfileMode::Off);
}

#[test]
fn generate_writes_into_the_raw_dir() {
    let directory = tempfile::tempdir().unwrap();
    let raw_dir = directory.path().join("raw");

    let mode = resolve_profile_mode(Some(raw_dir.as_os_str()), None).unwrap();
    assert_eq!(mode, ProfileMode::Generate(raw_dir.clone()));

    let flags = profile_flags(&mode, false);
    let expected = format!("-fprofile-generate={}", raw_dir.display());
    assert_eq!(flags, [expected]);
}

#[test]
fn clang_cl_takes_profile_flags_through_clang() {
    let mode = use_mode(0);
    let flags = profile_flags(&mode, true);

    let is_prefixed = flags.iter().all(|flag| flag.starts_with("/clang:"));
    assert!(is_prefixed, "{flags:?}");
}

#[test]
fn no_profile_mode_adds_no_flags() {
    let flags = profile_flags(&ProfileMode::Off, false);
    assert!(flags.is_empty(), "{flags:?}");
}

#[test]
fn the_profile_runtime_comes_from_the_driver_link_line() {
    let driver_output = concat!(
        " \"/usr/bin/ld\" \"-o\" \"probe\" \"/tmp/probe-1.o\" \"-u__llvm_profile_runtime\" ",
        "\"/llvm/lib/clang/22/lib/linux/libclang_rt.profile-x86_64.a\" \"-lc\"",
    );

    let runtime = profile_runtime(driver_output);
    let expected = PathBuf::from("/llvm/lib/clang/22/lib/linux/libclang_rt.profile-x86_64.a");
    assert_eq!(runtime, Some(expected));
}

#[test]
fn clang_cl_profile_runtime_unescapes_the_path() {
    let driver_output = concat!(
        " \"link.exe\" \"-out:probe.exe\" ",
        "\"-defaultlib:C:\\\\LLVM\\\\lib\\\\clang_rt.profile.lib\" \"probe.obj\"",
    );

    let runtime = profile_runtime(driver_output);
    let expected = PathBuf::from("C:\\LLVM\\lib\\clang_rt.profile.lib");
    assert_eq!(runtime, Some(expected));
}

#[test]
fn a_link_line_without_the_runtime_gives_none() {
    let runtime = profile_runtime(" \"/usr/bin/ld\" \"-o\" \"probe\" \"-lc\"");
    assert_eq!(runtime, None);
}

#[test]
fn rustc_profile_generate_with_svt_generate_is_an_error() {
    let mode = ProfileMode::Generate(PathBuf::from("raw"));
    let options = ["opt-level=3", "profile-generate=/tmp/rust"];

    let error = check_rustc_profile(&options, &mode).unwrap_err();
    assert!(error.contains("profile-generate=/tmp/rust"), "{error}");
    assert!(error.contains("__llvm_profile_"), "{error}");
}

#[test]
fn rustc_pgo_alone_is_allowed() {
    let options = ["profile-generate=/tmp/rust", "profile-use=/tmp/rust.profdata"];
    let checked = check_rustc_profile(&options, &ProfileMode::Off);
    assert_eq!(checked, Ok(()));

    let checked = check_rustc_profile(&options, &use_mode(0));
    assert_eq!(checked, Ok(()));
}

#[test]
fn rustc_profile_use_with_svt_generate_is_allowed() {
    let mode = ProfileMode::Generate(PathBuf::from("raw"));
    let options = ["profile-use=/tmp/rust.profdata"];

    let checked = check_rustc_profile(&options, &mode);
    assert_eq!(checked, Ok(()));
}
