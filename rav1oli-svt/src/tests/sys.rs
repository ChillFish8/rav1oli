#[cfg(svt_default_cpu)]
const SVT_C_FLAGS: &str = include_str!(concat!(env!("OUT_DIR"), "/svt-c-flags.txt"));

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
