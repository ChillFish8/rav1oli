/// The C flag for the last `-C target-cpu` in cargo's encoded rustflags.
///
/// x86_64 takes `-march` and aarch64 takes `-mcpu`. clang-cl gets the flag through `/clang:`.
pub fn cpu_flag_for(rustflags: &str, target_arch: &str, is_clang_cl: bool) -> Option<String> {
    let cpu = last_target_cpu(rustflags)?;

    let flag_name = match target_arch {
        "x86_64" => "-march",
        "aarch64" => "-mcpu",
        other => panic!("SVT-AV1 has no CPU flag for `-C target-cpu={cpu}` on {other}"),
    };

    let flag = format!("{flag_name}={cpu}");
    if is_clang_cl {
        return Some(format!("/clang:{flag}"));
    }

    Some(flag)
}

/// The value of the last `-C target-cpu`, in any of the spellings rustc accepts.
fn last_target_cpu(rustflags: &str) -> Option<&str> {
    let mut target_cpu = None;
    let mut is_codegen_value = false;

    for flag in rustflags.split('\x1f') {
        let codegen_option = if is_codegen_value {
            is_codegen_value = false;
            Some(flag)
        } else if flag == "-C" || flag == "--codegen" {
            is_codegen_value = true;
            None
        } else if let Some(option) = flag.strip_prefix("--codegen=") {
            Some(option)
        } else {
            flag.strip_prefix("-C")
        };

        let cpu = codegen_option.and_then(|option| option.strip_prefix("target-cpu="));
        if cpu.is_some() {
            target_cpu = cpu;
        }
    }

    target_cpu
}
