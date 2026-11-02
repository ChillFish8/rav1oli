#[path = "build/bindings.rs"]
mod bindings;
#[path = "build/cpu.rs"]
mod cpu;
#[path = "build/library.rs"]
mod library;
#[path = "build/target.rs"]
mod target;
#[path = "build/toolchain.rs"]
mod toolchain;

use std::path::Path;

use self::library::SvtBuild;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=build");
    println!("cargo::rerun-if-changed=include");
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

    let svt_build = SvtBuild {
        clang: &clang,
        nasm: nasm.as_deref(),
        cpu_flag: cpu_flag.as_deref(),
        is_x86_64,
        is_msvc,
    };
    let install_dir = library::build_svt(&svt_build);

    let out_dir = target::out_dir();
    let c_flags_path = out_dir.join("svt-c-flags.txt");
    library::write_c_flags(&install_dir, &c_flags_path);

    let release_flags_path = out_dir.join("svt-release-flags.txt");
    library::write_release_flags(&install_dir, &release_flags_path);

    link_svt(&install_dir);

    let bindings_path = out_dir.join("bindings.rs");
    bindings::generate_bindings(&bindings_path);
}

/// Links the static SVT library and the system libraries its CMake links.
fn link_svt(install_dir: &Path) {
    let lib_dir = install_dir.join("lib");
    println!("cargo::rustc-link-search=native={}", lib_dir.display());
    println!("cargo::rustc-link-lib=static=SvtAv1Enc");

    if target::is_unix_target() {
        println!("cargo::rustc-link-lib=dylib=pthread");
        println!("cargo::rustc-link-lib=dylib=m");
    }
}
