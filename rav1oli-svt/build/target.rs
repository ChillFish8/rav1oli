use std::env;
use std::path::PathBuf;

use crate::cpu;

pub fn out_dir() -> PathBuf {
    let out_dir = env::var_os("OUT_DIR");
    let Some(out_dir) = out_dir else {
        panic!("cargo did not set OUT_DIR");
    };

    PathBuf::from(out_dir)
}

pub fn target_arch() -> String {
    env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default()
}

pub fn target_os() -> String {
    env::var("CARGO_CFG_TARGET_OS").unwrap_or_default()
}

pub fn is_unix_target() -> bool {
    let target_family = env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();

    target_family.split(',').any(|family| family == "unix")
}

/// Whether the target uses the MSVC ABI, which SVT builds with clang-cl.
pub fn is_msvc_target() -> bool {
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    target_env == "msvc"
}

/// Whether the target links the C runtime statically.
pub fn is_crt_static() -> bool {
    let target_features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();

    target_features.split(',').any(|feature| feature == "crt-static")
}

/// The C flags for the user's `-C target-cpu`, empty when they set none.
pub fn target_cpu_flags() -> Vec<String> {
    let rustflags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let target_arch = target_arch();
    let is_clang_cl = is_msvc_target();

    cpu::cpu_flags_for(&rustflags, &target_arch, is_clang_cl)
}

/// The file name of a static library as the target's linker looks it up.
pub fn static_library_name(name: &str) -> String {
    if is_msvc_target() {
        return format!("{name}.lib");
    }

    format!("lib{name}.a")
}
