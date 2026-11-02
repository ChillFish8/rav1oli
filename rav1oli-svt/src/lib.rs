//! AV1 encoding through the SVT-AV1-Essential fork
//!
//! The fork is built statically from its submodule with clang. [version] reports the version it was built at.
//!
//! [Encoder] takes 8-bit or 10-bit 4:2:0 frames, each with optional [DistortionGrids], and writes an IVF
//! file.

#[cfg(test)]
#[path = "../build/cpu.rs"]
mod cpu_flag;
mod encoder;
mod error;
mod grids;
mod ivf;
#[cfg(test)]
#[path = "../build/llvm.rs"]
mod llvm;
mod logging;
mod params;
mod sys;
#[cfg(test)]
mod tests;

use std::ffi::CStr;

pub use self::encoder::{ChromaPosition, ColourDescription, EncodeSummary, Encoder, EncoderSettings, Planes};
pub use self::error::{EncodeError, GridError, GridKind, ParamError};
pub use self::grids::{DistortionGrids, MAX_CUT, MAX_DROP};
pub use self::params::{check_params, is_encoder_owned, param_names};

/// The version string of the linked SVT-AV1 library.
pub fn version() -> String {
    // SAFETY: SVT returns a pointer to a static, NUL-terminated string.
    let raw_version = unsafe { sys::svt_av1_get_version() };
    // SAFETY: The pointer is non-null and lives for the whole program.
    let version = unsafe { CStr::from_ptr(raw_version) };

    version.to_string_lossy().into_owned()
}
