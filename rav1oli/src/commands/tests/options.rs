use std::path::PathBuf;

use clap::Parser;
use clap::error::ErrorKind;

use crate::commands::Command;
use crate::commands::options::{AnalysisOptions, DEFAULT_INTENSITY_TARGET};
use crate::decode::CacheLocation;
use crate::metrics::{BUTTERAUGLI_QUANTISATION, CVVDP_QUANTISATION};

#[derive(Debug, Parser)]
struct TestCli {
    #[command(subcommand)]
    command: Command,
}

fn cache_location(arguments: &[&str]) -> CacheLocation {
    let cli = TestCli::try_parse_from(arguments).expect("arguments should parse");

    match cli.command {
        Command::Analyse(args) => args.cache.location(),
        Command::Encode(args) => args.cache.location(),
        Command::Run(args) => args.cache.location(),
    }
}

#[test]
fn cache_dir_is_accepted_by_every_command() {
    let commands: [&[&str]; 3] = [
        &["test", "analyse", "-i", "a", "-m", "b", "--cache-dir", "c"],
        &[
            "test",
            "encode",
            "-i",
            "a",
            "-m",
            "b",
            "-o",
            "d",
            "--cache-dir",
            "c",
        ],
        &["test", "run", "-i", "a", "-o", "d", "--cache-dir", "c"],
    ];

    for arguments in commands {
        let location = cache_location(arguments);
        let expected = CacheLocation::Directory(PathBuf::from("c"));
        assert_eq!(location, expected, "arguments: {arguments:?}");
    }
}

#[test]
fn no_cache_dir_means_beside_the_video() {
    let location = cache_location(&["test", "run", "-i", "a", "-o", "d"]);

    assert_eq!(location, CacheLocation::BesideVideo);
}

fn analyse_options(extra: &[&str]) -> AnalysisOptions {
    let mut arguments = vec!["test", "analyse", "-i", "a", "-m", "b"];
    arguments.extend_from_slice(extra);
    let cli = TestCli::try_parse_from(arguments).expect("arguments should parse");

    match cli.command {
        Command::Analyse(args) => args.analysis,
        _ => panic!("expected the analyse command"),
    }
}

fn validation_error(extra: &[&str]) -> clap::Error {
    let options = analyse_options(extra);
    options.validate().expect_err("validation should fail")
}

#[test]
fn display_flags_need_cvvdp() {
    let model_error = validation_error(&["--metric", "butteraugli", "--display-model", "x"]);
    let model_message = model_error.to_string();
    assert_eq!(model_error.kind(), ErrorKind::ArgumentConflict);
    assert!(model_message.contains("--display-model"), "{model_message}");

    let config_error = validation_error(&[
        "--metric",
        "butteraugli",
        "--display-model",
        "x",
        "--display-config",
        "c.json",
    ]);
    let config_message = config_error.to_string();
    assert_eq!(config_error.kind(), ErrorKind::ArgumentConflict);
    let names_model = config_message.contains("--display-model");
    let names_config = config_message.contains("--display-config");
    assert!(names_model, "{config_message}");
    assert!(names_config, "{config_message}");

    let only_config = validation_error(&["--metric", "butteraugli", "--display-config", "c.json"]);
    let only_message = only_config.to_string();
    assert_eq!(only_config.kind(), ErrorKind::ArgumentConflict);
    assert!(only_message.contains("--display-config"), "{only_message}");
}

#[test]
fn intensity_target_needs_butteraugli() {
    let error = validation_error(&["--metric", "cvvdp", "--intensity-target", "100"]);
    let message = error.to_string();

    assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    assert!(message.contains("--intensity-target"), "{message}");
}

#[test]
fn display_config_needs_display_model() {
    let error = validation_error(&["--metric", "cvvdp", "--display-config", "c.json"]);
    let message = error.to_string();

    assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    assert!(message.contains("--display-config"), "{message}");
    assert!(message.contains("--display-model"), "{message}");
}

#[test]
fn cvvdp_accepts_display_flags() {
    let options = analyse_options(&[
        "--metric",
        "cvvdp",
        "--display-model",
        "x",
        "--display-config",
        "c.json",
    ]);

    options.validate().expect("cvvdp accepts display flags");
    assert_eq!(options.display_model.as_deref(), Some("x"));
    assert_eq!(options.display_config, Some(PathBuf::from("c.json")));
}

#[test]
fn butteraugli_accepts_intensity_target() {
    let options = analyse_options(&["--metric", "butteraugli", "--intensity-target", "100"]);

    options
        .validate()
        .expect("butteraugli accepts an intensity target");
    assert_eq!(options.intensity_target, Some(100.0));
}

#[test]
fn gpu_defaults_to_zero() {
    let options = analyse_options(&[]);

    assert_eq!(options.gpu, 0);
}

#[test]
fn floor_defaults_per_metric() {
    let butteraugli = analyse_options(&["--metric", "butteraugli"]);
    let cvvdp = analyse_options(&["--metric", "cvvdp"]);

    assert_eq!(butteraugli.quantisation(), BUTTERAUGLI_QUANTISATION);
    assert_eq!(cvvdp.quantisation(), CVVDP_QUANTISATION);
    assert_eq!(butteraugli.quantisation().floor, 0.5);
    assert_eq!(cvvdp.quantisation().floor, 0.0);
}

#[test]
fn a_floor_overrides_the_default() {
    let options = analyse_options(&["--metric", "butteraugli", "--floor", "1.5"]);
    let quantisation = options.quantisation();

    options.validate().expect("floor is valid");
    assert_eq!(quantisation.floor, 1.5);
    assert_eq!(quantisation.cap, 8.0);
}

#[test]
fn a_floor_at_or_above_the_cap_is_rejected() {
    let error = validation_error(&["--metric", "cvvdp", "--floor", "1.0"]);
    let message = error.to_string();

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(message.contains("--floor"), "{message}");
    assert!(message.contains("cap of 1"), "{message}");
}

#[test]
fn a_butteraugli_floor_error_names_its_cap() {
    let error = validation_error(&["--metric", "butteraugli", "--floor", "9"]);
    let message = error.to_string();

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(message.contains("cap of 8"), "{message}");
}

#[test]
fn a_negative_floor_is_rejected() {
    let error = validation_error(&["--floor=-1.0"]);
    let message = error.to_string();

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(message.contains("--floor"), "{message}");
}

#[test]
fn a_non_finite_floor_is_rejected() {
    let error = validation_error(&["--floor", "NaN"]);
    let message = error.to_string();

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(message.contains("--floor"), "{message}");
}

#[test]
fn intensity_target_defaults_to_203() {
    let options = analyse_options(&["--metric", "butteraugli"]);

    assert_eq!(DEFAULT_INTENSITY_TARGET, 203.0);
    assert_eq!(options.intensity_target(), 203.0);
}

#[test]
fn an_intensity_target_overrides_the_default() {
    let options = analyse_options(&["--metric", "butteraugli", "--intensity-target", "100"]);

    assert_eq!(options.intensity_target(), 100.0);
}

#[test]
fn unusable_intensity_targets_are_rejected() {
    let unusable = [
        "--intensity-target=0",
        "--intensity-target=-100",
        "--intensity-target=nan",
    ];

    for argument in unusable {
        let error = validation_error(&["--metric", "butteraugli", argument]);
        let message = error.to_string();

        assert_eq!(error.kind(), ErrorKind::ValueValidation, "{argument}");
        assert!(message.contains("--intensity-target"), "{message}");
    }
}
