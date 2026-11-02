use std::path::{Path, PathBuf};

use rav1oli_vship::DisplayModel;

use crate::metrics::select_display;

fn write_config(directory: &Path, contents: &str) -> PathBuf {
    let path = directory.join("displays.json");
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn fhd_content_defaults_to_trix_fhd() {
    let selected = select_display(None, None, 1920, 1080).unwrap();

    assert_eq!(selected.model, DisplayModel::TrixFhd);
    assert_eq!(selected.settings.model, "trix_fhd");
    assert_eq!(selected.settings.config, None);
}

#[test]
fn uhd_content_defaults_to_trix_uhd_hdr() {
    let selected = select_display(None, None, 3840, 2160).unwrap();

    assert_eq!(selected.model, DisplayModel::TrixUhdHdr);
    assert_eq!((selected.width, selected.height), (3840, 2160));
}

#[test]
fn other_resolutions_need_a_display_model() {
    let error = select_display(None, None, 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("--display-model"));
}

#[test]
fn a_built_in_model_with_the_wrong_resolution_is_an_error() {
    let error = select_display(Some("trix_fhd"), None, 3840, 2160).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("1920x1080"));
    assert!(message.contains("3840x2160"));
}

#[test]
fn an_unknown_model_is_an_error() {
    let error = select_display(Some("nope"), None, 1920, 1080).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("nope"));
    assert!(message.contains("trix_fhd"));
}

#[test]
fn a_custom_model_uses_its_config_resolution() {
    let directory = tempfile::tempdir().unwrap();
    let contents = r#"{ "mine": { "resolution": [1280, 720], "diagonal_size_inches": 24 } }"#;
    let path = write_config(directory.path(), contents);

    let selected = select_display(Some("mine"), Some(&path), 1280, 720).unwrap();

    let expected = DisplayModel::Custom {
        name: "mine".to_string(),
        json: contents.to_string(),
    };
    assert_eq!(selected.model, expected);
    assert_eq!(selected.settings.config.as_deref(), Some(contents));
}

#[test]
fn a_custom_model_without_a_resolution_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_config(directory.path(), r#"{ "mine": {} }"#);

    let error = select_display(Some("mine"), Some(&path), 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("displays.json"));
    assert!(message.contains("mine"));
}

#[test]
fn invalid_config_json_names_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_config(directory.path(), "not json");

    let error = select_display(Some("mine"), Some(&path), 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("displays.json"));
}

#[test]
fn a_config_without_a_model_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_config(directory.path(), "{}");

    let error = select_display(None, Some(&path), 1920, 1080).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("--display-model"));
}

#[test]
fn bad_resolutions_are_errors() {
    let bad_resolutions = [
        "[1280]",
        "[1280, 720, 1]",
        "[-1280, 720]",
        "[1280.5, 720]",
        "[4294967296, 720]",
        "\"1280x720\"",
    ];
    let directory = tempfile::tempdir().unwrap();

    for resolution in bad_resolutions {
        let contents = format!(r#"{{ "mine": {{ "resolution": {resolution} }} }}"#);
        let path = write_config(directory.path(), &contents);

        let result = select_display(Some("mine"), Some(&path), 1280, 720);

        assert!(result.is_err(), "{resolution} should be rejected");
    }
}

#[test]
fn a_missing_config_file_names_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("absent.json");

    let error = select_display(Some("mine"), Some(&path), 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("absent.json"));
}

#[test]
fn a_non_object_config_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_config(directory.path(), "[1, 2]");

    let error = select_display(Some("mine"), Some(&path), 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("displays.json"));
}

#[test]
fn a_bad_entry_that_is_not_selected_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let contents = r#"{
        "mine": { "resolution": [1280, 720] },
        "broken": { "resolution": [1280] }
    }"#;
    let path = write_config(directory.path(), contents);

    let error = select_display(Some("mine"), Some(&path), 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("displays.json"));
    assert!(message.contains("broken"));
}

#[test]
fn a_model_missing_from_the_config_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let contents = r#"{ "mine": { "resolution": [1920, 1080] } }"#;
    let path = write_config(directory.path(), contents);

    let error = select_display(Some("trix_fhd"), Some(&path), 1920, 1080).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("trix_fhd"));
    assert!(message.contains("displays.json"));
}

#[test]
fn a_close_built_in_name_is_suggested() {
    let error = select_display(Some("trix_uhd"), None, 3840, 2160).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("did you mean"));
}

#[test]
fn a_close_config_name_is_suggested() {
    let directory = tempfile::tempdir().unwrap();
    let contents = r#"{ "office_monitor": { "resolution": [1280, 720] } }"#;
    let path = write_config(directory.path(), contents);

    let error = select_display(Some("office_monitr"), Some(&path), 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("did you mean 'office_monitor'"));
}

#[test]
fn an_unrelated_name_gets_no_suggestion() {
    let error = select_display(Some("zzzz"), None, 1920, 1080).unwrap_err();

    let message = error.to_string();
    assert!(!message.contains("did you mean"));
    assert!(message.contains("trix_fhd"));
}

#[test]
fn a_config_with_no_models_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_config(directory.path(), "{}");

    let error = select_display(Some("mine"), Some(&path), 1280, 720).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("contains no display models"), "{message}");
    assert!(message.contains("displays.json"), "{message}");
}
