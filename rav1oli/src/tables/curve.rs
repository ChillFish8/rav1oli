use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rav1oli_quantise::Quantisation;
use rav1oli_svt::{MAX_CUT, MAX_DROP};
use rav1oli_table::Metric;
use serde::Deserialize;

use super::defaults::default_json;

const SUPPORTED_VERSION: i64 = 1;

/// A piecewise linear curve from a distortion to a whole number value.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    points: Vec<(f64, u8)>,
}

/// The drop and cut values for every map score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookups {
    pub drops: [u8; 256],
    pub cuts: [u8; 256],
}

/// A metric's drop and cut curves.
#[derive(Debug, Clone, PartialEq)]
pub struct Tables {
    pub metric: Metric,
    pub drop: Curve,
    pub cut: Curve,
}

#[derive(Deserialize)]
struct StoredVersion {
    version: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTable {
    /// Read so a table without a version is rejected, the value is checked beforehand.
    #[serde(rename = "version")]
    _version: i64,
    metric: Metric,
    drop: Option<Vec<(f64, f64)>>,
    cut: Option<Vec<(f64, f64)>>,
}

impl StoredTable {
    fn read(json: &str) -> Result<StoredTable> {
        let stored_version: StoredVersion =
            serde_json::from_str(json).context("the table needs a whole-number version")?;
        if stored_version.version != SUPPORTED_VERSION {
            bail!(
                "unsupported table version {}, only version {SUPPORTED_VERSION} is supported",
                stored_version.version,
            );
        }

        serde_json::from_str(json).context("the table is not valid")
    }
}

impl Curve {
    /// Checks the points and builds a curve, with values of at most `largest`.
    pub(super) fn new(name: &str, stored: &[(f64, f64)], largest: u8) -> Result<Curve> {
        if stored.is_empty() {
            bail!("the {name} curve needs at least one point");
        }

        let mut points: Vec<(f64, u8)> = Vec::with_capacity(stored.len());
        for (index, &(distortion, value)) in stored.iter().enumerate() {
            let prefix = format!("the {name} curve's point {index} ([{distortion:?}, {value}])");
            if !distortion.is_finite() || distortion < 0.0 {
                bail!("{prefix} needs a finite distortion of at least 0");
            }
            if !value.is_finite() || value.fract() != 0.0 {
                bail!("{prefix} needs a whole number value");
            }
            if value < 0.0 {
                bail!("{prefix} is below 0");
            }
            if value > f64::from(largest) {
                bail!("{prefix} is above the largest {name} of {largest}");
            }
            if let Some(&(previous, _)) = points.last()
                && distortion <= previous
            {
                bail!("{prefix} needs a larger distortion than the point before it ({previous:?})");
            }

            points.push((distortion, value as u8));
        }

        Ok(Curve { points })
    }

    /// The value at `distortion`, held at the first and last points beyond their range.
    pub fn evaluate(&self, distortion: f32) -> u8 {
        let distortion = f64::from(distortion);
        let (first_distortion, first_value) = self.points[0];
        if distortion <= first_distortion {
            return first_value;
        }

        let (last_distortion, last_value) = self.points[self.points.len() - 1];
        if distortion >= last_distortion {
            return last_value;
        }

        let after = self.points.partition_point(|point| point.0 <= distortion);
        let (low_distortion, low_value) = self.points[after - 1];
        let (high_distortion, high_value) = self.points[after];

        let fraction = (distortion - low_distortion) / (high_distortion - low_distortion);
        let low = f64::from(low_value);
        let high = f64::from(high_value);
        let value = low + (high - low) * fraction;
        value.round() as u8
    }

    #[cfg(test)]
    pub fn points(&self) -> &[(f64, u8)] {
        &self.points
    }
}

impl Tables {
    /// Parses a table file, filling a missing curve with the default for its metric.
    pub fn parse(json: &str) -> Result<Tables> {
        let stored = StoredTable::read(json)?;
        let defaults = Tables::default_for(stored.metric);

        let drop = match &stored.drop {
            Some(points) => Curve::new("drop", points, MAX_DROP)?,
            None => defaults.drop,
        };
        let cut = match &stored.cut {
            Some(points) => Curve::new("cut", points, MAX_CUT)?,
            None => defaults.cut,
        };

        Ok(Tables {
            metric: stored.metric,
            drop,
            cut,
        })
    }

    pub fn load(path: &Path) -> Result<Tables> {
        let display = path.display();
        let json = fs::read_to_string(path).with_context(|| format!("failed to read {display}"))?;
        Tables::parse(&json).with_context(|| format!("the table {display} is invalid"))
    }

    pub fn default_for(metric: Metric) -> Tables {
        let json = default_json(metric);
        let stored = StoredTable::read(json).expect("the built-in table is valid");
        let drop_points = stored.drop.expect("the built-in table has a drop curve");
        let cut_points = stored.cut.expect("the built-in table has a cut curve");
        let drop = Curve::new("drop", &drop_points, MAX_DROP).expect("the built-in drop curve is valid");
        let cut = Curve::new("cut", &cut_points, MAX_CUT).expect("the built-in cut curve is valid");

        Tables {
            metric: stored.metric,
            drop,
            cut,
        }
    }

    /// Errors naming both metrics when `metric` differs from the table's.
    pub fn check_metric(&self, metric: Metric) -> Result<()> {
        if self.metric == metric {
            return Ok(());
        }

        let table_metric = self.metric;
        bail!("the table is for {table_metric} but the map was made with {metric}");
    }

    /// Evaluates both curves at every map score.
    pub fn lookups(&self, quantisation: &Quantisation) -> Lookups {
        let mut drops = [0; 256];
        let mut cuts = [0; 256];

        for score in 0..=u8::MAX {
            let distortion = quantisation.dequantise(score);
            drops[usize::from(score)] = self.drop.evaluate(distortion);
            cuts[usize::from(score)] = self.cut.evaluate(distortion);
        }

        Lookups { drops, cuts }
    }
}
