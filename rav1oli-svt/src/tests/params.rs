use crate::params::{DefaultConfig, check_requirements};
use crate::{ParamError, check_params};

fn flags(tokens: &[&str]) -> Vec<String> {
    tokens.iter().map(|token| token.to_string()).collect()
}

fn check(tokens: &[&str]) -> Result<(), ParamError> {
    let args = flags(tokens);
    check_params(&args)
}

#[test]
fn tune_and_ac_bias_are_accepted() {
    let result = check(&["--tune", "0", "--ac-bias", "1"]);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_single_dash_name_is_accepted() {
    let result = check(&["-tune", "0"]);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn an_equals_form_is_accepted() {
    let result = check(&["--tune=0"]);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn an_unknown_name_is_an_error() {
    let error = check(&["--not-a-thing", "1"]).unwrap_err();
    let message = error.to_string();

    assert!(matches!(error, ParamError::Unknown { .. }), "{error:?}");
    assert!(message.contains("not-a-thing"), "{message}");
}

#[test]
fn list_flags_that_allocate_are_accepted_and_released() {
    let result = check(&[
        "--sframe-posi",
        "10,20",
        "--sframe-qp",
        "30",
        "--frame-resz-events",
        "5,9",
    ]);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_bad_value_is_an_error() {
    let error = check(&["--tune", "banana"]).unwrap_err();
    let ParamError::Rejected { name, value } = error else {
        panic!("expected Rejected, got {error:?}");
    };

    assert_eq!(name, "tune");
    assert_eq!(value, "banana");
}

#[test]
fn a_flag_without_a_value_is_an_error() {
    let error = check(&["--tune"]).unwrap_err();
    let ParamError::MissingValue { name } = error else {
        panic!("expected MissingValue, got {error:?}");
    };

    assert_eq!(name, "tune");
}

#[test]
fn a_stray_value_is_an_error() {
    let error = check(&["0"]).unwrap_err();
    let ParamError::StrayValue { token } = error else {
        panic!("expected StrayValue, got {error:?}");
    };

    assert_eq!(token, "0");
}

#[test]
fn a_negative_number_is_a_stray_value() {
    let error = check(&["-1"]).unwrap_err();
    let ParamError::StrayValue { token } = error else {
        panic!("expected StrayValue, got {error:?}");
    };

    assert_eq!(token, "-1");
}

#[test]
fn a_negative_value_after_a_flag_is_accepted() {
    let result = check(&["--key-frame-qindex-offset", "-3"]);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn encoder_owned_keys_are_errors() {
    let owned = [
        ("--crf", "30", "crf"),
        ("--preset", "4", "preset"),
        ("--width", "1920", "width"),
        ("-w", "1920", "w"),
        ("--input-depth", "10", "input-depth"),
        ("--enable-distortion-map", "0", "enable-distortion-map"),
        ("--asm", "max", "asm"),
        ("--qp", "20", "qp"),
        ("-q", "20", "q"),
        ("--rc", "0", "rc"),
    ];

    for (flag, value, expected) in owned {
        let error = check(&[flag, value]).unwrap_err();
        let ParamError::EncoderOwned { name } = error else {
            panic!("{flag}: expected EncoderOwned, got {error:?}");
        };

        assert_eq!(name, expected);
    }
}

fn requirement_reason(tokens: &[&str]) -> &'static str {
    let error = check(tokens).unwrap_err();
    let ParamError::Requirement { reason } = error else {
        panic!("expected Requirement, got {error:?}");
    };

    reason
}

#[test]
fn aq_mode_1_breaks_a_requirement() {
    let reason = requirement_reason(&["--aq-mode", "1"]);
    assert!(reason.contains("aq-mode"), "{reason}");
}

#[test]
fn a_max_bitrate_breaks_a_requirement() {
    let reason = requirement_reason(&["--mbr", "1000"]);
    assert!(reason.contains("max bitrate"), "{reason}");
}

#[test]
fn resize_breaks_a_requirement() {
    let reason = requirement_reason(&["--resize-mode", "1"]);
    assert!(reason.contains("resizing"), "{reason}");
}

#[test]
fn superres_breaks_a_requirement() {
    let reason = requirement_reason(&["--superres-mode", "1"]);
    assert!(reason.contains("super resolution"), "{reason}");
}

#[test]
fn a_non_crf_rate_control_breaks_a_requirement() {
    let mut defaults = DefaultConfig::new().unwrap();
    defaults.config.rate_control_mode = 1;

    let error = check_requirements(&defaults.config).unwrap_err();
    let ParamError::Requirement { reason } = error else {
        panic!("expected Requirement, got {error:?}");
    };

    assert!(reason.contains("CRF"), "{reason}");
}

#[test]
fn an_roi_map_breaks_a_requirement() {
    let mut defaults = DefaultConfig::new().unwrap();
    defaults.config.enable_roi_map = true;

    let error = check_requirements(&defaults.config).unwrap_err();
    let ParamError::Requirement { reason } = error else {
        panic!("expected Requirement, got {error:?}");
    };

    assert!(reason.contains("ROI"), "{reason}");
}
