use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rav1oli_quantise::Quantisation;
use rav1oli_svt::{MAX_CUT, MAX_DROP, MAX_OFFSET, MIN_OFFSET, MapMode};
use rav1oli_table::Metric;
use serde::Deserialize;

use super::defaults::default_json;

const SUPPORTED_VERSION: i64 = 1;

type StoredPoints = Vec<(f64, f64)>;

/// A piecewise linear curve from a distortion to a whole number value.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    points: Vec<(f64, i16)>,
}

/// The grid values for every map score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookups {
    Additive { drops: [u8; 256], cuts: [u8; 256] },
    Redistribute { offsets: [i8; 256], weights: [i8; 256] },
}

/// A metric's curves for one map mode.
///
/// The superblock curve gives a drop in additive mode and an offset in redistribute mode. The block
/// curve gives a cut in additive mode and a weight in redistribute mode.
#[derive(Debug, Clone, PartialEq)]
pub struct Tables {
    pub metric: Metric,
    pub mode: MapMode,
    pub superblock: Curve,
    pub block: Curve,
}

/// A curve's name in the table file and the values it may hold.
#[derive(Debug, Copy, Clone)]
struct CurveSpec {
    name: &'static str,
    smallest: i16,
    largest: i16,
}

impl CurveSpec {
    /// The superblock and block curves of `mode`.
    fn for_mode(mode: MapMode) -> [CurveSpec; 2] {
        match mode {
            MapMode::Additive => [
                CurveSpec {
                    name: "drop",
                    smallest: 0,
                    largest: i16::from(MAX_DROP),
                },
                CurveSpec {
                    name: "cut",
                    smallest: 0,
                    largest: i16::from(MAX_CUT),
                },
            ],
            MapMode::Redistribute => [
                CurveSpec {
                    name: "offset",
                    smallest: i16::from(MIN_OFFSET),
                    largest: i16::from(MAX_OFFSET),
                },
                CurveSpec {
                    name: "weight",
                    smallest: i16::from(i8::MIN),
                    largest: i16::from(i8::MAX),
                },
            ],
        }
    }

    fn build(self, stored: &[(f64, f64)]) -> Result<Curve> {
        Curve::new(self.name, stored, self.smallest, self.largest)
    }
}

#[derive(Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "lowercase")]
enum StoredMode {
    Additive,
    Redistribute,
}

impl From<StoredMode> for MapMode {
    fn from(stored: StoredMode) -> MapMode {
        match stored {
            StoredMode::Additive => MapMode::Additive,
            StoredMode::Redistribute => MapMode::Redistribute,
        }
    }
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
    mode: Option<StoredMode>,
    drop: Option<StoredPoints>,
    cut: Option<StoredPoints>,
    offset: Option<StoredPoints>,
    weight: Option<StoredPoints>,
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

    /// The mode the table is for, redistribute when it names none.
    fn mode(&self) -> MapMode {
        let stored = self.mode.unwrap_or(StoredMode::Redistribute);
        stored.into()
    }

    /// The superblock and block curves of `mode`, erroring on a curve of the other mode.
    fn curves(&self, mode: MapMode) -> Result<[Option<&StoredPoints>; 2]> {
        let (own, other) = match mode {
            MapMode::Additive => (
                [self.drop.as_ref(), self.cut.as_ref()],
                [
                    ("offset", self.offset.is_some()),
                    ("weight", self.weight.is_some()),
                ],
            ),
            MapMode::Redistribute => (
                [self.offset.as_ref(), self.weight.as_ref()],
                [("drop", self.drop.is_some()), ("cut", self.cut.is_some())],
            ),
        };

        let [superblock_spec, block_spec] = CurveSpec::for_mode(mode);
        for (name, is_present) in other {
            if !is_present {
                continue;
            }

            let hint = match mode {
                MapMode::Additive => "remove `\"mode\": \"additive\"` to use it",
                MapMode::Redistribute => "add `\"mode\": \"additive\"` to use it",
            };
            bail!(
                "a {mode} table takes {} and {} curves, but this one has a {name} curve, {hint}",
                superblock_spec.name,
                block_spec.name,
            );
        }

        Ok(own)
    }
}

