use anyhow::{Context, bail};
use rav1oli_bestsource::{ColorFamily, Frame, PlanarTarget, PlaneTarget};
use rav1oli_vship::{ChromaLocation, ColorSpace, Planes, Range, SampleType};

/// The tallest frame FFVship still treats as standard definition.
const STANDARD_DEFINITION_HEIGHT: i64 = 650;

const UNSPECIFIED: i32 = 2;

const SUPPORTED_MATRICES: [i32; 9] = [1, 5, 6, 8, 9, 10, 14, 16, 17];
const SUPPORTED_TRANSFERS: [i32; 10] = [1, 4, 5, 6, 7, 8, 13, 16, 17, 18];
const SUPPORTED_PRIMARIES: [i32; 7] = [1, 4, 5, 6, 7, 9, 12];

/// H.273 names of the unsupported matrices, for clearer errors.
const UNSUPPORTED_MATRIX_NAMES: [(i32, &str); 5] = [
    (4, "FCC"),
    (7, "SMPTE 240M"),
    (11, "SMPTE 2085"),
    (12, "chromaticity-derived non-constant"),
    (13, "chromaticity-derived constant"),
];

/// H.273 names of the unsupported transfers, for clearer errors.
const UNSUPPORTED_TRANSFER_NAMES: [(i32, &str); 5] = [
    (3, "reserved"),
    (9, "log 100:1"),
    (10, "log 316:1"),
    (11, "IEC 61966-2-4"),
    (12, "BT.1361"),
];

/// H.273 names of the unsupported primaries, for clearer errors.
const UNSUPPORTED_PRIMARIES_NAMES: [(i32, &str); 4] = [
    (8, "generic film"),
    (10, "CIE 1931 XYZ"),
    (11, "SMPTE 431"),
    (22, "EBU Tech 3213"),
];

const MATRIX_BT709: u32 = 1;
const MATRIX_BT470_BG: u32 = 5;
const MATRIX_BT2020_NCL: u32 = 9;
const MATRIX_BT2020_CL: u32 = 10;
const MATRIX_ICTCP: u32 = 14;

const TRANSFER_BT709: i32 = 1;
const TRANSFER_BT470_BG: i32 = 5;
const TRANSFER_SRGB: i32 = 13;
const TRANSFER_PQ: i32 = 16;
const TRANSFER_BT2020_10_BIT: i32 = 14;
const TRANSFER_BT2020_12_BIT: i32 = 15;

const PRIMARIES_BT709: i32 = 1;
const PRIMARIES_BT470_BG: i32 = 5;
const PRIMARIES_BT2020: i32 = 9;

const RANGE_UNSPECIFIED: i32 = 0;
const RANGE_LIMITED: i32 = 1;
const RANGE_FULL: i32 = 2;

const CHROMA_LOCATION_UNSPECIFIED: i32 = 0;
const CHROMA_LOCATION_LEFT: i32 = 1;
const CHROMA_LOCATION_CENTER: i32 = 2;
const CHROMA_LOCATION_TOP_LEFT: i32 = 3;
const CHROMA_LOCATION_TOP: i32 = 4;
const CHROMA_LOCATION_BOTTOM_LEFT: i32 = 5;
const CHROMA_LOCATION_BOTTOM: i32 = 6;

/// The layout and colour of a decoded frame, in the numbering BestSource reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDescription {
    pub width: i64,
    pub height: i64,
    /// The height before rounding down to whole chroma rows.
    pub encoded_height: i64,
    pub family: ColorFamily,
    pub bits: i32,
    pub float: bool,
    pub sub_sampling_w: i32,
    pub sub_sampling_h: i32,
    pub matrix: i32,
    pub transfer: i32,
    pub primaries: i32,
    pub range: i32,
    pub chroma_location: i32,
}

impl FrameDescription {
    /// Describes `frame` at the size `export_planar` writes.
    pub fn from_frame(frame: &Frame) -> Self {
        let format = frame.format();

        Self {
            width: i64::from(frame.ss_mod_width()),
            height: i64::from(frame.ss_mod_height()),
            encoded_height: i64::from(frame.height()),
            family: format.color_family,
            bits: format.bits,
            float: format.float,
            sub_sampling_w: format.sub_sampling_w,
            sub_sampling_h: format.sub_sampling_h,
            matrix: frame.matrix(),
            transfer: frame.transfer(),
            primaries: frame.primaries(),
            range: frame.range(),
            chroma_location: frame.chroma_location(),
        }
    }

