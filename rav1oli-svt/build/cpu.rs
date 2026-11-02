/// The codegen flags every clang compile of SVT takes.
pub const SVT_CODEGEN_FLAGS: [&str; 6] = [
    "-fno-math-errno",
    "-fomit-frame-pointer",
    "-fno-semantic-interposition",
    "-fno-stack-protector",
    "-fno-stack-clash-protection",
    "-U_FORTIFY_SOURCE",
];

/// The C flags for the last `-C target-cpu` in cargo's encoded rustflags.
///
/// x86_64 takes `-march` and `-mtune`, while aarch64 takes `-mcpu`, which also tunes. clang-cl gets each
/// flag through `/clang:`.
pub fn cpu_flags_for(rustflags: &str, target_arch: &str, is_clang_cl: bool) -> Vec<String> {
    let Some(cpu) = last_target_cpu(rustflags) else {
        return Vec::new();
    };

    let flags = match target_arch {
        "x86_64" => vec![format!("-march={cpu}"), format!("-mtune={cpu}")],
        "aarch64" => vec![format!("-mcpu={cpu}")],
        other => panic!("SVT-AV1 has no CPU flag for `-C target-cpu={cpu}` on {other}"),
    };

    if !is_clang_cl {
        return flags;
    }

    flags.iter().map(|flag| format!("/clang:{flag}")).collect()
}

/// The flags SVT's C compiles add to their base flags, the codegen flags then `cpu_flags`.
///
/// clang-cl takes only the CPU flags.
pub fn svt_c_flags(cpu_flags: &[String], is_clang_cl: bool) -> Vec<String> {
    let mut flags = Vec::new();

    if !is_clang_cl {
        let codegen_flags = SVT_CODEGEN_FLAGS.iter().map(|flag| flag.to_string());
        flags.extend(codegen_flags);
    }

    flags.extend_from_slice(cpu_flags);
    flags
}

/// The value of the last `-C target-cpu`, in any of the spellings rustc accepts.
fn last_target_cpu(rustflags: &str) -> Option<&str> {
    let options = codegen_options(rustflags);

    options
        .iter()
        .rev()
        .find_map(|option| option.strip_prefix("target-cpu="))
}

/// The value of every `-C` option in cargo's encoded rustflags, in any of the spellings rustc accepts.
pub fn codegen_options(rustflags: &str) -> Vec<&str> {
    let mut options = Vec::new();
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

        if let Some(option) = codegen_option {
            options.push(option);
        }
    }

    options
}
