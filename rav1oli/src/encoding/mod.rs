mod grids;
#[cfg(test)]
mod tests;

use std::path::Path;

use anyhow::Context;
use rav1oli_table::{MapMetadata, MapReader, hash_source};

pub use self::grids::{GridBuilder, check_frame_index};
use crate::analysis::{encoder_info, source_info};
use crate::commands::options::EncoderOptions;
use crate::decode::{CacheLocation, Decoder};
use crate::metrics::FrameDescription;
use crate::tables::Tables;
use crate::video::{
    SourceVideo,
    colour_description,
    describe_source,
    encode_video,
    encoder_settings,
    persist,
    temporary_sibling,
};

/// What the final encode reads and writes.
pub struct EncodeRequest<'a> {
    pub input: &'a Path,
    pub map: &'a Path,
    pub output: &'a Path,
    pub tables: Option<&'a Path>,
    pub encoder: &'a EncoderOptions,
    pub cache: CacheLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOutcome {
    pub frames: u64,
    pub bytes: u64,
}

/// Runs the final encode of the source, guided by the distortion map.
pub fn encode(request: &EncodeRequest<'_>) -> anyhow::Result<EncodeOutcome> {
    // Created before any long running work so an unwritable output directory fails early. It is held
    // until the end. It is persisted on success and deleted on failure.
    let temporary_output = temporary_sibling(request.output, ".ivf")?;

    tracing::info!(map = %request.map.display(), "opening the distortion map");
    let mut reader = MapReader::open(request.map)
        .with_context(|| format!("could not open the map {}", request.map.display()))?;
    let stored = reader.metadata().clone();
    let map_metric = stored.metric.metric;
    let tables = match request.tables {
        Some(path) => Tables::load(path)?,
        None => Tables::default_for(map_metric),
    };
    tables.check_metric(map_metric)?;
    let lookups = tables.lookups(&stored.quantisation);
    tracing::info!(metric = %map_metric, "opened the distortion map");

    tracing::info!(input = %request.input.display(), "opening the source");
    let mut decoder = Decoder::open(request.input, &request.cache)?;
    let source = describe_source(decoder.properties())?;
    let first_frame = decoder.frames().next().context("the source has no frames")??;
    let description = FrameDescription::from_frame(&first_frame);
    let colour = colour_description(&description)?;
    drop(first_frame);
    let hash = hash_source(request.input).context("could not hash the source")?;
    tracing::info!(
        width = source.width,
        height = source.height,
        frames = source.frame_count,
        "opened the source"
    );

    let current = MapMetadata {
        source: source_info(&source, hash),
        encoder: encoder_info(request.encoder),
        metric: stored.metric.clone(),
        quantisation: stored.quantisation,
    };
    let mismatches = stored
        .check_against(&current)
        .context("the map does not match the source")?;
    for mismatch in mismatches {
        tracing::warn!("{mismatch}");
    }

    let mut grid_builder = GridBuilder::new(lookups, &stored.source)?;
    let settings = encoder_settings(&source, colour, request.encoder);

    tracing::info!(output = %request.output.display(), "starting the final encode");
    let grids_for = |index: usize| {
        let map_frame = reader
            .next()
            .with_context(|| format!("the map has no frame for source frame {index}"))?
            .with_context(|| format!("could not read the map's frame {index}"))?;
        check_frame_index(map_frame.index(), index)?;
        let grids = grid_builder.build(map_frame.scores())?;

        Ok(Some(grids))
    };
    let frames = encode_video(
        &mut decoder,
        &source,
        &settings,
        &temporary_output,
        "final pass",
        grids_for,
    )?;
    tracing::info!("finished the final encode");

    persist(temporary_output, request.output)?;
    let bytes = std::fs::metadata(request.output)
        .with_context(|| format!("could not read the size of {}", request.output.display()))?
        .len();
    let kbps = average_kbps(bytes, frames, &source);
    tracing::info!(frames, bytes, kbps, "wrote the final encode");

    Ok(EncodeOutcome { frames, bytes })
}

fn average_kbps(bytes: u64, frames: u64, source: &SourceVideo) -> f64 {
    if frames == 0 {
        return 0.0;
    }

    let rate = source.frame_rate;
    let bits = bytes as f64 * 8.0 * f64::from(rate.numerator);
    let milliseconds = frames as f64 * f64::from(rate.denominator) * 1000.0;

    bits / milliseconds
}
