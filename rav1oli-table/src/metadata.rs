use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::TableError;

/// The width and height in pixels of the area each score covers.
pub const BLOCK_SIZE: u32 = 8;

pub(crate) const FORMAT_VERSION: u32 = 1;
pub(crate) const METADATA_KEY: &str = "rav1oli.map";

/// Describes how a distortion map was produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapMetadata {
    pub source: SourceInfo,
    pub encoder: EncoderSettings,
    pub metric: MetricSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// The hex encoded BLAKE3 hash of the source file.
    pub hash: String,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    pub frame_rate: FrameRate,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncoderSettings {
    pub preset: i8,
    pub crf: u8,
    pub svt_params: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricSettings {
    pub metric: Metric,
    pub temporal: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    Cvvdp,
    Butteraugli,
}

/// A setting which differs between the stored map and the current run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingMismatch {
    pub setting: &'static str,
    pub stored: String,
    pub current: String,
}

impl fmt::Display for SettingMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} was {} when the map was made but is now {}",
            self.setting, self.stored, self.current,
        )
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Metric::Cvvdp => f.write_str("cvvdp"),
            Metric::Butteraugli => f.write_str("butteraugli"),
        }
    }
}

/// The layout written to the file footer.
#[derive(Serialize, Deserialize)]
struct StoredMetadata {
    version: u32,
    #[serde(flatten)]
    metadata: MapMetadata,
}

#[derive(Deserialize)]
struct StoredVersion {
    version: u32,
}

impl MapMetadata {
    /// Checks the map can be used for the `current` run.
    ///
    /// A different source is an error. Differing encoder or metric settings are returned so the
    /// caller can warn about them.
    pub fn check_against(&self, current: &MapMetadata) -> Result<Vec<SettingMismatch>, TableError> {
        self.source.check_against(&current.source)?;

        let mut mismatches = Vec::new();
        let stored_params = describe_params(&self.encoder.svt_params);
        let current_params = describe_params(&current.encoder.svt_params);
        push_if_different(
            &mut mismatches,
            "preset",
            self.encoder.preset,
            current.encoder.preset,
        );
        push_if_different(&mut mismatches, "crf", self.encoder.crf, current.encoder.crf);
        push_if_different(&mut mismatches, "svt params", stored_params, current_params);
        push_if_different(
            &mut mismatches,
            "metric",
            self.metric.metric,
            current.metric.metric,
        );
        push_if_different(
            &mut mismatches,
            "temporal",
            self.metric.temporal,
            current.metric.temporal,
        );

        Ok(mismatches)
    }

    pub(crate) fn to_json(&self) -> Result<String, TableError> {
        let stored = StoredMetadata {
            version: FORMAT_VERSION,
            metadata: self.clone(),
        };
        let json = serde_json::to_string(&stored)?;
        Ok(json)
    }

    /// Decodes the footer metadata, checking the version before the rest of the layout.
    pub(crate) fn from_json(json: &str) -> Result<Self, TableError> {
        let stored_version: StoredVersion = serde_json::from_str(json)?;
        if stored_version.version != FORMAT_VERSION {
            return Err(TableError::UnsupportedVersion(stored_version.version));
        }

        let stored: StoredMetadata = serde_json::from_str(json)?;
        Ok(stored.metadata)
    }
}

impl SourceInfo {
    /// The number of blocks across each frame, counting a partial block at the edge.
    pub fn grid_width(&self) -> u32 {
        self.width.div_ceil(BLOCK_SIZE)
    }

    /// The number of blocks down each frame, counting a partial block at the edge.
    pub fn grid_height(&self) -> u32 {
        self.height.div_ceil(BLOCK_SIZE)
    }

    pub fn blocks_per_frame(&self) -> usize {
        let grid_width = self.grid_width() as usize;
        let grid_height = self.grid_height() as usize;
        grid_width * grid_height
    }

    fn check_against(&self, current: &SourceInfo) -> Result<(), TableError> {
        ensure_same("hash", &self.hash, &current.hash)?;
        ensure_same("width", self.width, current.width)?;
        ensure_same("height", self.height, current.height)?;
        ensure_same("frame count", self.frame_count, current.frame_count)?;
        ensure_same("frame rate", self.frame_rate, current.frame_rate)
    }
}

