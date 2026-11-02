use crate::llvm::{is_bitcode, merged_compile_flags, parse_major_version};

#[test]
fn a_clang_version_gives_its_major() {
    let text = "Homebrew clang version 22.1.6\nTarget: x86_64-unknown-linux-gnu\n";
    assert_eq!(parse_major_version(text), Some(22));
}

#[test]
fn an_llvm_tool_version_gives_its_major() {
    let text = "LLVM (http://llvm.org/):\n  LLVM version 19.1.7\n  Optimized build.\n";
    assert_eq!(parse_major_version(text), Some(19));
}

#[test]
fn a_development_version_gives_its_major() {
    let text = "clang version 23.0.0git (https://github.com/llvm/llvm-project abc)";
    assert_eq!(parse_major_version(text), Some(23));
}

#[test]
fn text_without_a_version_gives_none() {
    assert_eq!(parse_major_version("command not found"), None);
}

#[test]
fn raw_bitcode_is_bitcode() {
    assert!(is_bitcode(&[b'B', b'C', 0xC0, 0xDE, 0x35, 0x14]));
}

#[test]
fn wrapped_bitcode_is_bitcode() {
    assert!(is_bitcode(&[0xDE, 0xC0, 0x17, 0x0B, 0x00, 0x00]));
}

#[test]
fn an_elf_object_is_native() {
    assert!(!is_bitcode(&[0x7F, b'E', b'L', b'F', 0x02, 0x01]));
}

#[test]
fn a_short_file_is_native() {
    assert!(!is_bitcode(b"BC"));
}

fn gnu_base_flags() -> Vec<String> {
    let flags = [
        "-O0",
        "-ffunction-sections",
        "-fdata-sections",
        "-fPIC",
        "-gdwarf-4",
        "--target=aarch64-unknown-linux-gnu",
        "-flto=thin",
        "-fprofile-generate",
    ];

    flags.iter().map(|flag| flag.to_string()).collect()
}

#[test]
fn merged_flags_keep_the_target_and_sections() {
    let base_flags = gnu_base_flags();
    let flags = merged_compile_flags(&base_flags, None, false);

    let expected = [
        "-ffunction-sections",
        "-fdata-sections",
        "-fPIC",
        "-gdwarf-4",
        "--target=aarch64-unknown-linux-gnu",
        "-O3",
    ];
    assert_eq!(flags, expected);
}

#[test]
fn merged_flags_end_with_the_cpu_flag() {
    let base_flags = gnu_base_flags();
    let flags = merged_compile_flags(&base_flags, Some("-mcpu=neoverse-v2"), false);

    let last_flags = &flags[flags.len() - 2..];
    assert_eq!(last_flags, ["-O3", "-mcpu=neoverse-v2"]);
}

#[test]
fn merged_flags_for_clang_cl_drop_its_optimisation_and_profile_flags() {
    let base_flags: Vec<String> = [
        "-nologo",
        "-MD",
        "-O2",
        "/clang:-fprofile-use=a.profdata",
        "-Brepro",
    ]
    .iter()
    .map(|flag| flag.to_string())
    .collect();
    let flags = merged_compile_flags(&base_flags, None, true);

    assert_eq!(flags, ["-nologo", "-MD", "-Brepro", "/clang:-O3"]);
}
