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
#[path = "build/pgo.rs"]
mod pgo;
#[path = "build/simd.rs"]
mod simd;
#[path = "build/target.rs"]
mod target;
#[path = "build/toolchain.rs"]
mod toolchain;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use self::library::SvtBuild;
use self::lto::{LtoBuild, LtoTools};
use self::pgo::ProfileMode;

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
    println!("cargo::rerun-if-env-changed={}", pgo::GENERATE_VARIABLE);
    println!("cargo::rerun-if-env-changed={}", pgo::PROFILE_VARIABLE);
    println!("cargo::rustc-check-cfg=cfg(svt_default_cpu)");
    println!("cargo::rustc-check-cfg=cfg(svt_pgo_generate)");
    println!("cargo::rustc-check-cfg=cfg(svt_pgo_use)");

    let source = Path::new(library::SVT_SOURCE).join("CMakeLists.txt");
    if !source.exists() {
        panic!("the SVT-AV1 submodule is not checked out. Run `just submodules`");
    }

    let cpu_flags = target::target_cpu_flags();
    if cpu_flags.is_empty() {
        println!("cargo::rustc-cfg=svt_default_cpu");
    }

    let is_msvc = target::is_msvc_target();
    let clang = toolchain::find_clang(is_msvc);
    toolchain::require_tool("cmake");

    if is_msvc {
        toolchain::require_tool("ninja");
    }

    let target_arch = target::target_arch();
    let target_os = target::target_os();
    let is_x86_64 = target_arch == "x86_64";
    let c_flags = cpu::svt_c_flags(&cpu_flags, is_msvc, &target_os);
    let nasm = is_x86_64.then(|| toolchain::require_tool("nasm"));

    let is_lto = env::var_os("CARGO_FEATURE_LTO").is_some();
    let lto_tools = is_lto.then(|| LtoTools::find(&clang));

    let profile_mode = profile_mode();
    if let ProfileMode::Use { profile, .. } = &profile_mode {
        check_profile(&clang, profile);
        println!("cargo::rustc-cfg=svt_pgo_use");
    }

    let profile_flags = pgo::profile_flags(&profile_mode, is_msvc);
    let keeps_warnings = matches!(profile_mode, ProfileMode::Use { .. });
    let svt_build = SvtBuild {
        clang: &clang,
        nasm: nasm.as_deref(),
        c_flags: &c_flags,
        target_arch: &target_arch,
        is_msvc,
        is_lto,
        profile_flags: &profile_flags,
        keeps_warnings,
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
                c_flags: &c_flags,
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

    // The runtime serves SVT's instrumented code, so it must come after SVT on the link line.
    if let ProfileMode::Generate(_) = profile_mode {
        link_profile_runtime(&clang, is_msvc);
        println!("cargo::rustc-cfg=svt_pgo_generate");
    }

    let bindings_path = out_dir.join("bindings.rs");
    bindings::generate_bindings(&bindings_path);
}

/// The profile mode from the PGO variables, failing the build on a bad combination or path.
fn profile_mode() -> ProfileMode {
    let generate = env::var_os(pgo::GENERATE_VARIABLE);
    let profile = env::var_os(pgo::PROFILE_VARIABLE);
    let resolved = pgo::resolve_profile_mode(generate.as_deref(), profile.as_deref());
    let profile_mode = match resolved {
        Ok(profile_mode) => profile_mode,
        Err(reason) => panic!("{reason}"),
    };

    let rustflags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let codegen_options = cpu::codegen_options(&rustflags);
    let checked = pgo::check_rustc_profile(&codegen_options, &profile_mode);
    if let Err(reason) = checked {
        panic!("{reason}");
    }

    if let ProfileMode::Use { profile, .. } = &profile_mode {
        let shown_profile = profile.display();
        println!("cargo::rerun-if-changed={shown_profile}");
    }

    profile_mode
}

/// Checks `llvm-profdata` from clang's LLVM version can read `profile`.
fn check_profile(clang: &Path, profile: &Path) {
    let llvm_profdata = toolchain::require_llvm_tool("llvm-profdata", clang);

    let mut show_command = Command::new(llvm_profdata);
    show_command.arg("show").arg(profile);
    toolchain::tool_output(&mut show_command);
}

/// Links clang's profile runtime, which writes the raw profiles of an instrumented build at exit.
///
/// rustc links the program, so the runtime clang would link is found from its `-###` link line.
fn link_profile_runtime(clang: &Path, is_msvc: bool) {
    let out_dir = target::out_dir();
    let probe_source = out_dir.join("pgo-probe.c");
    library::write_text(&probe_source, "int main(void) { return 0; }\n");

    let c_config = library::c_config(clang);
    let compiler = c_config.get_compiler();
    let generate_flag = if is_msvc {
        "/clang:-fprofile-generate"
    } else {
        "-fprofile-generate"
    };

    let mut probe_command = Command::new(clang);
    probe_command
        .current_dir(&out_dir)
        .args(compiler.args())
        .arg(generate_flag)
        .arg("-###")
        .arg(&probe_source);
    let driver_output = driver_output(&mut probe_command);

    let runtime = pgo::profile_runtime(&driver_output);
    let shown_clang = clang.display();
    let Some(runtime) = runtime else {
        panic!("{shown_clang} links no profile runtime for -fprofile-generate:\n{driver_output}");
    };

    let shown_runtime = runtime.display();
    if !runtime.is_file() {
        panic!(
            "{shown_clang} links its profile runtime from {shown_runtime}, which does not exist. Install \
             compiler-rt for this clang"
        );
    }

    let runtime_dir = runtime.parent().unwrap_or(Path::new(""));
    let runtime_name = runtime.file_name().unwrap_or_default();
    let shown_dir = runtime_dir.display();
    let shown_name = runtime_name.to_string_lossy();
    println!("cargo::rustc-link-search=native={shown_dir}");
    println!("cargo::rustc-link-lib=static:+verbatim={shown_name}");
}

/// Runs the clang driver under `-###`, which prints the commands it would run to stderr.
fn driver_output(command: &mut Command) -> String {
    let shown_command = format!("{command:?}");
    let output = command.output();
    let output = match output {
        Ok(output) => output,
        Err(error) => panic!("could not run {shown_command}: {error}"),
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        panic!("{shown_command} failed with {}:\n{stderr}", output.status);
    }

    stderr.into_owned()
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
