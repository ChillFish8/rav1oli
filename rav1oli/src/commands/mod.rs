mod analyse;
mod encode;
mod options;
mod run;

use clap::Subcommand;

use self::analyse::AnalyseArgs;
use self::encode::EncodeArgs;
use self::run::RunArgs;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the first pass encode and produce a distortion map.
    Analyse(AnalyseArgs),
    /// Run the second pass encode guided by a distortion map.
    Encode(EncodeArgs),
    /// Run both passes end to end.
    Run(RunArgs),
}

impl Command {
    /// Checks for invalid argument combinations clap cannot express.
    pub fn validate(&self) -> Result<(), clap::Error> {
        match self {
            Command::Analyse(args) => args.analysis.validate(),
            Command::Encode(_) => Ok(()),
            Command::Run(args) => args.analysis.validate(),
        }
    }

    pub fn execute(self) -> anyhow::Result<()> {
        match self {
            Command::Analyse(args) => analyse::execute(args),
            Command::Encode(args) => encode::execute(args),
            Command::Run(args) => run::execute(args),
        }
    }
}
