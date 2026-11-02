#[path = "build/bindings.rs"]
mod bindings;
#[path = "build/cpu.rs"]
mod cpu;
#[path = "build/library.rs"]
mod library;
#[path = "build/llvm.rs"]
mod llvm;
#[path = "build/lto.rs"]
mod lto;
#[path = "build/target.rs"]
mod target;
#[path = "build/toolchain.rs"]
mod toolchain;

use std::env;
use std::path::{Path, PathBuf};

use self::library::SvtBuild;
use self::lto::{LtoBuild, LtoTools};

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=build");
    println!("cargo::rerun-if-changed=include");
    println!("cargo::rerun-if-changed=shim");
    println!("cargo::rerun-if-changed=vendor/svt-av1/Source");
    println!("cargo::rerun-if-changed=vendor/svt-av1/CMakeLists.txt");
    println!("cargo::rerun-if-changed=vendor/svt-av1/cmake");
    println!("cargo::rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!("cargo::rerun-if-env-changed=CC");
    println!("cargo::rerun-if-env-changed=PATH");
    println!("cargo::rustc-check-cfg=cfg(svt_default_cpu)");

    let source = Path::new(library::SVT_SOURCE).join("CMakeLists.txt");
    if !source.exists() {
        panic!("the SVT-AV1 submodule is not checked out. Run `just submodules`");
    }

    let cpu_flag = target::target_cpu_flag();
    if cpu_flag.is_none() {
        println!("cargo::rustc-cfg=svt_default_cpu");
    }

    let is_msvc = target::is_msvc_target();
    let clang = toolchain::find_clang(is_msvc);
    toolchain::require_tool("cmake");

    if is_msvc {
        toolchain::require_tool("ninja");
    }

    let target_arch = target::target_arch();
    let is_x86_64 = target_arch == "x86_64";
    let nasm = is_x86_64.then(|| toolchain::require_tool("nasm"));

    let is_lto = env::var_os("CARGO_FEATURE_LTO").is_some();
    let lto_tools = is_lto.then(|| LtoTools::find(&clang));

    let svt_build = SvtBuild {
        clang: &clang,
        nasm: nasm.as_deref(),
        cpu_flag: cpu_flag.as_deref(),
        is_x86_64,
        is_msvc,
        is_lto,
    };
    let install_dir = library::build_svt(&svt_build);

    let out_dir = target::out_dir();
    let c_flags_path = out_dir.join("svt-c-flags.txt");
    library::write_c_flags(&install_dir, &c_flags_path);

    let release_flags_path = out_dir.join("svt-release-flags.txt");
    library::write_release_flags(&install_dir, &release_flags_path);

    let build_mode_path = out_dir.join("svt-build-mode.txt");
    let build_mode = if is_lto { "lto" } else { "plain" };
    library::write_text(&build_mode_path, build_mode);

    let lib_dir = install_dir.join("lib");
    let (link_dir, library_name) = match &lto_tools {
        Some(tools) => {
            let lto_build = LtoBuild {
                tools,
                clang: &clang,
                cpu_flag: cpu_flag.as_deref(),
                is_msvc,
            };
            let lto_dir = merge_svt(&lto_build, &lib_dir);
            (lto_dir, lto::LTO_LIBRARY)
        },
        None => (lib_dir, "SvtAv1Enc"),
    };

    // The shim calls into SVT, so it must come before SVT on the link line.
    compile_log_shim(&clang);
    link_svt(&link_dir, library_name);

    let bindings_path = out_dir.join("bindings.rs");
    bindings::generate_bindings(&bindings_path);
}

/// Builds the LTO archive from the static SVT library in `lib_dir`, returning the dir that holds it.
fn merge_svt(lto_build: &LtoBuild, lib_dir: &Path) -> PathBuf {
    let archive_name = target::static_library_name("SvtAv1Enc");
    let archive = lib_dir.join(archive_name);

    lto::build_lto_archive(lto_build, &archive)
}

/// Compiles the C helper that formats SVT's `va_list` log messages, which stable Rust cannot read.
fn compile_log_shim(clang: &Path) {
    let api_dir = Path::new(library::SVT_SOURCE).join("Source").join("API");

    cc::Build::new()
        .compiler(clang)
        .include("include")
        .include(api_dir)
        .file("shim/log_format.c")
        .compile("rav1oli_svt_shim");
}

/// Links the static SVT library and the system libraries its CMake links.
fn link_svt(link_dir: &Path, library_name: &str) {
    println!("cargo::rustc-link-search=native={}", link_dir.display());
    println!("cargo::rustc-link-lib=static={library_name}");

    if target::is_unix_target() {
        println!("cargo::rustc-link-lib=dylib=pthread");
        println!("cargo::rustc-link-lib=dylib=m");
    }
}
