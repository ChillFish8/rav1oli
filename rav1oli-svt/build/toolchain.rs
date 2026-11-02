use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::llvm;

/// The clang that builds SVT, from the user's C compiler variable or else `PATH`.
///
/// MSVC targets need clang-cl, every other target needs a GNU-style clang.
pub fn find_clang(is_msvc: bool) -> PathBuf {
    let clang_name = if is_msvc { "clang-cl" } else { "clang" };

    let user_cc = user_cc();
    let Some((variable, user_cc)) = user_cc else {
        let clang = find_on_path(clang_name);
        let Some(clang) = clang else {
            panic!("SVT-AV1 must be built with clang, but no {clang_name} was found on PATH");
        };

        return clang;
    };

    let user_cc = PathBuf::from(user_cc);
    let shown_cc = user_cc.display();
    let is_clang = names_clang(&user_cc, is_msvc);
    if !is_clang {
        panic!("SVT-AV1 must be built with {clang_name}, found {shown_cc} in {variable}");
    }

    if user_cc.is_file() {
        return user_cc;
    }

    let clang = find_on_path(&user_cc);
    let Some(clang) = clang else {
        panic!("{variable} names {shown_cc}, which was not found on PATH");
    };

    clang
}

/// The first C compiler variable set, in the order the cc crate reads them.
fn user_cc() -> Option<(String, OsString)> {
    let target = env::var("TARGET").unwrap_or_default();
    let target_underscored = target.replace('-', "_");
    let variables = [
        format!("CC_{target}"),
        format!("CC_{target_underscored}"),
        "TARGET_CC".to_string(),
        "CC".to_string(),
    ];

    for variable in &variables {
        println!("cargo::rerun-if-env-changed={variable}");
    }

    variables.into_iter().find_map(|variable| {
        let value = env::var_os(&variable)?;
        Some((variable, value))
    })
}

/// A tool SVT's build needs, failing with an error naming it when it is not on `PATH`.
pub fn require_tool(name: &str) -> PathBuf {
    let tool = find_on_path(name);
    let Some(tool) = tool else {
        panic!("building SVT-AV1 needs `{name}`, which was not found on PATH");
    };

    tool
}

/// An LLVM tool matching clang's LLVM major version.
///
/// It looks for `name` beside clang and then on `PATH`, and for Debian's `name-<major>` the same way.
pub fn require_llvm_tool(name: &str, clang: &Path) -> PathBuf {
    let clang_text = version_text(clang);
    let clang_version = parse_version(clang, &clang_text);
    let suffixed_name = format!("{name}-{clang_version}");

    let plain_tool = find_beside_or_on_path(name, clang);
    let suffixed_tool = find_beside_or_on_path(&suffixed_name, clang);
    if plain_tool.is_none() && suffixed_tool.is_none() {
        let clang_directory = clang.parent().unwrap_or(Path::new(""));
        let shown_directory = clang_directory.display();
        panic!(
            "building SVT-AV1 with LTO needs `{name}` or `{suffixed_name}`, \
             which is not in {shown_directory} or on PATH"
        );
    }

    let mut rejections = Vec::new();

    if let Some(plain_tool) = plain_tool {
        let rejection = reject_tool(&plain_tool, clang_version);
        let Some(rejection) = rejection else {
            return plain_tool;
        };

        rejections.push(rejection);
    }

    // A plain name can find another LLVM install, while the suffixed name names clang's major version.
    if let Some(suffixed_tool) = suffixed_tool {
        let rejection = reject_tool(&suffixed_tool, clang_version);
        let Some(rejection) = rejection else {
            if let Some(plain_rejection) = rejections.first() {
                let shown_tool = suffixed_tool.display();
                println!("cargo::warning=using {shown_tool} because {plain_rejection}");
            }

            return suffixed_tool;
        };

        rejections.push(rejection);
    }

    let shown_clang = clang.display();
    let reasons = rejections.join(". ");
    let mismatch = format!(
        "no `{name}` shares the LLVM major version of {shown_clang}, which is {clang_version}. {reasons}"
    );

    if clang_text.contains("Apple clang") {
        panic!(
            "{mismatch}. Apple clang numbers its versions apart from LLVM, so set `CC` to an LLVM clang, \
             such as Homebrew's `llvm`, that matches the LLVM tools"
        );
    }

    panic!("{mismatch}");
}

