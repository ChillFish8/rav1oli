mod first_pass;
mod output;
mod scoring;
mod source;
#[cfg(test)]
mod tests;

use anyhow::Context;
use rav1oli_table::{MapSummary, MapWriter, hash_source};

#[cfg(test)]
pub use self::first_pass::EncoderPlanes;
use self::first_pass::encode_first_pass;
pub use self::output::sibling_directory;
use self::output::{persist, temporary_sibling};
#[cfg(test)]
pub use self::scoring::check_frame_counts;
use self::scoring::score_frames;
#[cfg(test)]
pub use self::source::SourceVideo;
use self::source::{colour_description, describe_source, encoder_settings, map_metadata, metric_config};
use crate::commands::analyse::AnalyseArgs;
use crate::decode::Decoder;
use crate::metrics::FrameDescription;

/// Runs the first-pass encode of the source and scores it into a distortion map.
pub fn analyse(args: &AnalyseArgs) -> anyhow::Result<MapSummary> {
    let gpu = args.analysis.gpu;
    tracing::info!(gpu, "checking the GPU");
    rav1oli_vship::check_device(gpu).with_context(|| format!("failed to check GPU {gpu}"))?;
    tracing::info!(gpu, "checked the GPU");

    // Created before any long running work so an unwritable map directory fails early. Both temporary
    // files are held until the end so they are deleted whether the run succeeds or fails.
    let temporary_map = temporary_sibling(&args.map, ".tmp")?;
    let (encode_path, _temporary_encode) = match &args.keep_encode {
        Some(path) => (path.clone(), None),
        None => {
            let temporary = temporary_sibling(&args.map, ".ivf")?;
            (temporary.to_path_buf(), Some(temporary))
        },
    };

    tracing::info!(input = %args.input.display(), "opening the source");
    let cache = args.cache.location();
    let mut source_decoder = Decoder::open(&args.input, &cache)?;
    let source = describe_source(source_decoder.properties())?;
    let first_frame = source_decoder
        .frames()
        .next()
        .context("the source has no frames")??;
    let description = FrameDescription::from_frame(&first_frame);
    let colour = colour_description(&description)?;
    drop(first_frame);
    tracing::info!(
        width = source.width,
        height = source.height,
        frames = source.frame_count,
        "opened the source"
    );

    tracing::info!("building the settings");
    let config = metric_config(&args.analysis, &source)?;
    let settings = encoder_settings(&source, colour, &args.encoder);
    let hash = hash_source(&args.input).context("could not hash the source")?;
    let metadata = map_metadata(&source, hash, &args.encoder, &config);
    tracing::info!(hash = %metadata.source.hash, "built the settings");

    tracing::info!(output = %encode_path.display(), "starting the first-pass encode");
    encode_first_pass(&mut source_decoder, &source, &settings, &encode_path)?;
    tracing::info!("finished the first-pass encode");

    tracing::info!("starting scoring");
    let mut encoded_decoder = match &args.keep_encode {
        Some(path) => Decoder::open(path, &cache)?,
        None => Decoder::open_uncached(&encode_path)?,
    };

    let mut writer = MapWriter::create(&temporary_map, &metadata).context("could not create the map")?;
    let summary = score_frames(
        &mut source_decoder,
        &mut encoded_decoder,
        config,
        gpu,
        &mut writer,
    )?;
    log_summary(&summary);

    tracing::info!(map = %args.map.display(), "writing the distortion map");
    writer.finish(&summary).context("could not finish the map")?;
    persist(temporary_map, &args.map)?;
    tracing::info!(map = %args.map.display(), "wrote the distortion map");

    Ok(summary)
}

fn log_summary(summary: &MapSummary) {
    match summary {
        MapSummary::CvvdpTemporal { score } => tracing::info!(score, "scored with CVVDP"),
        MapSummary::CvvdpPerFrame(statistics) => {
            tracing::info!(average = statistics.average, "scored with CVVDP per frame");
        },
        MapSummary::Butteraugli { norm_q, .. } => {
            tracing::info!(average = norm_q.average, "scored with Butteraugli norm q");
        },
    }
}
