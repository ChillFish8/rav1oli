use std::ffi::CString;
use std::mem::MaybeUninit;
use std::ptr;

use crate::error::ParamError;
use crate::{logging, sys};

/// Every name the fork's parser accepts, aliases included.
///
/// Copied from `svt_av1_enc_parse_parameter` in `Source/Lib/Globals/enc_settings.c`.
const PARAM_NAMES: &[&str] = &[
    // Handled by name before the option tables.
    "keyint",
    "min-keyint",
    "speed",
    "quality",
    "tbr",
    "mbr",
    "crf",
    "rc",
    "profile",
    "color-format",
    "irefresh-type",
    "sframe-mode",
    "asm",
    "color-primaries",
    "transfer-characteristics",
    "matrix-coefficients",
    "color-range",
    "chroma-sample-position",
    "mastering-display",
    "content-light",
    "qindex-offsets",
    "chroma-qindex-offsets",
    "lambda-scale-factors",
    "frame-resz-events",
    "frame-resz-kf-denoms",
    "frame-resz-denoms",
    "sframe-posi",
    "sframe-qp",
    "sframe-qp-offset",
    "zones",
    // The `uint_opts` table.
    "w",
    "width",
    "h",
    "height",
    "q",
    "qp",
    "film-grain",
    "photon-noise",
    "hierarchical-levels",
    "tier",
    "level",
    "lp",
    "pin",
    "fps-num",
    "fps-denom",
    "lookahead",
    "scd",
    "max-qp",
    "min-qp",
    "minsection-pct",
    "maxsection-pct",
    "undershoot-pct",
    "overshoot-pct",
    "mbr-overshoot-pct",
    "recode-loop",
    "enable-stat-report",
    "scm",
    "input-depth",
    "forced-max-frame-width",
    "forced-max-frame-height",
    // The `uint8_opts` table.
    "pred-struct",
    "aq-mode",
    "superres-mode",
    "superres-qthres",
    "superres-kf-qthres",
    "superres-denom",
    "superres-kf-denom",
    "tune",
    "film-grain-denoise",
    "photon-noise-chroma",
    "enable-dlf",
    "resize-mode",
    "resize-denom",
    "resize-kf-denom",
    "qm-min",
    "qm-max",
    "chroma-qm-min",
    "chroma-qm-max",
    "use-fixed-qindex-offsets",
    "startup-mg-size",
    "variance-boost-strength",
    "variance-octile",
    "variance-boost-curve",
    "variance-dark-boost",
    "variance-bright-attenuation",
    "qp-scale-compress-strength",
    "balancing-q-bias",
    "optimize-b-mode",
    "fast-decode",
    "luminance-qp-bias",
    "enable-tf",
    "tf-strength",
    "max-tx-size",
    "noise-norm-strength",
    "sharp-tx",
    "tx-bias",
    "complex-hvs",
    "noise-adaptive-filtering",
    "enable-alt-cdef",
    "enable-alt-dlf",
    "distortion-bias-preset",
    // The `int64_opts` table.
    "buf-initial-sz",
    "buf-optimal-sz",
    "buf-sz",
    // The `double_opts` table.
    "ac-bias",
    // The `int_opts` table.
    "key-frame-chroma-qindex-offset",
    "key-frame-qindex-offset",
    "luma-y-dc-qindex-offset",
    "chroma-u-dc-qindex-offset",
    "chroma-u-ac-qindex-offset",
    "chroma-v-dc-qindex-offset",
    "chroma-v-ac-qindex-offset",
    "pass",
    "enable-cdef",
    "enable-restoration",
    "enable-mfmv",
    "intra-period",
    "tile-rows",
    "tile-columns",
    "sframe-dist",
    "webm",
    // The `int8_opts` table.
    "balancing-r0-dampening-layer",
    "preset",
    "sharpness",
    "startup-qp-offset",
    "enable-qmpsnr",
    // The `bool_opts` table.
    "use-q-file",
    "enable-overlays",
    "enable-force-key-frames",
    "enable-qm",
    "enable-dg",
    "gop-constraint-rc",
    "enable-variance-boost",
    "enable-distortion-map",
    "lossless",
    "avif",
    "rtc",
    "adaptive-film-grain",
    "auto-tiling",
    "low-memory",
    "hide-banner",
];

