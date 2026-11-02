pub mod analyse;
mod encode;
pub mod options;
pub mod passthrough;
mod run;
#[cfg(test)]
pub(crate) mod tests;

use clap::Subcommand;

use self::analyse::AnalyseArgs;
use self::encode::EncodeArgs;
use self::passthrough::SVT_FLAGS_HELP;
use self::run::RunArgs;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the first pass encode and produce a distortion map.
    #[command(after_help = SVT_FLAGS_HELP)]
    Analyse(AnalyseArgs),
    /// Run the second pass encode guided by a distortion map.
    #[command(after_help = SVT_FLAGS_HELP)]
    Encode(EncodeArgs),
    /// Run both passes end to end.
    #[command(after_help = SVT_FLAGS_HELP)]
    Run(RunArgs),
}

impl Command {
    /// Checks for invalid argument combinations clap cannot express.
    pub fn validate(&self, subcommand: &clap::Command) -> Result<(), clap::Error> {
        match self {
            Command::Analyse(args) => {
                args.validate_paths()?;
                args.analysis.validate()?;
                args.encoder.validate(subcommand)
            },
            Command::Encode(args) => {
                args.validate_paths()?;
                args.encoder.validate(subcommand)
            },
            Command::Run(args) => {
                args.validate_paths()?;
                args.analysis.validate()?;
                args.encoder.validate(subcommand)
            },
        }
    }

    /// Sets the SVT-AV1 flags split from the command line.
    pub fn set_svt_args(&mut self, svt_args: Vec<String>) {
        let encoder = match self {
            Command::Analyse(args) => &mut args.encoder,
            Command::Encode(args) => &mut args.encoder,
            Command::Run(args) => &mut args.encoder,
        };

        encoder.svt_args = svt_args;
    }

    pub fn execute(self) -> anyhow::Result<()> {
        match self {
            Command::Analyse(args) => analyse::execute(args),
            Command::Encode(args) => encode::execute(args),
            Command::Run(args) => run::execute(args),
        }
    }
}
