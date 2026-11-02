mod analyse;
mod options;
mod passthrough;

use clap::Parser;

use super::Command;
use super::analyse::AnalyseArgs;

#[derive(Debug, Parser)]
struct TestCli {
    #[command(subcommand)]
    command: Command,
}

/// Parses `arguments` as an `analyse` command line.
pub(crate) fn parse_analyse_args(arguments: &[&str]) -> AnalyseArgs {
    let cli = TestCli::try_parse_from(arguments).expect("arguments should parse");

    match cli.command {
        Command::Analyse(args) => args,
        _ => panic!("expected the analyse command"),
    }
}
