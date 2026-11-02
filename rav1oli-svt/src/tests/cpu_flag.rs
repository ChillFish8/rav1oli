use crate::cpu_flag::cpu_flag_for;

#[test]
fn native_on_x86_64_gives_march() {
    let flag = cpu_flag_for("-Ctarget-cpu=native", "x86_64", false);
    assert_eq!(flag.as_deref(), Some("-march=native"));
}

#[test]
fn apple_m1_on_aarch64_gives_mcpu() {
    let flag = cpu_flag_for("-C\x1ftarget-cpu=apple-m1", "aarch64", false);
    assert_eq!(flag.as_deref(), Some("-mcpu=apple-m1"));
}

#[test]
fn clang_cl_passes_the_flag_through_clang() {
    let flag = cpu_flag_for("--codegen=target-cpu=x86-64-v3", "x86_64", true);
    assert_eq!(flag.as_deref(), Some("/clang:-march=x86-64-v3"));
}

#[test]
fn no_target_cpu_gives_none() {
    let flag = cpu_flag_for("-Copt-level=3\x1f-C\x1fdebuginfo=0", "x86_64", false);
    assert_eq!(flag, None);
}

#[test]
fn empty_rustflags_give_none() {
    let flag = cpu_flag_for("", "x86_64", false);
    assert_eq!(flag, None);
}

#[test]
fn the_last_target_cpu_wins() {
    let rustflags = "-Ctarget-cpu=znver4\x1f--codegen\x1ftarget-cpu=x86-64-v2";
    let flag = cpu_flag_for(rustflags, "x86_64", false);
    assert_eq!(flag.as_deref(), Some("-march=x86-64-v2"));
}

#[test]
#[should_panic(expected = "no CPU flag")]
fn target_cpu_on_an_unsupported_arch_is_an_error() {
    cpu_flag_for("-Ctarget-cpu=generic-rv64", "riscv64", false);
}
