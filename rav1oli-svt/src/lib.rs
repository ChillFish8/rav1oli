//! AV1 encoding through the SVT-AV1-Essential fork
//!
//! The fork is built statically from its submodule with clang. [version] reports the version it was built at.

#[cfg(test)]
#[path = "../build/cpu.rs"]
mod cpu_flag;
mod sys;
#[cfg(test)]
mod tests;

use std::ffi::CStr;

/// The version string of the linked SVT-AV1 library.
pub fn version() -> String {
    // SAFETY: SVT returns a pointer to a static, NUL-terminated string.
    let raw_version = unsafe { sys::svt_av1_get_version() };
    // SAFETY: The pointer is non-null and lives for the whole program.
    let version = unsafe { CStr::from_ptr(raw_version) };

    version.to_string_lossy().into_owned()
}