/// Why `tool` can't be used beside a clang of `clang_version`, or `None` when it can.
fn reject_tool(tool: &Path, clang_version: u32) -> Option<String> {
    let shown_tool = tool.display();
    let version = try_llvm_major_version(tool);

    match version {
        Ok(version) if version == clang_version => None,
        Ok(version) => Some(format!("{shown_tool} is LLVM {version}, not {clang_version}")),
        Err(reason) => Some(reason),
    }
}

/// The LLVM major version a tool reports through `--version`, or why it could not be read.
fn try_llvm_major_version(tool: &Path) -> Result<u32, String> {
    let shown_tool = tool.display();
    let output = Command::new(tool).arg("--version").output();
    let output = match output {
        Ok(output) => output,
        Err(error) => return Err(format!("{shown_tool} --version could not run ({error})")),
    };

    if !output.status.success() {
        let status = output.status;
        return Err(format!("{shown_tool} --version failed with {status}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = llvm::parse_major_version(&stdout);
    let Some(version) = version else {
        return Err(format!("{shown_tool} --version reported no version"));
    };

    Ok(version)
}

fn version_text(tool: &Path) -> String {
    let mut version_command = Command::new(tool);
    version_command.arg("--version");
    tool_output(&mut version_command)
}

fn parse_version(tool: &Path, text: &str) -> u32 {
    let version = llvm::parse_major_version(text);
    let Some(version) = version else {
        let shown_tool = tool.display();
        panic!("could not find a version in the output of {shown_tool} --version:\n{text}");
    };

    version
}

fn find_beside_or_on_path(name: &str, clang: &Path) -> Option<PathBuf> {
    let clang_directory = clang.parent().unwrap_or(Path::new(""));
    let file_name = executable_name(name);
    let beside_clang = clang_directory.join(file_name);
    if beside_clang.is_file() {
        return Some(beside_clang);
    }

    find_on_path(name)
}

/// Runs a build tool, failing with its command line when it cannot start or exits unsuccessfully.
pub fn run_tool(command: &mut Command) {
    let shown_command = format!("{command:?}");
    let status = command.status();
    let status = match status {
        Ok(status) => status,
        Err(error) => panic!("could not run {shown_command}: {error}"),
    };

    if !status.success() {
        panic!("{shown_command} failed with {status}");
    }
}

/// Runs a build tool and returns its standard output.
pub fn tool_output(command: &mut Command) -> String {
    let shown_command = format!("{command:?}");
    let output = command.output();
    let output = match output {
        Ok(output) => output,
        Err(error) => panic!("could not run {shown_command}: {error}"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("{shown_command} failed with {}:\n{stderr}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.into_owned()
}

fn names_clang(compiler: &Path, is_msvc: bool) -> bool {
    let file_stem = compiler.file_stem().unwrap_or_default();
    let file_stem = file_stem.to_string_lossy();
    let is_clang_cl = file_stem.starts_with("clang-cl");

    if is_msvc {
        return is_clang_cl;
    }

    file_stem.contains("clang") && !is_clang_cl
}

fn executable_name(name: impl AsRef<Path>) -> OsString {
    let mut file_name = name.as_ref().as_os_str().to_owned();
    let has_extension = name.as_ref().extension().is_some();
    if !has_extension {
        file_name.push(env::consts::EXE_SUFFIX);
    }

    file_name
}

fn find_on_path(name: impl AsRef<Path>) -> Option<PathBuf> {
    let file_name = executable_name(name);

    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(&file_name))
        .find(|candidate| candidate.is_file())
}
