use std::path::PathBuf;

use clap::Parser;

use crate::commands::Command;
use crate::decode::CacheLocation;

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
