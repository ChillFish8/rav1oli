use std::ffi::{CStr, c_char, c_int};
use std::sync::Once;

use crate::sys;

static INSTALL: Once = Once::new();

/// Routes SVT-AV1's log output to `tracing`.
///
/// SVT reads the callback once, at the first `svt_av1_enc_init_handle` in the process, so this must run
/// before any handle is created.
pub(crate) fn install() {
    INSTALL.call_once(|| {
        let sink: sys::rav1oli_svt_log_sink = Some(emit_log);
        // SAFETY: The sink is a plain function that lives for the whole program.
        unsafe { sys::rav1oli_svt_install_log(sink) };
    });
}

/// Emits one formatted SVT message at the matching `tracing` level.
///
/// # Safety
///
/// `message` must be a NUL-terminated string, and `tag` must be null or one.
unsafe extern "C" fn emit_log(level: c_int, tag: *const c_char, message: *const c_char) {
    // SAFETY: Guaranteed by the caller.
    let raw_message = unsafe { CStr::from_ptr(message) };
    let message = raw_message.to_string_lossy();
    let message = message.trim_end();
    if message.is_empty() {
        return;
    }

    let tag = if tag.is_null() {
        "Svt"
    } else {
        // SAFETY: Guaranteed by the caller.
        let raw_tag = unsafe { CStr::from_ptr(tag) };
        raw_tag.to_str().unwrap_or("Svt")
    };

    match level {
        sys::SVT_AV1_LOG_FATAL | sys::SVT_AV1_LOG_ERROR => {
            tracing::error!(target: "svt_av1", tag, "{message}");
        },
        sys::SVT_AV1_LOG_WARN => tracing::warn!(target: "svt_av1", tag, "{message}"),
        sys::SVT_AV1_LOG_INFO => tracing::info!(target: "svt_av1", tag, "{message}"),
        sys::SVT_AV1_LOG_DEBUG => tracing::debug!(target: "svt_av1", tag, "{message}"),
        _ => tracing::trace!(target: "svt_av1", tag, "{message}"),
    }
}
