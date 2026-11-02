use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, bail};
use indicatif::ProgressBar;
use rav1oli_svt::{DistortionGrids, Encoder, EncoderSettings};

use super::planes::EncoderPlanes;
use super::source::SourceVideo;
use crate::decode::Decoder;
use crate::interrupt;
use crate::progress::{finish, frame_bar};

/// Encodes every frame of `decoder` to `output`, asking `grids_for` for each frame's grids.
///
/// `pass` names the encode in errors, such as "first pass". Returns the frames written. A failed encode
/// removes the partial `output`.
pub fn encode_video<G>(
    decoder: &mut Decoder,
    source: &SourceVideo,
    settings: &EncoderSettings,
    output: &Path,
    pass: &str,
    grids_for: G,
) -> anyhow::Result<u64>
where
    G: FnMut(usize) -> anyhow::Result<Option<DistortionGrids>>,
{
    let encoder =
        Encoder::new(settings, output).with_context(|| format!("could not start the {pass} encode"))?;

    let result = run_encoder(decoder, source, encoder, pass, grids_for);
    if result.is_err() {
        remove_partial(output, pass);
    }

    result
}

fn run_encoder<G>(
    decoder: &mut Decoder,
    source: &SourceVideo,
    encoder: Encoder,
    pass: &str,
    grids_for: G,
) -> anyhow::Result<u64>
where
    G: FnMut(usize) -> anyhow::Result<Option<DistortionGrids>>,
{
    let expected = u64::from(source.frame_count);
    let bar = frame_bar("encoding", expected);

    let result = encode_frames(decoder, source, encoder, pass, &bar, grids_for);
    let verb = if result.is_ok() {
        "encoded"
    } else {
        "stopped after"
    };
    finish(&bar, verb);

    let written = result?;
    if written != expected {
        bail!("the {pass} has {written} frames but the source has {expected}");
    }

    Ok(written)
}

fn remove_partial(output: &Path, pass: &str) {
    let removed = fs::remove_file(output);
    match removed {
        Ok(()) => {},
        Err(error) if error.kind() == ErrorKind::NotFound => {},
        Err(error) => {
            tracing::warn!(output = %output.display(), "could not remove the partial {pass}: {error}");
        },
    }
}

fn encode_frames<G>(
    decoder: &mut Decoder,
    source: &SourceVideo,
    mut encoder: Encoder,
    pass: &str,
    bar: &ProgressBar,
    mut grids_for: G,
) -> anyhow::Result<u64>
where
    G: FnMut(usize) -> anyhow::Result<Option<DistortionGrids>>,
{
    let mut planes = EncoderPlanes::new(source);

    for (index, frame) in decoder.frames().enumerate() {
        interrupt::check()?;

        let frame = frame?;
        let grids = grids_for(index)?;
        let exported = planes.export(&frame)?;

        let context = || format!("could not encode frame {index} of the {pass}");
        encoder
            .send_frame(&exported, grids.as_ref())
            .with_context(context)?;
        bar.inc(1);
    }

    let summary = encoder
        .finish()
        .with_context(|| format!("could not finish the {pass} encode"))?;
    Ok(summary.frames)
}