/// The names from the same parser that rav1oli sets itself.
///
/// These are the preset, quality, size, frame rate, bit depth, colour and distortion map entries
/// of `svt_av1_enc_parse_parameter` in `Source/Lib/Globals/enc_settings.c`, aliases included.
/// `q`, `qp` and `rc` are included because they set the CRF value and the rate control mode.
const ENCODER_OWNED: &[&str] = &[
    "preset",
    "crf",
    "q",
    "qp",
    "rc",
    "speed",
    "quality",
    "w",
    "width",
    "h",
    "height",
    "fps-num",
    "fps-denom",
    "input-depth",
    "color-format",
    "asm",
    "enable-distortion-map",
    "color-primaries",
    "transfer-characteristics",
    "matrix-coefficients",
    "color-range",
    "chroma-sample-position",
];

pub(crate) const SVT_SUCCESS: sys::EbErrorType = sys::EB_ErrorNone;

/// The only prediction structure the fork encodes, `RANDOM_ACCESS` in `Source/Lib/Codec/definitions.h`.
const RANDOM_ACCESS: u8 = 2;

/// Whether rav1oli sets this flag itself, so it cannot be passed through.
pub fn is_encoder_owned(name: &str) -> bool {
    ENCODER_OWNED.contains(&name)
}

/// Every long name the fork's parser accepts.
pub fn param_names() -> &'static [&'static str] {
    PARAM_NAMES
}

/// Checks that the SVT-AV1 flags are accepted by the fork and meet the distortion map requirements.
///
/// The flags are also set on an encoder for a nominal 1080p 10-bit 4:2:0 input, which runs the fork's
/// own checks of how the flags combine.
pub fn check_params(args: &[String]) -> Result<(), ParamError> {
    let mut guard = DefaultConfig::new()?;
    apply_nominal_input(&mut guard.config);

    apply_params(&mut guard.config, args)?;
    check_requirements(&guard.config)?;

    // The dry run would otherwise print SVT's settings banner.
    guard.config.hide_banner = true;

    let handle = guard.handle();
    // SAFETY: The handle and config are valid. SVT deep-copies the config except for its zones, which the
    // guard keeps alive until after the handle is released.
    let status = unsafe { sys::svt_av1_enc_set_parameter(handle, &mut guard.config) };
    if status != SVT_SUCCESS {
        let flags = args.join(" ");
        return Err(ParamError::Combination { flags, status });
    }

    Ok(())
}

/// Sets the input fields rav1oli always sets, for a nominal 1080p input.
fn apply_nominal_input(config: &mut sys::EbSvtAv1EncConfiguration) {
    config.source_width = 1920;
    config.source_height = 1080;
    config.encoder_bit_depth = 10;
    config.encoder_color_format = sys::EB_YUV420;
    config.enable_distortion_map = true;
}

/// Applies SVT-AV1 flags such as `--tune 0` or `--tune=0` to the config.
///
/// A token starting with `--` is never taken as a value, as no SVT-AV1 value starts that way.
pub(crate) fn apply_params(
    config: &mut sys::EbSvtAv1EncConfiguration,
    args: &[String],
) -> Result<(), ParamError> {
    let mut tokens = args.iter().peekable();

    while let Some(token) = tokens.next() {
        let is_number = token.parse::<f64>().is_ok();
        let stripped = strip_dashes(token).filter(|_| !is_number);
        let Some(stripped) = stripped else {
            return Err(ParamError::StrayValue { token: token.clone() });
        };

        let (name, inline_value) = match stripped.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (stripped, None),
        };

        if ENCODER_OWNED.contains(&name) {
            return Err(ParamError::EncoderOwned {
                name: name.to_string(),
            });
        }

        if !PARAM_NAMES.contains(&name) {
            return Err(ParamError::Unknown {
                name: name.to_string(),
            });
        }

        let value = match inline_value {
            Some(value) => Some(value),
            None => tokens.next_if(|next| !next.starts_with("--")).map(String::as_str),
        };
        let Some(value) = value else {
            return Err(ParamError::MissingValue {
                name: name.to_string(),
            });
        };

        parse_one(config, name, value)?;
    }

    Ok(())
}

/// Checks the settings the distortion map and the fork's single prediction structure depend on.
pub(crate) fn check_requirements(config: &sys::EbSvtAv1EncConfiguration) -> Result<(), ParamError> {
    let requirements = [
        (
            config.pred_structure != RANDOM_ACCESS,
            "the fork only encodes the random access prediction structure, `pred-struct` 2",
        ),
        (
            config.rtc,
            "`rtc` forces the low delay prediction structure, which the fork cannot encode",
        ),
        (
            config.rate_control_mode != sys::SVT_AV1_RC_MODE_CQP_OR_CRF as u8,
            "the distortion map needs CRF rate control",
        ),
        (
            config.max_bit_rate != 0,
            "the distortion map cannot be used with a max bitrate",
        ),
        (
            config.aq_mode == 1,
            "the distortion map cannot be used with `aq-mode` 1",
        ),
        (
            config.enable_roi_map,
            "the distortion map cannot be used with an ROI map",
        ),
        (
            config.superres_mode != 0,
            "the distortion map cannot be used with super resolution",
        ),
        (
            config.resize_mode != 0,
            "the distortion map cannot be used with resizing",
        ),
    ];

    for (broken, reason) in requirements {
        if broken {
            return Err(ParamError::Requirement { reason });
        }
    }

    Ok(())
}