impl Curve {
    /// Checks the points and builds a curve, with values between `smallest` and `largest`.
    pub(super) fn new(name: &str, stored: &[(f64, f64)], smallest: i16, largest: i16) -> Result<Curve> {
        if stored.is_empty() {
            bail!("the {name} curve needs at least one point");
        }

        let mut points: Vec<(f64, i16)> = Vec::with_capacity(stored.len());
        for (index, &(distortion, value)) in stored.iter().enumerate() {
            let prefix = format!("the {name} curve's point {index} ([{distortion:?}, {value}])");
            if !distortion.is_finite() || distortion < 0.0 {
                bail!("{prefix} needs a finite distortion of at least 0");
            }
            if !value.is_finite() || value.fract() != 0.0 {
                bail!("{prefix} needs a whole number value");
            }
            if value < f64::from(smallest) {
                bail!("{prefix} is below the smallest {name} of {smallest}");
            }
            if value > f64::from(largest) {
                bail!("{prefix} is above the largest {name} of {largest}");
            }
            if let Some(&(previous, _)) = points.last()
                && distortion <= previous
            {
                bail!("{prefix} needs a larger distortion than the point before it ({previous:?})");
            }

            points.push((distortion, value as i16));
        }

        Ok(Curve { points })
    }

    /// The value at `distortion`, held at the first and last points beyond their range.
    pub fn evaluate(&self, distortion: f32) -> i16 {
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
        value.round() as i16
    }

    #[cfg(test)]
    pub fn points(&self) -> &[(f64, i16)] {
        &self.points
    }
}

impl Tables {
    /// Parses a table file, filling a missing curve with the default for its metric and mode.
    pub fn parse(json: &str) -> Result<Tables> {
        let stored = StoredTable::read(json)?;
        let mode = stored.mode();
        let [superblock_points, block_points] = stored.curves(mode)?;
        let defaults = Tables::default_for(stored.metric, mode);
        let [superblock_spec, block_spec] = CurveSpec::for_mode(mode);

        let superblock = match superblock_points {
            Some(points) => superblock_spec.build(points)?,
            None => defaults.superblock,
        };
        let block = match block_points {
            Some(points) => block_spec.build(points)?,
            None => defaults.block,
        };

        Ok(Tables {
            metric: stored.metric,
            mode,
            superblock,
            block,
        })
    }

    pub fn load(path: &Path) -> Result<Tables> {
        let display = path.display();
        let json = fs::read_to_string(path).with_context(|| format!("failed to read {display}"))?;
        Tables::parse(&json).with_context(|| format!("the table {display} is invalid"))
    }

    pub fn default_for(metric: Metric, mode: MapMode) -> Tables {
        let json = default_json(metric, mode);
        let stored = StoredTable::read(json).expect("the built-in table is valid");
        let curves = stored.curves(mode).expect("the built-in table is for its mode");
        let [superblock_points, block_points] =
            curves.map(|points| points.expect("the built-in table has both curves"));
        let [superblock_spec, block_spec] = CurveSpec::for_mode(mode);
        let superblock = superblock_spec
            .build(superblock_points)
            .expect("the built-in superblock curve is valid");
        let block = block_spec
            .build(block_points)
            .expect("the built-in block curve is valid");

        Tables {
            metric: stored.metric,
            mode,
            superblock,
            block,
        }
    }

    /// Errors naming both metrics when `metric` differs from the table's.
    ///
    /// `owner` names what the metric came from, such as "the map".
    pub fn check_metric(&self, metric: Metric, owner: &str) -> Result<()> {
        if self.metric == metric {
            return Ok(());
        }

        let table_metric = self.metric;
        bail!("the table is for {table_metric} but {owner} uses {metric}");
    }

    /// Evaluates both curves at every map score.
    pub fn lookups(&self, quantisation: &Quantisation) -> Lookups {
        let mut superblock = [0i16; 256];
        let mut block = [0i16; 256];

        for score in 0..=u8::MAX {
            let distortion = quantisation.dequantise(score);
            superblock[usize::from(score)] = self.superblock.evaluate(distortion);
            block[usize::from(score)] = self.block.evaluate(distortion);
        }

        match self.mode {
            MapMode::Additive => Lookups::Additive {
                drops: superblock.map(|value| u8::try_from(value).expect("a drop is within 0 to 80")),
                cuts: block.map(|value| u8::try_from(value).expect("a cut is within 0 to 230")),
            },
            MapMode::Redistribute => Lookups::Redistribute {
                offsets: superblock.map(|value| i8::try_from(value).expect("an offset is within -80 to 40")),
                weights: block.map(|value| i8::try_from(value).expect("a weight fits an i8")),
            },
        }
    }
}
