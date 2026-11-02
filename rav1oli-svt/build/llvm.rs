/// The magic at the start of a raw LLVM bitcode file.
const BITCODE_MAGIC: [u8; 4] = [b'B', b'C', 0xC0, 0xDE];
/// The magic at the start of a bitcode wrapper, `0x0B17C0DE` stored little-endian.
const WRAPPER_MAGIC: [u8; 4] = [0xDE, 0xC0, 0x17, 0x0B];

/// The major version after the first `version` word, as in `clang version 22.1.6`.
pub fn parse_major_version(text: &str) -> Option<u32> {
    let mut words = text.split_whitespace();
    words.find(|word| *word == "version")?;

    let version = words.next()?;
    let major = version.split('.').next()?;
    major.parse().ok()
}

/// Whether an object file's leading bytes mark it as LLVM bitcode.
pub fn is_bitcode(header: &[u8]) -> bool {
    header.starts_with(&BITCODE_MAGIC) || header.starts_with(&WRAPPER_MAGIC)
}

/// The flags that compile merged bitcode, built from the C compile's base flags.
///
/// Optimisation, LTO and profile flags are dropped, then `-O3` and the CPU flag are added.
pub fn merged_compile_flags(base_flags: &[String], cpu_flag: Option<&str>, is_msvc: bool) -> Vec<String> {
    let mut flags: Vec<String> = base_flags
        .iter()
        .filter(|flag| is_kept_flag(flag))
        .cloned()
        .collect();

    let optimisation = if is_msvc { "/clang:-O3" } else { "-O3" };
    flags.push(optimisation.to_string());

    if let Some(cpu_flag) = cpu_flag {
        flags.push(cpu_flag.to_string());
    }

    flags
}

fn is_kept_flag(flag: &str) -> bool {
    let flag = flag.strip_prefix("/clang:").unwrap_or(flag);
    let is_optimisation = flag.starts_with("-O") || flag.starts_with("/O");
    let is_lto = flag.starts_with("-flto");
    let is_profile = flag.starts_with("-fprofile-") || flag.starts_with("-fcs-profile-");

    !is_optimisation && !is_lto && !is_profile
}
