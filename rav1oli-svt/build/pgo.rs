use std::ffi::OsStr;
use std::fs;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};

/// The variable naming the dir an instrumented build writes raw profiles into.
pub const GENERATE_VARIABLE: &str = "RAV1OLI_SVT_PGO_GENERATE";
/// The variable naming the merged profile a build is optimised with.
pub const PROFILE_VARIABLE: &str = "RAV1OLI_SVT_PGO_PROFILE";

/// How SVT's C code is compiled for profile-guided optimisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileMode {
    Off,
    /// Instrumented, writing raw profiles into the dir.
    Generate(PathBuf),
    /// Optimised with the merged profile at `profile`, whose contents hash to `content_hash`.
    Use {
        profile: PathBuf,
        content_hash: u64,
    },
}

/// Picks the profile mode from the values of the two PGO variables.
///
/// An empty value counts as unset. Paths must be absolute, as cargo runs build scripts from the crate dir.
pub fn resolve_profile_mode(
    generate: Option<&OsStr>,
    profile: Option<&OsStr>,
) -> Result<ProfileMode, String> {
    let generate = generate.filter(|value| !value.is_empty());
    let profile = profile.filter(|value| !value.is_empty());

    match (generate, profile) {
        (Some(_), Some(_)) => {
            let reason = format!(
                "{GENERATE_VARIABLE} and {PROFILE_VARIABLE} are both set, but a build writes a profile or \
                 uses one"
            );
            Err(reason)
        },
        (Some(raw_dir), None) => {
            let raw_dir = checked_path(GENERATE_VARIABLE, raw_dir)?;
            Ok(ProfileMode::Generate(raw_dir))
        },
        (None, Some(profile)) => {
            let profile = checked_path(PROFILE_VARIABLE, profile)?;
            let contents = read_profile(&profile)?;
            let content_hash = content_hash(&contents);
            Ok(ProfileMode::Use {
                profile,
                content_hash,
            })
        },
        (None, None) => Ok(ProfileMode::Off),
    }
}

/// The C flags for `mode`. Using a profile turns a profile that no longer matches the code into an error.
///
/// clang reports a function whose IR profile no longer matches under `backend-plugin`. The profile's hash is
/// defined so a retrained profile at the same path changes the compile command, which makes CMake recompile.
pub fn profile_flags(mode: &ProfileMode, is_msvc: bool) -> Vec<String> {
    let flags = match mode {
        ProfileMode::Off => Vec::new(),
        ProfileMode::Generate(raw_dir) => {
            let shown_dir = raw_dir.display();
            vec![format!("-fprofile-generate={shown_dir}")]
        },
        ProfileMode::Use {
            profile,
            content_hash,
        } => {
            let shown_profile = profile.display();
            vec![
                format!("-fprofile-use={shown_profile}"),
                format!("-DRAV1OLI_SVT_PROFILE_HASH={content_hash:016x}"),
                "-Werror=backend-plugin".to_string(),
                // Only fires for a front-end profile from `-fprofile-instr-generate`.
                "-Werror=profile-instr-out-of-date".to_string(),
            ]
        },
    };

    if !is_msvc {
        return flags;
    }

    flags.iter().map(|flag| format!("/clang:{flag}")).collect()
}

/// The profile runtime on the link line clang prints for `-fprofile-generate` under `-###`.
pub fn profile_runtime(driver_output: &str) -> Option<PathBuf> {
    let arguments = quoted_arguments(driver_output);

    for argument in &arguments {
        let path = argument.strip_prefix("-defaultlib:").unwrap_or(argument);
        let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let is_runtime_name =
            file_name.starts_with("libclang_rt.profile") || file_name.starts_with("clang_rt.profile");
        let is_library = file_name.ends_with(".a") || file_name.ends_with(".lib");
        if is_runtime_name && is_library {
            return Some(PathBuf::from(path));
        }
    }

    None
}

/// Rejects rustc's `-C profile-generate` beside an instrumented SVT build.
///
/// Both link a profile runtime, so the binary would define the `__llvm_profile_*` symbols twice.
pub fn check_rustc_profile(codegen_options: &[&str], mode: &ProfileMode) -> Result<(), String> {
    let ProfileMode::Generate(_) = mode else {
        return Ok(());
    };

    let rustc_generate = codegen_options
        .iter()
        .find(|option| option.starts_with("profile-generate"));
    let Some(option) = rustc_generate else {
        return Ok(());
    };

    let reason = format!(
        "`-C {option}` in RUSTFLAGS links rustc's profile runtime and {GENERATE_VARIABLE} links clang's, so \
         the binary would define the `__llvm_profile_*` symbols twice. Set only one of them"
    );
    Err(reason)
}

/// A hash of a profile's contents, which only needs to change when the contents do.
pub fn content_hash(contents: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(contents);
    hasher.finish()
}

/// Checks a path from `variable` is absolute and holds no spaces, which CMake would split its C flags on.
fn checked_path(variable: &str, value: &OsStr) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    let shown_path = path.display();

    if !path.is_absolute() {
        let reason = format!(
            "{variable} is {shown_path}, which must be absolute as cargo runs build scripts in the crate dir"
        );
        return Err(reason);
    }

    let text = path.to_string_lossy();
    if text.contains(char::is_whitespace) {
        let reason =
            format!("{variable} is {shown_path}, which must not contain spaces as CMake splits on them");
        return Err(reason);
    }

    Ok(path)
}

fn read_profile(profile: &Path) -> Result<Vec<u8>, String> {
    let shown_profile = profile.display();

    let metadata = fs::metadata(profile);
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(error) => {
            let reason = format!("{PROFILE_VARIABLE} names {shown_profile}, which cannot be read: {error}");
            return Err(reason);
        },
    };

    if !metadata.is_file() {
        let reason = format!("{PROFILE_VARIABLE} names {shown_profile}, which is not a file");
        return Err(reason);
    }

    let contents = fs::read(profile);
    match contents {
        Ok(contents) => Ok(contents),
        Err(error) => {
            let reason = format!("{PROFILE_VARIABLE} names {shown_profile}, which cannot be read: {error}");
            Err(reason)
        },
    }
}

/// The double-quoted arguments clang's `-###` prints, with its backslash escapes undone.
fn quoted_arguments(driver_output: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = None;
    let mut is_escaped = false;

    for character in driver_output.chars() {
        let Some(argument) = current.as_mut() else {
            if character == '"' {
                current = Some(String::new());
            }

            continue;
        };

        if is_escaped {
            argument.push(character);
            is_escaped = false;
        } else if character == '\\' {
            is_escaped = true;
        } else if character == '"' {
            let finished = current.take();
            arguments.extend(finished);
        } else {
            argument.push(character);
        }
    }

    arguments
}
