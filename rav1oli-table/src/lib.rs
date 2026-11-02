//! Storage for the per block distortion scores produced by the analyse pass
//!
//! A distortion map is a parquet file with one row per frame. Each row holds a `u8` score for every
//! 8x8 block of the frame, laid out row by row. The file footer records the source video and the
//! settings used to produce the map, so a later pass can check it is being used correctly.

mod error;
mod metadata;
mod reader;
mod schema;
mod writer;

#[cfg(test)]
mod tests;

pub use rav1oli_quantise::{BLOCK_SIZE, InvalidQuantisation, Quantisation};

pub use self::error::TableError;
pub use self::metadata::{
    DisplaySettings,
    EncoderSettings,
    FrameRate,
    FrameScores,
    MapMetadata,
    MapSummary,
    Metric,
    MetricSettings,
    SettingMismatch,
    SourceInfo,
    Statistics,
    hash_source,
};
pub use self::reader::{Frame, MapReader};
pub use self::writer::MapWriter;