    /// Describes how `other` differs from this description, if it differs at all.
    pub fn difference(&self, other: &Self) -> Option<String> {
        if let Some(difference) = self.layout_difference(other) {
            return Some(difference);
        }

        let fields = [
            ("encoded height", self.encoded_height, other.encoded_height),
            ("matrix", i64::from(self.matrix), i64::from(other.matrix)),
            ("transfer", i64::from(self.transfer), i64::from(other.transfer)),
            ("primaries", i64::from(self.primaries), i64::from(other.primaries)),
            ("range", i64::from(self.range), i64::from(other.range)),
            (
                "chroma location",
                i64::from(self.chroma_location),
                i64::from(other.chroma_location),
            ),
        ];
        let changed = fields
            .into_iter()
            .find(|(_, expected, actual)| expected != actual);
        let (name, expected, actual) = changed?;

        let message = format!("{name} is {actual} but expected {expected}");
        Some(message)
    }

    fn layout_difference(&self, other: &Self) -> Option<String> {
        if self.width != other.width || self.height != other.height {
            let (width, height) = (other.width, other.height);
            let (expected_width, expected_height) = (self.width, self.height);
            let message = format!("size is {width}x{height} but expected {expected_width}x{expected_height}");
            return Some(message);
        }

        if self.family != other.family {
            let (family, expected) = (other.family, self.family);
            let message = format!("colour family is {family:?} but expected {expected:?}");
            return Some(message);
        }

        if self.bits != other.bits {
            let message = format!("bit depth is {} but expected {}", other.bits, self.bits);
            return Some(message);
        }

        if self.float != other.float {
            let message = format!("float is {} but expected {}", other.float, self.float);
            return Some(message);
        }

        let actual = (other.sub_sampling_w, other.sub_sampling_h);
        let expected = (self.sub_sampling_w, self.sub_sampling_h);
        if actual != expected {
            let message = format!("chroma subsampling is {actual:?} but expected {expected:?}");
            return Some(message);
        }

        None
    }
}

/// A colour field checked against what Vship supports.
#[derive(Debug, Clone, Copy)]
enum ColourField {
    Matrix,
    Transfer,
    Primaries,
}

impl ColourField {
    fn name(self) -> &'static str {
        match self {
            Self::Matrix => "matrix",
            Self::Transfer => "transfer",
            Self::Primaries => "primaries",
        }
    }

    fn supported(self) -> &'static [i32] {
        match self {
            Self::Matrix => &SUPPORTED_MATRICES,
            Self::Transfer => &SUPPORTED_TRANSFERS,
            Self::Primaries => &SUPPORTED_PRIMARIES,
        }
    }

    fn unsupported_names(self) -> &'static [(i32, &'static str)] {
        match self {
            Self::Matrix => &UNSUPPORTED_MATRIX_NAMES,
            Self::Transfer => &UNSUPPORTED_TRANSFER_NAMES,
            Self::Primaries => &UNSUPPORTED_PRIMARIES_NAMES,
        }
    }

    fn check(self, value: i32) -> anyhow::Result<i32> {
        if self.supported().contains(&value) {
            return Ok(value);
        }

        let field = self.name();
        let known = self
            .unsupported_names()
            .iter()
            .find(|(known_value, _)| *known_value == value);
        match known {
            Some((_, name)) => bail!("the {field} {value} ({name}) is not supported"),
            None => bail!("the {field} {value} is not supported"),
        }
    }
}

fn sample_type(description: &FrameDescription) -> anyhow::Result<SampleType> {
    let bits = description.bits;
    let sample = match (description.float, bits) {
        (false, 8) => SampleType::Uint8,
        (false, 9) => SampleType::Uint9,
        (false, 10) => SampleType::Uint10,
        (false, 12) => SampleType::Uint12,
        (false, 14) => SampleType::Uint14,
        (false, 16) => SampleType::Uint16,
        (true, 16) => SampleType::Half,
        (true, 32) => SampleType::Float,
        (true, _) => bail!("{bits} bit float samples are not supported"),
        (false, _) => bail!("{bits} bit integer samples are not supported"),
    };

    Ok(sample)
}

