use clap::error::ErrorKind;
use clap::{Args, ValueEnum};

/// The metric used to measure distortion in the first pass.
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, ValueEnum)]
pub enum Metric {
    #[default]
    Cvvdp,
    Butteraugli,
}

#[derive(Debug, Clone, Args)]
pub struct AnalysisOptions {
    /// The metric used to build the distortion map.
    #[arg(long, value_enum, default_value_t = Metric::default())]
    pub metric: Metric,
    /// Disable the temporal component of CVVDP.
    #[arg(long)]
    pub no_temporal: bool,
}

impl AnalysisOptions {
    /// Checks the options are valid for the selected metric.
    pub fn validate(&self) -> Result<(), clap::Error> {
        if self.no_temporal && self.metric != Metric::Cvvdp {
            let message = "the argument '--no-temporal' can only be used with '--metric cvvdp'";
            let error = clap::Error::raw(ErrorKind::ArgumentConflict, message);
            return Err(error);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Args)]
pub struct EncoderOptions {
    /// The SVT-AV1 preset, lower is slower with better quality.
    #[arg(long, default_value_t = 4, allow_negative_numbers = true)]
    pub preset: i8,
    /// The SVT-AV1 constant rate factor, lower is higher quality.
    #[arg(long, default_value_t = 30)]
    pub crf: u8,
    /// Extra parameters passed directly to SVT-AV1.
    #[arg(long, allow_hyphen_values = true)]
    pub svt_params: Option<String>,
}
