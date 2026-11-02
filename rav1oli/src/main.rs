mod analysis;
mod commands;
mod decode;
mod interrupt;
mod metrics;
mod progress;

use std::io::IsTerminal;

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};
use tracing_subscriber::EnvFilter;

use self::commands::Command;
use self::commands::passthrough::split_args;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Set the log level of the system.
    ///
    /// This is overridden by `RUST_LOG` if set. The same filter pattern
    /// as `RUST_LOG` is supported.
    #[arg(long, global = true, env = "IDOL_LOG_LEVEL")]
    log_level: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let mut cli = Args::command();
    let split = split_args(&cli, std::env::args_os().collect());
    let matches = match cli.try_get_matches_from_mut(split.clap_args) {
        Ok(matches) => matches,
        Err(error) => error.exit(),
    };
    let mut args = match Args::from_arg_matches(&matches) {
        Ok(args) => args,
        Err(error) => error.format(&mut cli).exit(),
    };

    args.command.set_svt_args(split.svt_args);

    let subcommand_name = matches.subcommand_name();
    let Some(subcommand) = subcommand_name.and_then(|name| cli.find_subcommand(name)) else {
        unreachable!("clap requires a subcommand and only knows the ones it parsed");
    };

    // Logging starts before validation, so SVT-AV1's reasons for rejecting a flag are shown.
    // Safety: Called from main thread
    unsafe { setup_logging(args.log_level.as_deref()) }

    if let Err(error) = args.command.validate(subcommand) {
        exit_with_error(&mut cli, &matches, error);
    }

    tracing::info!("initialising");

    args.command.execute()
}

/// Prints a validation error with the usage of the selected subcommand and exits.
fn exit_with_error(cli: &mut clap::Command, matches: &ArgMatches, error: clap::Error) -> ! {
    let Some(name) = matches.subcommand_name() else {
        error.format(cli).exit()
    };

    match cli.find_subcommand_mut(name) {
        Some(subcommand) => error.format(subcommand).exit(),
        None => error.format(cli).exit(),
    }
}

/// Initialises the logging for the system.
///
/// # Safety
/// Has the same safety requirements of [std::env::set_var].
unsafe fn setup_logging(args_log_level: Option<&str>) {
    if std::env::var("RUST_LOG").is_err() {
        let level = args_log_level.unwrap_or("info");

        unsafe { std::env::set_var("RUST_LOG", level) };
    }

    let filter = EnvFilter::from_default_env();
    let writer = progress::tracing_writer();
    let is_terminal = std::io::stderr().is_terminal();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(is_terminal)
        .with_writer(writer)
        .init();
}
