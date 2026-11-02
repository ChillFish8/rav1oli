use std::ffi::OsString;

use clap::error::ErrorKind;
use rav1oli_svt::{ParamError, check_params, is_encoder_owned, param_names};

/// The minimum similarity for a "did you mean" suggestion, matching `--display-model`.
const SUGGESTION_THRESHOLD: f64 = 0.5;

/// Shown in the help of every command that encodes.
pub const SVT_FLAGS_HELP: &str = "Unrecognised `--flags` are passed to SVT-AV1 as encoder parameters.";

/// The command line split between clap and SVT-AV1.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct SplitArgs {
    pub clap_args: Vec<OsString>,
    pub svt_args: Vec<String>,
}

/// A long flag clap knows about.
#[derive(Debug, Clone, Eq, PartialEq)]
struct KnownFlag {
    name: String,
    takes_value: bool,
}

/// Splits the command line into the arguments clap parses and the `--flags` meant for SVT-AV1.
///
/// Any long flag that the selected subcommand and the global options do not define goes to SVT-AV1
/// along with its value.
pub fn split_args(command: &clap::Command, args: Vec<OsString>) -> SplitArgs {
    let mut split = SplitArgs::default();
    let mut known = flags_of(command, &[]);
    let mut subcommand_found = false;
    let mut tokens = args.into_iter();

    if let Some(program) = tokens.next() {
        split.clap_args.push(program);
    }

    while let Some(token) = tokens.next() {
        let Some(text) = token.to_str() else {
            split.clap_args.push(token);
            continue;
        };

        if text == "--" {
            split.clap_args.push(token);
            split.clap_args.extend(tokens);
            break;
        }

        if !subcommand_found && !text.starts_with('-') {
            let subcommand = command.find_subcommand(text);
            if let Some(subcommand) = subcommand {
                known = flags_of(subcommand, &global_flags(command));
                subcommand_found = true;
            }

            split.clap_args.push(token);
            continue;
        }

        let Some(long) = text.strip_prefix("--") else {
            split.clap_args.push(token);
            continue;
        };

        let (name, has_inline_value) = match long.split_once('=') {
            Some((name, _)) => (name, true),
            None => (long, false),
        };
        let flag = known.iter().find(|flag| flag.name == name);

        match flag {
            Some(flag) => {
                let takes_next = flag.takes_value && !has_inline_value;
                split.clap_args.push(token);
                if takes_next {
                    split.clap_args.extend(tokens.next());
                }
            },
            None => {
                let text = text.to_string();
                split.svt_args.push(text);
                if !has_inline_value {
                    take_svt_value(&mut tokens, &mut split);
                }
            },
        }
    }

    split
}

/// Moves the token after an SVT-AV1 flag across as its value, whatever it looks like.
fn take_svt_value(tokens: &mut std::vec::IntoIter<OsString>, split: &mut SplitArgs) {
    let Some(next) = tokens.next() else {
        return;
    };

    match next.to_str() {
        Some(text) => split.svt_args.push(text.to_string()),
        None => split.clap_args.push(next),
    }
}

fn global_flags(command: &clap::Command) -> Vec<KnownFlag> {
    command
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .filter_map(known_flag)
        .collect()
}

fn flags_of(command: &clap::Command, globals: &[KnownFlag]) -> Vec<KnownFlag> {
    let own = command.get_arguments().filter_map(known_flag);
    let builtins = ["help", "version"].map(|name| KnownFlag {
        name: name.to_string(),
        takes_value: false,
    });

    own.chain(globals.iter().cloned()).chain(builtins).collect()
}

fn known_flag(arg: &clap::Arg) -> Option<KnownFlag> {
    let name = arg.get_long()?;

    Some(KnownFlag {
        name: name.to_string(),
        takes_value: arg.get_action().takes_values(),
    })
}

/// The long flags the subcommand accepts, without the leading dashes.
pub fn long_flags(command: &clap::Command) -> Vec<String> {
    command
        .get_arguments()
        .filter_map(|arg| arg.get_long())
        .map(str::to_string)
        .collect()
}

/// Checks the SVT-AV1 flags, suggesting the closest flag if one is mistyped.
pub fn validate_svt_args(svt_args: &[String], rav1oli_flags: &[String]) -> Result<(), clap::Error> {
    let Err(error) = check_params(svt_args) else {
        return Ok(());
    };

    let (kind, message) = match &error {
        ParamError::Unknown { name } => {
            let message = unknown_flag_message(name, rav1oli_flags);
            (ErrorKind::UnknownArgument, message)
        },
        other => (ErrorKind::ValueValidation, other.to_string()),
    };

    Err(clap::Error::raw(kind, message))
}

fn unknown_flag_message(name: &str, rav1oli_flags: &[String]) -> String {
    let base = format!("`--{name}` is neither a rav1oli flag nor an SVT-AV1 parameter");
    let suggestion = closest_flag(name, rav1oli_flags);

    match suggestion {
        Some(flag) => format!("{base}, did you mean `--{flag}`?"),
        None => format!("{base}."),
    }
}

/// The most similar usable flag, skipping aliases and SVT-AV1 flags rav1oli sets itself.
fn closest_flag(name: &str, rav1oli_flags: &[String]) -> Option<String> {
    let svt_names = param_names()
        .iter()
        .filter(|flag| !is_encoder_owned(flag))
        .map(|flag| flag.to_string());
    let candidates = rav1oli_flags.iter().cloned().chain(svt_names);

    let mut scored: Vec<(String, f64)> = candidates
        .filter(|flag| flag.len() >= 2)
        .map(|flag| {
            let score = strsim::normalized_levenshtein(name, &flag);
            (flag, score)
        })
        .filter(|(_, score)| *score >= SUGGESTION_THRESHOLD)
        .collect();
    scored.sort_by(|(_, left), (_, right)| right.total_cmp(left));

    scored.into_iter().map(|(flag, _)| flag).next()
}