fn matrix(description: &FrameDescription) -> anyhow::Result<u32> {
    if description.family == ColorFamily::Rgb {
        return Ok(0);
    }

    if description.matrix == 0 {
        bail!("the matrix 0 (identity) is only supported for RGB frames");
    }

    let is_high_definition = description.encoded_height > STANDARD_DEFINITION_HEIGHT;
    if description.matrix == UNSPECIFIED && is_high_definition {
        return Ok(MATRIX_BT709);
    }

    if description.matrix == UNSPECIFIED {
        return Ok(MATRIX_BT470_BG);
    }

    let value = ColourField::Matrix.check(description.matrix)?;

    Ok(value.unsigned_abs())
}

/// The transfer and primaries FFVship assumes for untagged values.
fn default_transfer_and_primaries(family: ColorFamily, matrix: u32) -> (i32, i32) {
    match (family, matrix) {
        (ColorFamily::Rgb, _) => (TRANSFER_SRGB, PRIMARIES_BT709),
        (_, MATRIX_BT470_BG) => (TRANSFER_BT470_BG, PRIMARIES_BT470_BG),
        (_, MATRIX_BT2020_NCL | MATRIX_BT2020_CL | MATRIX_ICTCP) => (TRANSFER_PQ, PRIMARIES_BT2020),
        _ => (TRANSFER_BT709, PRIMARIES_BT709),
    }
}

fn transfer(value: i32, default: i32) -> anyhow::Result<u32> {
    // BT.2020 10 and 12 bit (14 and 15) are defined as the same curve as BT.709.
    let value = match value {
        UNSPECIFIED => default,
        TRANSFER_BT2020_10_BIT | TRANSFER_BT2020_12_BIT => TRANSFER_BT709,
        other => other,
    };
    let value = ColourField::Transfer.check(value)?;

    Ok(value.unsigned_abs())
}

fn primaries(value: i32, default: i32) -> anyhow::Result<i32> {
    let value = if value == UNSPECIFIED { default } else { value };

    ColourField::Primaries.check(value)
}

fn range(value: i32, family: ColorFamily) -> anyhow::Result<Range> {
    let range = match (value, family) {
        (RANGE_UNSPECIFIED, ColorFamily::Rgb) => Range::Full,
        (RANGE_UNSPECIFIED | RANGE_LIMITED, _) => Range::Limited,
        (RANGE_FULL, _) => Range::Full,
        (other, _) => bail!("the range {other} is not supported"),
    };

    Ok(range)
}

fn chroma_location(value: i32) -> anyhow::Result<ChromaLocation> {
    let location = match value {
        CHROMA_LOCATION_UNSPECIFIED | CHROMA_LOCATION_LEFT => ChromaLocation::Left,
        CHROMA_LOCATION_CENTER => ChromaLocation::Center,
        CHROMA_LOCATION_TOP_LEFT => ChromaLocation::TopLeft,
        CHROMA_LOCATION_TOP => ChromaLocation::Top,
        CHROMA_LOCATION_BOTTOM_LEFT => {
            bail!("the chroma location {value} (bottom left) is not supported")
        },
        CHROMA_LOCATION_BOTTOM => bail!("the chroma location {value} (bottom) is not supported"),
        other => bail!("the chroma location {other} is not supported"),
    };

    Ok(location)
}

/// Maps a frame description to the colour space Vship scores it in.
///
/// Untagged values are guessed the way FFVship does. Values Vship does not support are
/// errors.
pub fn colour_space(description: &FrameDescription) -> anyhow::Result<ColorSpace> {
    match description.family {
        ColorFamily::Yuv | ColorFamily::Rgb => {},
        other => bail!("the {other:?} colour family is not supported"),
    }

    let sample = sample_type(description)?;
    let matrix = matrix(description)?;
    let defaults = default_transfer_and_primaries(description.family, matrix);
    let transfer = transfer(description.transfer, defaults.0)?;
    let primaries = primaries(description.primaries, defaults.1)?;
    let range = range(description.range, description.family)?;
    let chroma_location = chroma_location(description.chroma_location)?;

    Ok(ColorSpace {
        sample,
        width: description.width,
        height: description.height,
        range,
        sub_sampling_w: description.sub_sampling_w,
        sub_sampling_h: description.sub_sampling_h,
        chroma_location,
        matrix,
        transfer,
        primaries,
    })
}

