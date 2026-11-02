use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches, Parser};

use crate::commands::Command;
use crate::commands::passthrough::{SplitArgs, split_args};

#[derive(Debug, Parser)]
struct TestCli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, global = true)]
    log_level: Option<String>,
}

fn split(arguments: &[&str]) -> SplitArgs {
    let args = os_strings(arguments);
    let cli = TestCli::command();

    split_args(&cli, args)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn os_strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn parse_and_validate(arguments: &[&str]) -> Result<Command, clap::Error> {
    let mut cli = TestCli::command();
    let args = os_strings(arguments);
    let split = split_args(&cli, args);
    let matches = cli.try_get_matches_from_mut(split.clap_args)?;
    let mut parsed = TestCli::from_arg_matches(&matches)?;
    parsed.command.set_svt_args(split.svt_args);

    let name = matches.subcommand_name().expect("a subcommand was parsed");
    let subcommand = cli.find_subcommand(name).expect("the subcommand exists");
    parsed.command.validate(subcommand)?;

    Ok(parsed.command)
}

#[test]
fn svt_flags_after_rav1oli_flags_are_split() {
    let arguments = [
        "test",
        "run",
        "-i",
        "a",
        "-o",
        "b",
        "--gpu",
        "0",
        "--crf",
        "30",
        "--ac-bias",
        "1",
    ];
    let split = split(&arguments);

    assert_eq!(split.svt_args, strings(&["--ac-bias", "1"]));
    let expected = os_strings(&["test", "run", "-i", "a", "-o", "b", "--gpu", "0", "--crf", "30"]);
    assert_eq!(split.clap_args, expected);
}

#[test]
fn svt_flags_before_and_between_are_split() {
    let arguments = [
        "test",
        "run",
        "--ac-bias",
        "1",
        "-i",
        "a",
        "--tune",
        "0",
        "-o",
        "b",
    ];
    let split = split(&arguments);

    assert_eq!(split.svt_args, strings(&["--ac-bias", "1", "--tune", "0"]));
    assert_eq!(
        split.clap_args,
        os_strings(&["test", "run", "-i", "a", "-o", "b"])
    );
}

#[test]
fn boolean_flags_take_no_value() {
    let arguments = [
        "test",
        "analyse",
        "-i",
        "a",
        "-m",
        "b",
        "--no-temporal",
        "--tune",
        "0",
    ];
    let split = split(&arguments);

    assert_eq!(split.svt_args, strings(&["--tune", "0"]));
    let expected = os_strings(&["test", "analyse", "-i", "a", "-m", "b", "--no-temporal"]);
    assert_eq!(split.clap_args, expected);
}

#[test]
fn equals_forms_are_split() {
    let arguments = ["test", "run", "-i", "a", "-o", "b", "--tune=0", "--crf=30"];
    let split = split(&arguments);

    assert_eq!(split.svt_args, strings(&["--tune=0"]));
    let expected = os_strings(&["test", "run", "-i", "a", "-o", "b", "--crf=30"]);
    assert_eq!(split.clap_args, expected);
}

#[test]
fn the_global_log_level_stays_with_clap() {
    let arguments = [
        "test",
        "--log-level",
        "debug",
        "run",
        "-i",
        "a",
        "-o",
        "b",
        "--log-level",
        "warn",
    ];
    let split = split(&arguments);

    assert!(split.svt_args.is_empty());
    let expected = os_strings(&arguments);
    assert_eq!(split.clap_args, expected);
}

#[test]
fn negative_svt_values_stay_with_their_flag() {
    let split = split(&["test", "run", "-i", "a", "--sharpness", "-1", "-o", "b"]);

    assert_eq!(split.svt_args, strings(&["--sharpness", "-1"]));
}

#[test]
fn comma_lists_starting_with_a_dash_stay_with_their_flag() {
    let arguments = ["test", "run", "-i", "a", "--qindex-offsets", "-3,-2,0", "-o", "b"];
    let split = split(&arguments);

    assert_eq!(split.svt_args, strings(&["--qindex-offsets", "-3,-2,0"]));

    let command = parse_and_validate(&arguments);
    assert!(command.is_ok(), "{command:?}");
}

#[test]
fn a_flag_after_an_svt_flag_is_taken_as_its_value() {
    let arguments = ["test", "run", "-i", "a", "-o", "b", "--tune", "--ac-bias"];
    let error = parse_and_validate(&arguments).expect_err("--ac-bias is not a tune");

    let message = error.to_string();
    assert!(message.contains("tune"), "{message}");
    assert!(message.contains("--ac-bias"), "{message}");
}

#[test]
fn a_global_flag_gets_a_suggestion() {
    let arguments = ["test", "run", "-i", "a", "-o", "b", "--log-levl", "warn"];
    let error = parse_and_validate(&arguments).expect_err("--log-levl should be rejected");

    let message = error.to_string();
    assert!(message.contains("did you mean `--log-level`"), "{message}");
}

#[test]
fn everything_after_the_terminator_stays_with_clap() {
    let arguments = ["test", "run", "-i", "a", "-o", "b", "--", "--tune", "0"];
    let split = split(&arguments);

    assert!(split.svt_args.is_empty());
    let expected = os_strings(&arguments);
    assert_eq!(split.clap_args, expected);
}

#[test]
fn help_mixed_with_svt_flags_stays_with_clap() {
    let split = split(&["test", "run", "--tune", "0", "--help"]);

    assert_eq!(split.svt_args, strings(&["--tune", "0"]));
    let expected = os_strings(&["test", "run", "--help"]);
    assert_eq!(split.clap_args, expected);
}

#[test]
fn a_flag_where_a_rav1oli_value_belongs_is_an_error() {
    let arguments = ["test", "run", "-i", "a", "-o", "--weird-name", "1"];
    let result = parse_and_validate(&arguments);

    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_typo_of_a_rav1oli_flag_gets_a_suggestion() {
    let error = parse_and_validate(&["test", "run", "-i", "a", "-o", "b", "--gpus", "1"])
        .expect_err("--gpus should be rejected");

    assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    let message = error.to_string();
    assert!(message.contains("did you mean `--gpu`"), "{message}");
}

#[test]
fn a_typo_of_an_svt_flag_gets_a_suggestion() {
    let error = parse_and_validate(&["test", "run", "-i", "a", "-o", "b", "--ac-bais", "1"])
        .expect_err("--ac-bais should be rejected");

    assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    let message = error.to_string();
    assert!(message.contains("did you mean `--ac-bias`"), "{message}");
}

#[test]
fn an_unrelated_flag_gets_no_suggestion() {
    let error = parse_and_validate(&["test", "run", "-i", "a", "-o", "b", "--zzzzzzzzzzzz", "1"])
        .expect_err("--zzzzzzzzzzzz should be rejected");

    assert!(
        error
            .to_string()
            .contains("neither a rav1oli flag nor an SVT-AV1 parameter")
    );
    assert!(!error.to_string().contains("did you mean"), "{error}");
}

#[test]
fn encoder_owned_flags_are_rejected() {
    let error = parse_and_validate(&["test", "run", "-i", "a", "-o", "b", "--width", "100"])
        .expect_err("--width is set by rav1oli");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
}

#[test]
fn valid_svt_flags_are_accepted_and_stored_joined() {
    let command = parse_and_validate(&["test", "run", "-i", "a", "-o", "b", "--ac-bias", "1"])
        .expect("--ac-bias is an SVT-AV1 parameter");

    let Command::Run(args) = command else {
        panic!("expected the run command");
    };

    let expected = Some("--ac-bias 1".to_string());
    assert_eq!(args.encoder.svt_params_string(), expected);
}

#[test]
fn svt_params_are_stored_joined() {
    let mut command = parse_and_validate(&["test", "run", "-i", "a", "-o", "b"]).expect("should parse");
    command.set_svt_args(strings(&["--ac-bias", "1"]));

    let Command::Run(args) = command else {
        panic!("expected the run command");
    };

    let expected = Some("--ac-bias 1".to_string());
    assert_eq!(args.encoder.svt_params_string(), expected);
}

#[test]
fn no_svt_args_means_no_params_string() {
    let command = parse_and_validate(&["test", "run", "-i", "a", "-o", "b"]).expect("should parse");

    let Command::Run(args) = command else {
        panic!("expected the run command");
    };

    assert_eq!(args.encoder.svt_params_string(), None);
}