/// Strips a leading `--` then `-`, as `SvtAv1EncApp` does. Gives `None` for a token with no dash.
fn strip_dashes(token: &str) -> Option<&str> {
    if !token.starts_with('-') {
        return None;
    }

    let without_long = token.strip_prefix("--").unwrap_or(token);
    let without_short = without_long.strip_prefix('-').unwrap_or(without_long);

    Some(without_short)
}

/// Applies one `name` and `value` pair through SVT's parser.
pub(crate) fn parse_one(
    config: &mut sys::EbSvtAv1EncConfiguration,
    name: &str,
    value: &str,
) -> Result<(), ParamError> {
    let rejected = || ParamError::Rejected {
        name: name.to_string(),
        value: value.to_string(),
    };

    let Ok(c_name) = CString::new(name) else {
        return Err(rejected());
    };

    let Ok(c_value) = CString::new(value) else {
        return Err(rejected());
    };

    // SAFETY: The config and both strings are valid for the call, and SVT copies what it keeps.
    let status = unsafe { sys::svt_av1_enc_parse_parameter(config, c_name.as_ptr(), c_value.as_ptr()) };
    if status != SVT_SUCCESS {
        return Err(rejected());
    }

    Ok(())
}

/// A default config together with the handle that produced it, released on drop.
///
/// Dropping also frees the lists and strings that SVT allocates into the config while parsing flags. The
/// handle goes first, as an encoder set up from the config keeps pointing at its zones.
pub(crate) struct DefaultConfig {
    handle: *mut sys::EbComponentType,
    pub(crate) config: sys::EbSvtAv1EncConfiguration,
}

impl DefaultConfig {
    pub(crate) fn new() -> Result<Self, ParamError> {
        logging::install();

        let mut handle: *mut sys::EbComponentType = ptr::null_mut();
        let mut config = MaybeUninit::<sys::EbSvtAv1EncConfiguration>::zeroed();

        // SAFETY: The handle and config pointers are valid. SVT fills the config with its defaults.
        let status = unsafe { sys::svt_av1_enc_init_handle(&mut handle, config.as_mut_ptr()) };
        if status != SVT_SUCCESS || handle.is_null() {
            return Err(ParamError::Setup { status });
        }

        // SAFETY: The config was zeroed, which is valid for its plain-data fields, then set by SVT.
        let config = unsafe { config.assume_init() };

        Ok(Self { handle, config })
    }

    pub(crate) fn handle(&self) -> *mut sys::EbComponentType {
        self.handle
    }
}

impl Drop for DefaultConfig {
    fn drop(&mut self) {
        // SAFETY: The handle came from `svt_av1_enc_init_handle` and is released exactly once.
        unsafe { sys::svt_av1_enc_deinit_handle(self.handle) };

        let scale_events = &mut self.config.frame_scale_evts;
        let s_frames = &mut self.config.sframe_posi;

        // SAFETY: The fork allocates each of these with plain `malloc` or `strdup` while parsing flags,
        // and nothing else owns them once the handle is released. The build always defines `NDEBUG`, so
        // SVT keeps no allocation tracker that bypassing its free macro would upset. Each is null or freed
        // once and then nulled.
        unsafe {
            free_pointer(&mut scale_events.start_frame_nums);
            free_pointer(&mut scale_events.resize_kf_denoms);
            free_pointer(&mut scale_events.resize_denoms);
            free_pointer(&mut s_frames.sframe_posis);
            free_pointer(&mut s_frames.sframe_qps);
            free_pointer(&mut s_frames.sframe_qp_offsets);
            free_pointer(&mut self.config.zones);
            free_pointer(&mut self.config.parsed_zones);
        }
    }
}

/// Frees a `malloc`ed pointer and sets it to null.
///
/// # Safety
///
/// The pointer must be null or come from the C allocator and not be freed elsewhere.
unsafe fn free_pointer<T>(pointer: &mut *mut T) {
    // SAFETY: Guaranteed by the caller. `free` accepts null.
    unsafe { libc::free((*pointer).cast()) };
    *pointer = ptr::null_mut();
}