/// Checks the reference and distorted frames can be scored against each other.
///
/// Bit depth may differ.
pub fn check_compatible(reference: &FrameDescription, distorted: &FrameDescription) -> anyhow::Result<()> {
    if reference.width != distorted.width || reference.height != distorted.height {
        let reference_size = format!("{}x{}", reference.width, reference.height);
        let distorted_size = format!("{}x{}", distorted.width, distorted.height);
        let message =
            format!("the reference size is {reference_size} but the distorted size is {distorted_size}");
        bail!("{message}");
    }

    let reference_shifts = (reference.sub_sampling_w, reference.sub_sampling_h);
    let distorted_shifts = (distorted.sub_sampling_w, distorted.sub_sampling_h);
    if reference_shifts != distorted_shifts {
        let message = format!(
            "the reference chroma subsampling is {reference_shifts:?} but the distorted \
             chroma subsampling is {distorted_shifts:?}"
        );
        bail!("{message}");
    }

    if reference.family != distorted.family {
        let (reference_family, distorted_family) = (reference.family, distorted.family);
        let message = format!(
            "the reference colour family is {reference_family:?} but the distorted colour family is \
             {distorted_family:?}"
        );
        bail!("{message}");
    }

    Ok(())
}

/// Host memory one frame is exported into, laid out for Vship.
#[derive(Debug)]
pub struct PlaneBuffers {
    description: FrameDescription,
    data: [Vec<u8>; 3],
    strides: [usize; 3],
}

impl PlaneBuffers {
    pub fn new(description: &FrameDescription) -> anyhow::Result<Self> {
        let space = colour_space(description)?;
        let width = usize::try_from(description.width).context("the frame width is not valid")?;
        let height = usize::try_from(description.height).context("the frame height is not valid")?;
        let shift_w = shift(description.sub_sampling_w)?;
        let shift_h = shift(description.sub_sampling_h)?;
        let sample_bytes = space.sample.bytes_per_sample();
        let subsampled = description.family == ColorFamily::Yuv;

        let mut data: [Vec<u8>; 3] = Default::default();
        let mut strides = [0; 3];
        for index in 0..3 {
            let chroma = index > 0 && subsampled;
            let plane_width = if chroma {
                round_up_shift(width, shift_w)
            } else {
                width
            };
            let plane_height = if chroma {
                round_up_shift(height, shift_h)
            } else {
                height
            };
            let stride = plane_width
                .checked_mul(sample_bytes)
                .context("the frame is too large")?;
            let length = stride
                .checked_mul(plane_height)
                .context("the frame is too large")?;
            strides[index] = stride;
            data[index] = vec![0; length];
        }

        Ok(Self {
            description: *description,
            data,
            strides,
        })
    }

    /// Copies `frame` into the buffers, which must have been sized for its layout.
    pub fn export(&mut self, frame: &Frame) -> anyhow::Result<()> {
        let described = FrameDescription::from_frame(frame);
        if let Some(difference) = self.description.layout_difference(&described) {
            bail!("the frame does not match the buffers, {difference}");
        }

        let planes = self
            .data
            .iter_mut()
            .zip(self.strides)
            .map(|(data, stride)| PlaneTarget { data, stride })
            .collect();
        let mut target = PlanarTarget { planes, alpha: None };
        frame.export_planar(&mut target)?;

        Ok(())
    }

    pub fn planes(&self) -> Planes<'_> {
        Planes {
            data: [&self.data[0], &self.data[1], &self.data[2]],
            strides: self.strides,
        }
    }
}

fn shift(value: i32) -> anyhow::Result<u32> {
    match u32::try_from(value) {
        Ok(shift @ 0..=2) => Ok(shift),
        _ => bail!("the chroma subsampling shift {value} is not supported"),
    }
}

fn round_up_shift(size: usize, shift: u32) -> usize {
    let mask = (1usize << shift) - 1;
    (size + mask) >> shift
}
