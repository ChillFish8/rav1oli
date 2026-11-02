use std::fs;
use std::path::{Path, PathBuf};

use crate::target;

/// The fork's source tree, checked out as a submodule.
pub const SVT_SOURCE: &str = "vendor/svt-av1";

/// The tools and flags one SVT build uses.
pub struct SvtBuild<'a> {
    pub clang: &'a Path,
    pub nasm: Option<&'a Path>,
    pub cpu_flag: Option<&'a str>,
    pub is_x86_64: bool,
    pub is_msvc: bool,
}

/// Configures, builds and installs the static SVT library, returning the install dir.
pub fn build_svt(svt_build: &SvtBuild) -> PathBuf {
    let mut c_config = cc::Build::new();
    c_config.compiler(svt_build.clang);

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

    if svt_build.is_x86_64 {
        config.define("ENABLE_AVX512", "ON");
    }

    if let Some(cpu_flag) = svt_build.cpu_flag {
        config.cflag(cpu_flag).cxxflag(cpu_flag);
    }

    // The Visual Studio generator ignores `CMAKE_C_COMPILER`, so clang-cl needs Ninja.
    if svt_build.is_msvc {
        let runtime_library = msvc_runtime_library();
        config
            .generator("Ninja")
            .define("CMAKE_MSVC_RUNTIME_LIBRARY", runtime_library);
    }

    config.build()
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
    let cache_path = install_dir.join("build").join("CMakeCache.txt");
    let cache = fs::read_to_string(&cache_path);
    let cache = match cache {
        Ok(cache) => cache,
        Err(error) => panic!("could not read {}: {error}", cache_path.display()),
    };

    let prefix = format!("{entry}:STRING=");
    let value = cache.lines().find_map(|line| line.strip_prefix(prefix.as_str()));
    let Some(value) = value else {
        panic!("{} has no {entry} entry", cache_path.display());
    };

    let written = fs::write(output, value);
    if let Err(error) = written {
        panic!("could not write {}: {error}", output.display());
    }
}
