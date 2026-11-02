use crate::cpu_flag::{
    SVT_CODEGEN_FLAGS,
    codegen_options,
    cpu_flags_for,
    svt_c_flags,
    takes_elf_codegen_flags,
};

#[test]
fn native_on_x86_64_gives_only_march() {
    let flags = cpu_flags_for("-Ctarget-cpu=native", "x86_64", false);
    assert_eq!(flags, ["-march=native"]);
}

#[test]
fn x86_64_v3_gives_only_march() {
    let flags = cpu_flags_for("-Ctarget-cpu=x86-64-v3", "x86_64", false);
    assert_eq!(flags, ["-march=x86-64-v3"]);
}

#[test]
fn apple_m1_on_aarch64_gives_only_mcpu() {
    let flags = cpu_flags_for("-C\x1ftarget-cpu=apple-m1", "aarch64", false);
    assert_eq!(flags, ["-mcpu=apple-m1"]);
}

#[test]
fn clang_cl_passes_the_flags_through_clang() {
    let flags = cpu_flags_for("--codegen=target-cpu=x86-64-v3", "x86_64", true);
    assert_eq!(flags, ["/clang:-march=x86-64-v3"]);
}

#[test]
fn no_target_cpu_gives_no_flags() {
    let flags = cpu_flags_for("-Copt-level=3\x1f-C\x1fdebuginfo=0", "x86_64", false);
    assert!(flags.is_empty(), "{flags:?}");
}

#[test]
fn empty_rustflags_give_no_flags() {
    let flags = cpu_flags_for("", "x86_64", false);
    assert!(flags.is_empty(), "{flags:?}");
}

#[test]
fn the_last_target_cpu_wins() {
    let rustflags = "-Ctarget-cpu=znver4\x1f--codegen\x1ftarget-cpu=x86-64-v2";
    let flags = cpu_flags_for(rustflags, "x86_64", false);
    assert_eq!(flags, ["-march=x86-64-v2"]);
}

#[test]
#[should_panic(expected = "no CPU flag")]
fn target_cpu_on_an_unsupported_arch_is_an_error() {
    cpu_flags_for("-Ctarget-cpu=generic-rv64", "riscv64", false);
}

#[test]
fn codegen_options_cover_every_spelling() {
    let rustflags = concat!(
        "-Copt-level=3\x1f-C\x1fdebuginfo=0\x1f",
        "--codegen=lto\x1f--codegen\x1fpanic=abort\x1f-Dwarnings",
    );
    let options = codegen_options(rustflags);
    assert_eq!(options, ["opt-level=3", "debuginfo=0", "lto", "panic=abort"]);
}

#[test]
fn svt_c_flags_put_the_codegen_flags_before_the_cpu_flags() {
    let cpu_flags = ["-march=znver4".to_string()];
    let flags = svt_c_flags(&cpu_flags, false, "linux");

    let expected = [
        "-fno-math-errno",
        "-fomit-frame-pointer",
        "-fno-stack-protector",
        "-U_FORTIFY_SOURCE",
        "-fno-semantic-interposition",
        "-fno-stack-clash-protection",
        "-march=znver4",
    ];
    assert_eq!(flags, expected);
}

#[test]
fn svt_c_flags_on_macos_leave_out_the_elf_flags() {
    let flags = svt_c_flags(&[], false, "macos");
    assert_eq!(flags, SVT_CODEGEN_FLAGS);
}

#[test]
fn svt_c_flags_on_windows_gnu_leave_out_the_elf_flags() {
    let flags = svt_c_flags(&[], false, "windows");
    assert_eq!(flags, SVT_CODEGEN_FLAGS);
}

#[test]
fn only_linux_android_and_freebsd_take_the_elf_flags() {
    let taking: Vec<&str> = [
        "linux", "android", "freebsd", "macos", "ios", "windows", "netbsd", "openbsd",
    ]
    .into_iter()
    .filter(|target_os| takes_elf_codegen_flags(target_os))
    .collect();

    assert_eq!(taking, ["linux", "android", "freebsd"]);
}

#[test]
fn clang_cl_takes_only_the_cpu_flags() {
    let cpu_flags = ["/clang:-march=x86-64-v3".to_string()];
    let flags = svt_c_flags(&cpu_flags, true, "windows");
    assert_eq!(flags, cpu_flags);
}
