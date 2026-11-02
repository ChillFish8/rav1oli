use std::fs;
use std::path::{Path, PathBuf};

use crate::{simd, target};

/// The fork's source tree, checked out as a submodule.
pub const SVT_SOURCE: &str = "vendor/svt-av1";

/// The tools and flags one SVT build uses.
pub struct SvtBuild<'a> {
    pub clang: &'a Path,
    pub nasm: Option<&'a Path>,
    /// The codegen and CPU flags from [svt_c_flags](crate::cpu::svt_c_flags).
    pub c_flags: &'a [String],
    pub target_arch: &'a str,
    pub is_msvc: bool,
    pub is_lto: bool,
    pub profile_flags: &'a [String],
    /// Whether SVT's warnings stay on, so the profile errors in `profile_flags` can fire.
    pub keeps_warnings: bool,
}

/// Drops the flag the cmake crate adds to silence warnings, which also silences warnings made errors.
const KEEP_WARNINGS_SCRIPT: &str = r#"foreach(flags_variable CMAKE_C_FLAGS CMAKE_CXX_FLAGS)
    string(REGEX REPLACE "(^| )-(w|W0)( |$)" " " ${flags_variable} "${${flags_variable}}")
endforeach()
"#;

/// The project include for builds that keep the cmake crate's flags as they are.
const NO_CHANGE_SCRIPT: &str = "# The flags are left as the cmake crate set them.\n";

/// A target that builds nothing, so a build of it only configures SVT.
const CONFIGURE_TARGET: &str = "rav1oli_configure";

/// Adds [CONFIGURE_TARGET](crate::library::CONFIGURE_TARGET) to every project include.
const CONFIGURE_TARGET_SCRIPT: &str = "add_custom_target(rav1oli_configure)\n";

/// Configures, builds and installs the static SVT library, returning the install dir.
pub fn build_svt(svt_build: &SvtBuild) -> PathBuf {
    let c_config = c_config(svt_build.clang);

    let mut cxx_config = cc::Build::new();
    cxx_config.cpp(true).compiler(svt_build.clang);

    // The fork writes its binaries into its source tree by default, which every build would share.
    let out_dir = target::out_dir();
    let binary_dir = out_dir.join("build").join("Bin");

    let mut config = cmake::Config::new(SVT_SOURCE);
    // Always Release, as a Debug build turns on SVT's allocation tracker which our frees would bypass.
    config
        .profile("Release")
        .init_c_cfg(c_config)
        .init_cxx_cfg(cxx_config)
        .define("CMAKE_C_COMPILER", svt_build.clang)
        .define("CMAKE_CXX_COMPILER", svt_build.clang)
        .define("CMAKE_OUTPUT_DIRECTORY", &binary_dir)
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_APPS", "OFF")
        .define("BUILD_TESTING", "OFF")
        .define("NATIVE", "OFF")
        .define("SVT_AV1_LTO", "OFF")
        .define("SVT_AV1_PGO", "OFF");

    if let Some(nasm) = svt_build.nasm {
        config.define("CMAKE_ASM_NASM_COMPILER", nasm);
    }

    if svt_build.target_arch == "x86_64" {
        config.define("ENABLE_AVX512", "ON");
    }

    for flag in svt_build.c_flags {
        config.cflag(flag).cxxflag(flag);
    }

    if svt_build.is_lto {
        config.cflag("-flto").cxxflag("-flto");
    }

    for flag in svt_build.profile_flags {
        config.cflag(flag).cxxflag(flag);
    }

    // CMake runs the script straight after SVT's `project()`, before any target reads the flags. It is
    // always set, as CMake would otherwise keep a profile-use build's script in its cache.
    let project_script = if svt_build.keeps_warnings {
        KEEP_WARNINGS_SCRIPT
    } else {
        NO_CHANGE_SCRIPT
    };
    let script_path = out_dir.join("project-include.cmake");
    let script = format!("{project_script}{CONFIGURE_TARGET_SCRIPT}");
    write_text(&script_path, &script);
    config.define("CMAKE_PROJECT_INCLUDE", &script_path);

    // The Visual Studio generator ignores `CMAKE_C_COMPILER`, so clang-cl needs Ninja.
    if svt_build.is_msvc {
        let runtime_library = msvc_runtime_library();
        config
            .generator("Ninja")
            .define("CMAKE_MSVC_RUNTIME_LIBRARY", runtime_library);
    }

    // Configuring alone first lets a failed SIMD flag check stop the build before SVT compiles.
    config.build_target(CONFIGURE_TARGET);
    let install_dir = config.build();
    check_simd(&install_dir, svt_build.target_arch);

    config.build_target("install").always_configure(false);
    config.build()
}

/// Fails the build unless SVT's CMake kept the SIMD flags its kernels need.
fn check_simd(install_dir: &Path, target_arch: &str) {
    let cache_path = cache_path(install_dir);
    let cache = read_cache(&cache_path);

    let checked = simd::check_simd_cache(&cache, target_arch);
    if let Err(reason) = checked {
        let log_dir = cache_path.with_file_name("CMakeFiles");
        let shown_log_dir = log_dir.display();
        panic!("{reason}. CMake logs its flag checks under {shown_log_dir}");
    }
}

/// The cc config whose flags seed SVT's C compile.
pub fn c_config(clang: &Path) -> cc::Build {
    let mut c_config = cc::Build::new();
    c_config.compiler(clang);
    c_config
}

/// The release CRT Rust links, as CMake would otherwise pick the debug CRT for a Debug build.
fn msvc_runtime_library() -> &'static str {
    if target::is_crt_static() {
        return "MultiThreaded";
    }

    "MultiThreadedDLL"
}

/// Writes the `CMAKE_C_FLAGS` SVT was configured with to `output`.
pub fn write_c_flags(install_dir: &Path, output: &Path) {
    write_cache_entry(install_dir, "CMAKE_C_FLAGS", output);
}

/// Writes the `CMAKE_C_FLAGS_RELEASE` SVT was configured with to `output`.
pub fn write_release_flags(install_dir: &Path, output: &Path) {
    write_cache_entry(install_dir, "CMAKE_C_FLAGS_RELEASE", output);
}

fn write_cache_entry(install_dir: &Path, entry: &str, output: &Path) {
    let cache_path = cache_path(install_dir);
    let cache = read_cache(&cache_path);

    let prefix = format!("{entry}:STRING=");
    let value = cache.lines().find_map(|line| line.strip_prefix(prefix.as_str()));
    let Some(value) = value else {
        panic!("{} has no {entry} entry", cache_path.display());
    };

    write_text(output, value);
}

fn cache_path(install_dir: &Path) -> PathBuf {
    install_dir.join("build").join("CMakeCache.txt")
}

fn read_cache(cache_path: &Path) -> String {
    let cache = fs::read_to_string(cache_path);
    match cache {
        Ok(cache) => cache,
        Err(error) => panic!("could not read {}: {error}", cache_path.display()),
    }
}

/// Writes `contents` to `path`, failing with an error naming the path.
pub fn write_text(path: &Path, contents: &str) {
    let written = fs::write(path, contents);
    if let Err(error) = written {
        panic!("could not write {}: {error}", path.display());
    }
}
