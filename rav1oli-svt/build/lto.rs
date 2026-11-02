use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{library, llvm, target, toolchain};

/// The library name the merged archive is linked under.
pub const LTO_LIBRARY: &str = "SvtAv1EncLto";

/// The LLVM tools that merge SVT's bitcode, each sharing clang's LLVM major version.
pub struct LtoTools {
    pub llvm_link: PathBuf,
    pub llvm_ar: PathBuf,
}

impl LtoTools {
    pub fn find(clang: &Path) -> Self {
        let llvm_link = toolchain::require_llvm_tool("llvm-link", clang);
        let llvm_ar = toolchain::require_llvm_tool("llvm-ar", clang);

        Self { llvm_link, llvm_ar }
    }
}

/// The tools and flags that turn SVT's bitcode archive into a native one.
pub struct LtoBuild<'a> {
    pub tools: &'a LtoTools,
    pub clang: &'a Path,
    pub c_flags: &'a [String],
    pub is_msvc: bool,
}

/// Merges the bitcode objects in `archive` into one native object, returning the dir of the new archive.
///
/// Native objects, such as NASM output, are packed beside the merged object unchanged.
pub fn build_lto_archive(lto_build: &LtoBuild, archive: &Path) -> PathBuf {
    let out_dir = target::out_dir();
    let objects_dir = out_dir.join("svt-objects");
    recreate_dir(&objects_dir);

    let members = archive_members(lto_build.tools, archive);
    extract_archive(lto_build.tools, archive, &objects_dir);

    let (bitcode, native) = split_members(&objects_dir, members);
    if bitcode.is_empty() {
        let shown_archive = archive.display();
        panic!("{shown_archive} holds no bitcode, so SVT was not built with -flto");
    }

    let merged_bitcode = "svt-merged.bc";
    let mut link_command = Command::new(&lto_build.tools.llvm_link);
    link_command
        .current_dir(&objects_dir)
        .args(&bitcode)
        .arg("-o")
        .arg(merged_bitcode);
    toolchain::run_tool(&mut link_command);

    let merged_object = "svt-merged.o";
    let mut compile_command = merged_compile_command(lto_build, merged_bitcode, merged_object);
    compile_command.current_dir(&objects_dir);
    toolchain::run_tool(&mut compile_command);

    let lto_dir = out_dir.join("svt-lto");
    recreate_dir(&lto_dir);

    let archive_name = target::static_library_name(LTO_LIBRARY);
    let lto_archive = lto_dir.join(archive_name);
    let mut archive_command = Command::new(&lto_build.tools.llvm_ar);
    archive_command.current_dir(&objects_dir);

    if lto_build.is_msvc {
        archive_command.arg("--format=coff");
    }

    archive_command
        .arg("rcs")
        .arg(&lto_archive)
        .arg(merged_object)
        .args(&native);
    toolchain::run_tool(&mut archive_command);

    lto_dir
}

/// The clang command that compiles the merged bitcode to one native object.
///
/// It takes the base flags of SVT's C compile, so the target, PIC and section flags match.
fn merged_compile_command(lto_build: &LtoBuild, input: &str, output: &str) -> Command {
    let c_config = library::c_config(lto_build.clang);
    let compiler = c_config.get_compiler();
    let base_flags: Vec<String> = compiler
        .args()
        .iter()
        .map(|flag| flag.to_string_lossy().into_owned())
        .collect();
    let flags = llvm::merged_compile_flags(&base_flags, lto_build.c_flags, lto_build.is_msvc);

    let compile_flag = if lto_build.is_msvc { "/c" } else { "-c" };
    let mut command = Command::new(lto_build.clang);
    command
        .args(&flags)
        .arg(compile_flag)
        .arg(input)
        .arg("-o")
        .arg(output);
    command
}

fn recreate_dir(directory: &Path) {
    if directory.exists() {
        let removed = fs::remove_dir_all(directory);
        if let Err(error) = removed {
            panic!("could not remove {}: {error}", directory.display());
        }
    }

    let created = fs::create_dir_all(directory);
    if let Err(error) = created {
        panic!("could not create {}: {error}", directory.display());
    }
}

/// The member names of `archive` in order, failing if two share a name as extracting would lose one.
fn archive_members(tools: &LtoTools, archive: &Path) -> Vec<String> {
    let mut list_command = Command::new(&tools.llvm_ar);
    list_command.arg("t").arg(archive);
    let listing = toolchain::tool_output(&mut list_command);

    let members: Vec<String> = listing.lines().map(str::to_owned).collect();
    let mut seen = HashSet::new();
    for member in &members {
        let is_new = seen.insert(member.as_str());
        if !is_new {
            panic!("{} has two members named {member}", archive.display());
        }
    }

    members
}

fn extract_archive(tools: &LtoTools, archive: &Path, objects_dir: &Path) {
    let mut extract_command = Command::new(&tools.llvm_ar);
    extract_command.current_dir(objects_dir).arg("x").arg(archive);
    toolchain::run_tool(&mut extract_command);
}

/// Splits the extracted members into bitcode and native objects, keeping archive order.
fn split_members(objects_dir: &Path, members: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut bitcode = Vec::new();
    let mut native = Vec::new();

    for member in members {
        let member_path = objects_dir.join(&member);
        let header = read_header(&member_path);
        if llvm::is_bitcode(&header) {
            bitcode.push(member);
        } else {
            native.push(member);
        }
    }

    (bitcode, native)
}

fn read_header(path: &Path) -> Vec<u8> {
    let file = File::open(path);
    let file = match file {
        Ok(file) => file,
        Err(error) => panic!("could not open {}: {error}", path.display()),
    };

    let mut header = Vec::with_capacity(4);
    let read = file.take(4).read_to_end(&mut header);
    if let Err(error) = read {
        panic!("could not read {}: {error}", path.display());
    }

    header
}
