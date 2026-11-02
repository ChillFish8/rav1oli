use anyhow::{Context, bail};
use indicatif::ProgressBar;
use rav1oli_bestsource::Frame;
use rav1oli_table::{MapSummary, MapWriter};

use crate::decode::Decoder;
use crate::interrupt;
use crate::metrics::{MetricConfig, MetricScorer};
use crate::progress::{finish, frame_bar};

/// Checks the encode has exactly one frame for each source frame.
pub fn check_frame_counts(source: u32, encoded: i64) -> anyhow::Result<()> {
    if encoded != i64::from(source) {
        bail!("the first pass has {encoded} frames but the source has {source}");
    }

    Ok(())
}

/// Scores every frame pair and writes the block scores to `writer`.
///
/// The source is the reference and the encode is the distorted video.
pub fn score_frames(
    source: &mut Decoder,
    encoded: &mut Decoder,
    config: MetricConfig,
    gpu: i32,
    writer: &mut MapWriter,
) -> anyhow::Result<MapSummary> {
    let source_count = source.properties().num_frames;
    let encoded_count = encoded.properties().num_frames;
    let frame_count = u32::try_from(source_count).context("the source has an invalid frame count")?;
    check_frame_counts(frame_count, encoded_count)?;

    let bar = frame_bar("scoring", u64::from(frame_count));
    let result = score_pairs(source, encoded, config, gpu, writer, &bar);
    let verb = if result.is_ok() { "scored" } else { "stopped after" };
    finish(&bar, verb);

    result
}

fn score_pairs(
    source: &mut Decoder,
    encoded: &mut Decoder,
    config: MetricConfig,
    gpu: i32,
    writer: &mut MapWriter,
    bar: &ProgressBar,
) -> anyhow::Result<MapSummary> {
    interrupt::check()?;

    let mut pairs = source.frames().zip(encoded.frames()).enumerate();
    let Some((_, (first_reference, first_distorted))) = pairs.next() else {
        bail!("the source has no frames to score");
    };
    let (first_reference, first_distorted) = decode_pair(0, first_reference, first_distorted)?;

    let mut scorer = MetricScorer::new(config, gpu, &first_reference, &first_distorted)?;
    write_pair(&mut scorer, writer, 0, &first_reference, &first_distorted)?;
    bar.inc(1);

    for (index, (reference, distorted)) in pairs {
        interrupt::check()?;

        let (reference, distorted) = decode_pair(index, reference, distorted)?;

        write_pair(&mut scorer, writer, index, &reference, &distorted)?;
        bar.inc(1);
    }

    scorer.finish()
}

fn decode_pair(
    index: usize,
    reference: anyhow::Result<Frame>,
    distorted: anyhow::Result<Frame>,
) -> anyhow::Result<(Frame, Frame)> {
    let reference = reference.with_context(|| format!("could not decode source frame {index}"))?;
    let distorted = distorted.with_context(|| format!("could not decode first-pass frame {index}"))?;

    Ok((reference, distorted))
}

fn write_pair(
    scorer: &mut MetricScorer,
    writer: &mut MapWriter,
    index: usize,
    reference: &Frame,
    distorted: &Frame,
) -> anyhow::Result<()> {
    let scored = scorer
        .score(reference, distorted)
        .with_context(|| format!("could not score frame {index}"))?;

    writer
        .write_frame(&scored.block_scores, scored.frame_scores)
        .with_context(|| format!("could not write frame {index} to the map"))
}