/// Hashes the file at `path` with BLAKE3, returning the hex encoded digest.
pub fn hash_source(path: &Path) -> Result<String, TableError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap(path)?;

    let digest = hasher.finalize();
    Ok(digest.to_hex().to_string())
}

fn describe_params(params: &Option<String>) -> String {
    match params {
        Some(params) => params.clone(),
        None => "none".to_string(),
    }
}

fn push_if_different<T>(mismatches: &mut Vec<SettingMismatch>, setting: &'static str, stored: T, current: T)
where
    T: PartialEq + ToString,
{
    if stored == current {
        return;
    }

    let mismatch = SettingMismatch {
        setting,
        stored: stored.to_string(),
        current: current.to_string(),
    };
    mismatches.push(mismatch);
}

fn ensure_same<T>(field: &'static str, stored: T, current: T) -> Result<(), TableError>
where
    T: PartialEq + ToString,
{
    if stored == current {
        return Ok(());
    }

    Err(TableError::SourceMismatch {
        field,
        stored: stored.to_string(),
        current: current.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::tests::sample_metadata;

    #[test]
    fn grid_size_counts_whole_blocks() {
        let source = sample_metadata().source;

        assert_eq!(source.grid_width(), 240);
        assert_eq!(source.grid_height(), 135);
        assert_eq!(source.blocks_per_frame(), 240 * 135);
    }

    #[test]
    fn grid_size_rounds_partial_blocks_up() {
        let mut source = sample_metadata().source;
        source.width = 1921;
        source.height = 1081;

        assert_eq!(source.grid_width(), 241);
        assert_eq!(source.grid_height(), 136);
    }

    #[test]
    fn check_against_identical_metadata_has_no_mismatches() {
        let stored = sample_metadata();

        let mismatches = stored.check_against(&stored).unwrap();

        assert!(mismatches.is_empty());
    }

    #[test]
    fn check_against_reports_changed_encoder_settings() {
        let stored = sample_metadata();
        let mut current = stored.clone();
        current.encoder.crf = 25;
        current.encoder.svt_params = Some("--tune 0".to_string());

        let mismatches = stored.check_against(&current).unwrap();

        let crf = SettingMismatch {
            setting: "crf",
            stored: "30".to_string(),
            current: "25".to_string(),
        };
        let svt_params = SettingMismatch {
            setting: "svt params",
            stored: "none".to_string(),
            current: "--tune 0".to_string(),
        };
        assert_eq!(mismatches, vec![crf, svt_params]);
    }

    #[test]
    fn check_against_reports_changed_metric_settings() {
        let stored = sample_metadata();
        let mut current = stored.clone();
        current.metric.metric = Metric::Butteraugli;
        current.metric.temporal = false;

        let mismatches = stored.check_against(&current).unwrap();

        let settings: Vec<&str> = mismatches.iter().map(|mismatch| mismatch.setting).collect();
        assert_eq!(settings, vec!["metric", "temporal"]);
    }

    #[test]
    fn check_against_rejects_different_hash() {
        let stored = sample_metadata();
        let mut current = stored.clone();
        current.source.hash = "def456".to_string();

        let result = stored.check_against(&current);

        let error = result.unwrap_err();
        assert!(matches!(error, TableError::SourceMismatch { field: "hash", .. }));
    }

    #[test]
    fn check_against_rejects_different_resolution() {
        let stored = sample_metadata();
        let mut current = stored.clone();
        current.source.height = 720;

        let result = stored.check_against(&current);

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            TableError::SourceMismatch { field: "height", .. }
        ));
    }

    #[test]
    fn json_round_trips() {
        let metadata = sample_metadata();

        let json = metadata.to_json().unwrap();
        let decoded = MapMetadata::from_json(&json).unwrap();

        assert_eq!(decoded, metadata);
    }

    #[test]
    fn from_json_rejects_other_versions() {
        let json = sample_metadata().to_json().unwrap();
        let json = json.replace("\"version\":1", "\"version\":99");

        let result = MapMetadata::from_json(&json);

        let error = result.unwrap_err();
        assert!(matches!(error, TableError::UnsupportedVersion(99)));
    }

    #[test]
    fn hash_source_matches_blake3_of_contents() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"some video bytes").unwrap();

        let hash = hash_source(file.path()).unwrap();

        let expected = blake3::hash(b"some video bytes");
        assert_eq!(hash, expected.to_hex().as_str());
    }
}
