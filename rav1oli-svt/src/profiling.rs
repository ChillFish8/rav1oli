use std::ffi::c_int;
use std::hint;

unsafe extern "C" {
    /// Defined by clang's profile runtime beside the code that writes the profile at exit.
    #[link_name = "__llvm_profile_runtime"]
    static LLVM_PROFILE_RUNTIME: c_int;
}

/// Keeps clang's profile runtime in the link of an instrumented build.
///
/// clang links with `-u __llvm_profile_runtime` for this on Linux, which rustc does not pass.
pub(crate) fn keep_profile_runtime() {
    let runtime = &raw const LLVM_PROFILE_RUNTIME;
    hint::black_box(runtime);
}
