use anyhow::{Context, bail};
use rav1oli_bestsource::{ColorFamily, VideoFormat, VideoProperties};
use rav1oli_svt::{ChromaPosition, ColourDescription, CpuCap, EncoderSettings};
use rav1oli_table::FrameRate;

use crate::commands::options::EncoderOptions;
use crate::metrics::FrameDescription;

const RANGE_FULL: i32 = 2;

const CHROMA_LOCATION_UNSPECIFIED: i32 = 0;
const CHROMA_LOCATION_LEFT: i32 = 1;
const CHROMA_LOCATION_CENTER: i32 = 2;
const CHROMA_LOCATION_TOP_LEFT: i32 = 3;
const CHROMA_LOCATION_TOP: i32 = 4;
const CHROMA_LOCATION_BOTTOM_LEFT: i32 = 5;
const CHROMA_LOCATION_BOTTOM: i32 = 6;

/// The checked properties of a source video, at the size the encoder is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceVideo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub frame_count: u32,
    pub frame_rate: FrameRate,
}

/// Checks the source is 8-bit or 10-bit YUV 4:2:0 and reads its size, frame count and frame rate.
///
/// The size is rounded down to whole chroma samples, so odd sized sources lose their last row or column.
pub fn describe_source(properties: &VideoProperties) -> anyhow::Result<SourceVideo> {
    check_format(&properties.format)?;

    let width = u32::try_from(properties.ss_mod_width)
        .with_context(|| format!("the source width {} is invalid", properties.ss_mod_width))?;
    let height = u32::try_from(properties.ss_mod_height)
        .with_context(|| format!("the source height {} is invalid", properties.ss_mod_height))?;
    if width == 0 || height == 0 {
        bail!("the source size {width}x{height} is invalid");
    }

    let bit_depth = u8::try_from(properties.format.bits)
        .with_context(|| format!("the source bit depth {} is invalid", properties.format.bits))?;

    let frame_count = u32::try_from(properties.num_frames)
        .with_context(|| format!("the source frame count {} is invalid", properties.num_frames))?;
    if frame_count == 0 {
        bail!("the source has no frames");
    }

    let (numerator, denominator) = properties.fps;
    let numerator = u32::try_from(numerator)
        .ok()
        .filter(|numerator| *numerator > 0)
        .with_context(|| format!("the source frame rate numerator {numerator} is invalid"))?;
    let denominator = u32::try_from(denominator)
        .ok()
        .filter(|denominator| *denominator > 0)
        .with_context(|| format!("the source frame rate denominator {denominator} is invalid"))?;

    Ok(SourceVideo {
        width,
        height,
        bit_depth,
        frame_count,
        frame_rate: FrameRate {
            numerator,
            denominator,
        },
    })
}

fn check_format(format: &VideoFormat) -> anyhow::Result<()> {
    let bits = format.bits;
    let family = match format.color_family {
        ColorFamily::Yuv => "YUV",
        ColorFamily::Rgb => "RGB",
        ColorFamily::Gray => "gray",
        ColorFamily::Bayer => "Bayer",
        ColorFamily::Unknown => "unknown",
    };
    let subsampling = match (format.sub_sampling_w, format.sub_sampling_h) {
        (0, 0) => "4:4:4".to_owned(),
        (1, 0) => "4:2:2".to_owned(),
        (1, 1) => "4:2:0".to_owned(),
        (width, height) => format!("with subsampling shifts {width} and {height}"),
    };
    let found = format!("the source is {bits}-bit {family} {subsampling}");
    let supported = "only 8-bit or 10-bit YUV 4:2:0 is supported";

    if format.color_family != ColorFamily::Yuv {
        bail!("{found}, {supported}");
    }

    if format.float {
        bail!("{found} with floating point samples, {supported}");
    }

    if format.alpha {
        bail!("{found} with an alpha plane, {supported}");
    }

    let is_420 = (format.sub_sampling_w, format.sub_sampling_h) == (1, 1);
    if !is_420 || !matches!(bits, 8 | 10) {
        bail!("{found}, {supported}");
    }

    Ok(())
}

/// Maps the colour of a decoded frame to what the encoder signals in the stream.
///
/// Unspecified range is limited and unspecified chroma location is left, as is usual for YUV video.
pub fn colour_description(frame: &FrameDescription) -> anyhow::Result<ColourDescription> {
    let matrix = colour_code("matrix", frame.matrix)?;
    let transfer = colour_code("transfer", frame.transfer)?;
    let primaries = colour_code("primaries", frame.primaries)?;
    let chroma_position = chroma_position(frame.chroma_location)?;

    Ok(ColourDescription {
        matrix,
        transfer,
        primaries,
        full_range: frame.range == RANGE_FULL,
        chroma_position,
    })
}

fn colour_code(name: &str, value: i32) -> anyhow::Result<u8> {
    u8::try_from(value).with_context(|| format!("the {name} code {value} does not fit in a byte"))
}

fn chroma_position(location: i32) -> anyhow::Result<ChromaPosition> {
    match location {
        CHROMA_LOCATION_UNSPECIFIED | CHROMA_LOCATION_LEFT => Ok(ChromaPosition::Left),
        CHROMA_LOCATION_CENTER => Ok(ChromaPosition::Center),
        CHROMA_LOCATION_TOP_LEFT => Ok(ChromaPosition::TopLeft),
        CHROMA_LOCATION_TOP => Ok(ChromaPosition::Top),
        CHROMA_LOCATION_BOTTOM_LEFT => bail!("chroma location bottom left can not be signalled in AV1"),
        CHROMA_LOCATION_BOTTOM => bail!("chroma location bottom can not be signalled in AV1"),
        other => bail!("the chroma location {other} is not known"),
    }
}

/// The settings for encoding `source`.
pub fn encoder_settings(
    source: &SourceVideo,
    colour: ColourDescription,
    options: &EncoderOptions,
) -> EncoderSettings {
    EncoderSettings {
        width: source.width,
        height: source.height,
        bit_depth: source.bit_depth,
        colour,
        fps_numerator: source.frame_rate.numerator,
        fps_denominator: source.frame_rate.denominator,
        preset: options.preset,
        crf: options.crf,
        svt_args: options.svt_args.clone(),
        cpu_cap: CpuCap::Default,
        map_mode: None,
    }
}
