use std::io::ErrorKind;
use std::path::Path;
use std::{fs, slice};

use anyhow::{Context, bail};
use indicatif::ProgressBar;
use rav1oli_bestsource::{Frame, PlanarTarget, PlaneTarget};
use rav1oli_svt::{Encoder, EncoderSettings, Planes};

use super::source::SourceVideo;
use crate::decode::Decoder;
use crate::interrupt;
use crate::progress::{finish, frame_bar};

const TEN_BIT: u8 = 10;

#[derive(Debug)]
enum Buffers {
    Eight([Vec<u8>; 3]),
    Ten([Vec<u16>; 3]),
}

/// Reusable host planes one source frame is exported into for SVT-AV1.
#[derive(Debug)]
pub struct EncoderPlanes {
    buffers: Buffers,
    /// Samples from the start of one row to the next, for each plane.
    strides: [usize; 3],
}

impl EncoderPlanes {
    pub fn new(source: &SourceVideo) -> Self {
        let luma_width = source.width as usize;
        let luma_height = source.height as usize;
        let chroma_width = luma_width.div_ceil(2);
        let chroma_height = luma_height.div_ceil(2);

        let lengths = [
            luma_width * luma_height,
            chroma_width * chroma_height,
            chroma_width * chroma_height,
        ];
        let strides = [luma_width, chroma_width, chroma_width];

        let buffers = if source.bit_depth == TEN_BIT {
            Buffers::Ten(lengths.map(|length| vec![0; length]))
        } else {
            Buffers::Eight(lengths.map(|length| vec![0; length]))
        };

        EncoderPlanes { buffers, strides }
    }

    /// Copies `frame` into the planes and views them as SVT-AV1 input.
    pub fn export(&mut self, frame: &Frame) -> anyhow::Result<Planes<'_>> {
        let strides = self.strides;
        match &mut self.buffers {
            Buffers::Eight([y_plane, u_plane, v_plane]) => {
                let planes = [
                    y_plane.as_mut_slice(),
                    u_plane.as_mut_slice(),
                    v_plane.as_mut_slice(),
                ];
                export_planes(frame, planes, strides, 1)?;
            },
            Buffers::Ten([y_plane, u_plane, v_plane]) => {
                let planes = [as_bytes(y_plane), as_bytes(u_plane), as_bytes(v_plane)];
                export_planes(frame, planes, strides, 2)?;
            },
        }

        let planes = match &self.buffers {
            Buffers::Eight([y_plane, u_plane, v_plane]) => Planes::Eight {
                y: y_plane,
                u: u_plane,
                v: v_plane,
                strides,
            },
            Buffers::Ten([y_plane, u_plane, v_plane]) => Planes::Ten {
                y: y_plane,
                u: u_plane,
                v: v_plane,
                strides,
            },
        };
        Ok(planes)
    }

    #[cfg(test)]
    pub(super) fn lengths_and_strides(&self) -> ([usize; 3], [usize; 3]) {
        let lengths = match &self.buffers {
            Buffers::Eight([y_plane, u_plane, v_plane]) => [y_plane.len(), u_plane.len(), v_plane.len()],
            Buffers::Ten([y_plane, u_plane, v_plane]) => [y_plane.len(), u_plane.len(), v_plane.len()],
        };
        (lengths, self.strides)
    }

    #[cfg(test)]
    pub(super) fn is_ten_bit(&self) -> bool {
        matches!(self.buffers, Buffers::Ten(_))
    }
}

fn as_bytes(samples: &mut [u16]) -> &mut [u8] {
    let length = std::mem::size_of_val(samples);
    let pointer = samples.as_mut_ptr().cast::<u8>();
    // SAFETY: `u8` has an alignment of 1 and `length` covers exactly the bytes of `samples`, which
    // is borrowed mutably for as long as the returned slice lives.
    unsafe { slice::from_raw_parts_mut(pointer, length) }
}

fn export_planes(
    frame: &Frame,
    planes: [&mut [u8]; 3],
    strides: [usize; 3],
    bytes_per_sample: usize,
) -> anyhow::Result<()> {
    let mut targets = Vec::with_capacity(3);
    for (data, stride) in planes.into_iter().zip(strides) {
        let stride = stride * bytes_per_sample;
        targets.push(PlaneTarget { data, stride });
    }

    let mut target = PlanarTarget {
        planes: targets,
        alpha: None,
    };
    frame
        .export_planar(&mut target)
        .context("could not copy the frame out of the decoder")
}

/// Encodes every frame of `decoder` to `output`, returning the frames written.
///
/// A failed encode removes the partial `output`.
pub fn encode_first_pass(
    decoder: &mut Decoder,
    source: &SourceVideo,
    settings: &EncoderSettings,
    output: &Path,
) -> anyhow::Result<u64> {
    let encoder = Encoder::new(settings, output).context("could not start the first-pass encode")?;

    let result = run_encoder(decoder, source, encoder);
    if result.is_err() {
        remove_partial(output);
    }

    result
}

fn run_encoder(decoder: &mut Decoder, source: &SourceVideo, encoder: Encoder) -> anyhow::Result<u64> {
    let expected = u64::from(source.frame_count);
    let bar = frame_bar("encoding", expected);

    let result = encode_frames(decoder, source, encoder, &bar);
    let verb = if result.is_ok() {
        "encoded"
    } else {
        "stopped after"
    };
    finish(&bar, verb);

    let written = result?;
    if written != expected {
        bail!("the first pass has {written} frames but the source has {expected}");
    }

    Ok(written)
}

fn remove_partial(output: &Path) {
    let removed = fs::remove_file(output);
    match removed {
        Ok(()) => {},
        Err(error) if error.kind() == ErrorKind::NotFound => {},
        Err(error) => {
            tracing::warn!(output = %output.display(), "could not remove the partial first pass: {error}");
        },
    }
}

fn encode_frames(
    decoder: &mut Decoder,
    source: &SourceVideo,
    mut encoder: Encoder,
    bar: &ProgressBar,
) -> anyhow::Result<u64> {
    let mut planes = EncoderPlanes::new(source);

    for (index, frame) in decoder.frames().enumerate() {
        interrupt::check()?;

        let frame = frame?;
        let exported = planes.export(&frame)?;

        let context = || format!("could not encode frame {index} of the first pass");
        encoder.send_frame(&exported, None).with_context(context)?;
        bar.inc(1);
    }

    let summary = encoder
        .finish()
        .context("could not finish the first-pass encode")?;
    Ok(summary.frames)
}
