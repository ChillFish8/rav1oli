use std::ffi::{CString, c_char};
use std::ptr;

use crate::error::VshipError;

/// The trix displays, more sane default displays for people to use.
const TRIX_CONFIG: &str = include_str!("../configs/trix_cvvdp_config.json");

/// The display CVVDP models viewing on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DisplayModel {
    /// A 27 inch 1080p SDR monitor at 203 nits in a dim room.
    #[default]
    TrixFhd,
    /// A 32 inch 4K SDR monitor at 203 nits in a dim room.
    TrixUhdSdr,
    /// A 32 inch 4K HDR monitor at 600 nits in a dim room.
    TrixUhdHdr,
    Standard4k,
    StandardFhd,
    StandardHdrPq,
    StandardHdrHlg,
    StandardHdrDark,
    StandardHdrLinearZoom,
    /// Adds a display, or overrides a built-in one, from inline JSON in Vship's display config
    /// format.
    Custom {
        name: String,
        json: String,
    },
}

impl DisplayModel {
    fn key(&self) -> &str {
        match self {
            Self::TrixFhd => "trix_fhd",
            Self::TrixUhdSdr => "trix_uhd_sdr",
            Self::TrixUhdHdr => "trix_uhd_hdr",
            Self::Standard4k => "standard_4k",
            Self::StandardFhd => "standard_fhd",
            Self::StandardHdrPq => "standard_hdr_pq",
            Self::StandardHdrHlg => "standard_hdr_hlg",
            Self::StandardHdrDark => "standard_hdr_dark",
            Self::StandardHdrLinearZoom => "standard_hdr_linear_zoom",
            Self::Custom { name, .. } => name,
        }
    }

    /// Refuses custom JSON that Vship would read as a file path.
    pub(crate) fn check(&self) -> Result<(), VshipError> {
        let Self::Custom { json, .. } = self else {
            return Ok(());
        };

        let trimmed = json.trim_start();
        if !trimmed.starts_with('{') {
            return Err(not_inline_json_error());
        }

        Ok(())
    }

    /// The inline JSON defining this display, if Vship does not build it in.
    fn config(&self) -> Option<&str> {
        match self {
            Self::TrixFhd | Self::TrixUhdSdr | Self::TrixUhdHdr => Some(TRIX_CONFIG),
            Self::Custom { json, .. } => Some(json.trim_start()),
            _ => None,
        }
    }

    /// The model key and config Vship's init takes.
    pub(crate) fn c_strings(&self) -> Result<DisplayStrings, VshipError> {
        let key = c_string(self.key(), "display name")?;
        let config = match self.config() {
            Some(config) => Some(c_string(config, "display config")?),
            None => None,
        };

        Ok(DisplayStrings { key, config })
    }
}

/// A display model's key and config as C strings, kept alive across Vship's init.
pub(crate) struct DisplayStrings {
    key: CString,
    config: Option<CString>,
}

impl DisplayStrings {
    pub(crate) fn key_pointer(&self) -> *const c_char {
        self.key.as_ptr()
    }

    /// The config pointer, null for a display Vship builds in.
    pub(crate) fn config_pointer(&self) -> *const c_char {
        match &self.config {
            Some(config) => config.as_ptr(),
            None => ptr::null(),
        }
    }
}

fn c_string(text: &str, name: &str) -> Result<CString, VshipError> {
    match CString::new(text) {
        Ok(converted) => Ok(converted),
        Err(_) => Err(nul_byte_error(name)),
    }
}

fn not_inline_json_error() -> VshipError {
    VshipError::new("a custom display config must be inline JSON starting with `{`, not a path")
}

fn nul_byte_error(name: &str) -> VshipError {
    VshipError::new(format!("the {name} carries a nul byte, which vship cannot take"))
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;

    fn key_of(model: &DisplayModel) -> String {
        let strings = model.c_strings().expect("converts the model");
        // SAFETY: `key_pointer` points at a NUL terminated string `strings` owns.
        let key = unsafe { CStr::from_ptr(strings.key_pointer()) };

        key.to_string_lossy().into_owned()
    }

    fn config_of(model: &DisplayModel) -> String {
        let strings = model.c_strings().expect("converts the model");
        // SAFETY: `config_pointer` points at a NUL terminated string `strings` owns.
        let config = unsafe { CStr::from_ptr(strings.config_pointer()) };

        config.to_string_lossy().into_owned()
    }

    fn custom(name: &str, json: &str) -> DisplayModel {
        DisplayModel::Custom {
            name: name.to_string(),
            json: json.to_string(),
        }
    }

    #[test]
    fn built_in_models_use_vships_keys() {
        let expected = [
            (DisplayModel::Standard4k, "standard_4k"),
            (DisplayModel::StandardFhd, "standard_fhd"),
            (DisplayModel::StandardHdrPq, "standard_hdr_pq"),
            (DisplayModel::StandardHdrHlg, "standard_hdr_hlg"),
            (DisplayModel::StandardHdrDark, "standard_hdr_dark"),
            (DisplayModel::StandardHdrLinearZoom, "standard_hdr_linear_zoom"),
        ];

        for (model, key) in expected {
            assert_eq!(key_of(&model), key);
        }
    }

    #[test]
    fn the_default_model_is_trix_fhd() {
        assert_eq!(DisplayModel::default(), DisplayModel::TrixFhd);
    }

    #[test]
    fn trix_models_pass_their_key_and_the_embedded_config() {
        let expected = [
            (DisplayModel::TrixFhd, "trix_fhd"),
            (DisplayModel::TrixUhdSdr, "trix_uhd_sdr"),
            (DisplayModel::TrixUhdHdr, "trix_uhd_hdr"),
        ];

        for (model, key) in expected {
            let config = config_of(&model);

            assert_eq!(key_of(&model), key);
            assert_eq!(config, TRIX_CONFIG);
            assert!(
                config.contains(&format!("\"{key}\"")),
                "{key} is missing from the config"
            );
        }
    }

    #[test]
    fn a_built_in_model_passes_no_config() {
        let strings = DisplayModel::StandardFhd.c_strings().expect("converts the model");

        assert!(strings.config_pointer().is_null());
    }

    #[test]
    fn a_custom_model_passes_its_name_and_config() {
        let model = custom("my_display", "{}");
        let strings = model.c_strings().expect("converts the model");

        assert_eq!(key_of(&model), "my_display");
        assert!(!strings.config_pointer().is_null());
    }

    #[test]
    fn custom_json_that_is_not_inline_is_refused() {
        let model = custom("my_display", "displays/my_display.json");

        let error = model.check().expect_err("a path passed");

        assert!(error.message().contains("inline JSON"), "{error}");
    }

    #[test]
    fn custom_json_may_start_with_whitespace() {
        let model = custom("my_display", "  \n{}");

        let result = model.check();
        let config = config_of(&model);

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(config, "{}");
    }

    #[test]
    fn a_nul_byte_in_a_custom_name_is_refused() {
        let model = custom("my\0display", "{}");

        let Err(error) = model.c_strings() else {
            panic!("a nul name converted");
        };

        assert!(error.message().contains("display name"), "{error}");
    }
}
