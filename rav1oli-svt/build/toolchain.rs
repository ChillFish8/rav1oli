use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

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

fn names_clang(compiler: &Path, is_msvc: bool) -> bool {
    let file_stem = compiler.file_stem().unwrap_or_default();
    let file_stem = file_stem.to_string_lossy();
    let is_clang_cl = file_stem.starts_with("clang-cl");

    if is_msvc {
        return is_clang_cl;
    }

    file_stem.contains("clang") && !is_clang_cl
}

fn find_on_path(name: impl AsRef<Path>) -> Option<PathBuf> {
    let mut file_name = name.as_ref().as_os_str().to_owned();
    let has_extension = name.as_ref().extension().is_some();
    if !has_extension {
        file_name.push(env::consts::EXE_SUFFIX);
    }

    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(&file_name))
        .find(|candidate| candidate.is_file())
}
