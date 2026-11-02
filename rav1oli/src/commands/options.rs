use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Args, ValueEnum};
use rav1oli_table::{Metric as TableMetric, Quantisation};

use super::passthrough::{long_flags, validate_svt_args};
use crate::decode::CacheLocation;
use crate::metrics::{BUTTERAUGLI_QUANTISATION, CVVDP_QUANTISATION};

/// The error for an invalid `--tables` file.
pub fn tables_error(error: &anyhow::Error) -> clap::Error {
    let message = format!("invalid value for '--tables': {error:#}");
    clap::Error::raw(ErrorKind::ValueValidation, message)
}

/// The display brightness in nits Butteraugli assumes when `--intensity-target` is not set.
pub const DEFAULT_INTENSITY_TARGET: f32 = 203.0;

/// The metric used to measure distortion in the first pass.
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, ValueEnum)]
pub enum Metric {
    #[default]
    Cvvdp,
    Butteraugli,
}

impl From<Metric> for TableMetric {
    fn from(metric: Metric) -> Self {
        match metric {
            Metric::Cvvdp => TableMetric::Cvvdp,
            Metric::Butteraugli => TableMetric::Butteraugli,
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct AnalysisOptions {
    /// The metric used to build the distortion map.
    #[arg(long, value_enum, default_value_t = Metric::default())]
    pub metric: Metric,
    /// Disable the temporal component of CVVDP.
    #[arg(long)]
    pub no_temporal: bool,
    /// The GPU Vship scores on.
    #[arg(long, default_value_t = 0)]
    pub gpu: i32,
    /// The display CVVDP models viewing on, picked from the video's resolution if not set.
    #[arg(long)]
    pub display_model: Option<String>,
    /// A JSON file of extra display models for --display-model.
    #[arg(long)]
    pub display_config: Option<PathBuf>,
    /// The display brightness in nits Butteraugli assumes, 203 if not set.
    #[arg(long)]
    pub intensity_target: Option<f32>,
    /// Distortion below this value scores zero in the map, defaulting per metric.
    #[arg(long)]
    pub floor: Option<f32>,
}

impl AnalysisOptions {
    /// Checks the options are valid for the selected metric.
    pub fn validate(&self) -> Result<(), clap::Error> {
        if self.no_temporal && self.metric != Metric::Cvvdp {
            let message = "the argument '--no-temporal' can only be used with '--metric cvvdp'";
            let error = clap::Error::raw(ErrorKind::ArgumentConflict, message);
            return Err(error);
        }

        let has_display_flag = self.display_model.is_some() || self.display_config.is_some();
        if has_display_flag && self.metric != Metric::Cvvdp {
            let message = "the arguments '--display-model' and '--display-config' can only be \
                used with '--metric cvvdp'";
            let error = clap::Error::raw(ErrorKind::ArgumentConflict, message);
            return Err(error);
        }

        if self.intensity_target.is_some() && self.metric != Metric::Butteraugli {
            let message = "the argument '--intensity-target' can only be used with \
                '--metric butteraugli'";
            let error = clap::Error::raw(ErrorKind::ArgumentConflict, message);
            return Err(error);
        }

        let invalid_intensity = self
            .intensity_target
            .filter(|target| !target.is_finite() || *target <= 0.0);
        if let Some(target) = invalid_intensity {
            let message =
                format!("invalid value for '--intensity-target': {target} must be finite and above 0");
            let error = clap::Error::raw(ErrorKind::ValueValidation, message);
            return Err(error);
        }

        if self.display_config.is_some() && self.display_model.is_none() {
            let message = "the argument '--display-config' requires '--display-model'";
            let error = clap::Error::raw(ErrorKind::ArgumentConflict, message);
            return Err(error);
        }

        let quantisation = self.quantisation();
        if quantisation.floor >= quantisation.cap {
            let (floor, cap) = (quantisation.floor, quantisation.cap);
            let message = format!("invalid value for '--floor': {floor} must be below the cap of {cap}");
            let error = clap::Error::raw(ErrorKind::ValueValidation, message);
            return Err(error);
        }

        if let Err(invalid) = quantisation.check() {
            let message = format!("invalid value for '--floor': {}", invalid.0);
            let error = clap::Error::raw(ErrorKind::ValueValidation, message);
            return Err(error);
        }

        Ok(())
    }

    /// The Butteraugli intensity target, or the default if `--intensity-target` is not set.
    pub fn intensity_target(&self) -> f32 {
        self.intensity_target.unwrap_or(DEFAULT_INTENSITY_TARGET)
    }

    /// The quantisation for the selected metric, with `--floor` replacing its default floor.
    pub fn quantisation(&self) -> Quantisation {
        let default = match self.metric {
            Metric::Cvvdp => CVVDP_QUANTISATION,
            Metric::Butteraugli => BUTTERAUGLI_QUANTISATION,
        };

        match self.floor {
            Some(floor) => Quantisation { floor, ..default },
            None => default,
        }
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
    /// Unrecognised `--flags`, filled in from the command line after parsing.
    #[arg(skip)]
    pub svt_args: Vec<String>,
}

impl EncoderOptions {
    /// Checks the SVT-AV1 flags, with `subcommand` supplying the flags to suggest from.
    pub fn validate(&self, subcommand: &clap::Command) -> Result<(), clap::Error> {
        let rav1oli_flags = long_flags(subcommand);

        validate_svt_args(&self.svt_args, &rav1oli_flags)
    }

    /// The SVT-AV1 flags joined into one string, or `None` if there are none.
    pub fn svt_params_string(&self) -> Option<String> {
        if self.svt_args.is_empty() {
            return None;
        }

        Some(self.svt_args.join(" "))
    }
}

#[derive(Debug, Clone, Args)]
pub struct CacheOptions {
    /// Where to keep indexes and locks, instead of a .rav1oli folder next to each video.
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
}

impl CacheOptions {
    pub fn location(&self) -> CacheLocation {
        match &self.cache_dir {
            Some(path) => CacheLocation::Directory(path.clone()),
            None => CacheLocation::BesideVideo,
        }
    }
}
