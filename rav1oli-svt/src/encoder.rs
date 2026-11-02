use std::fs::File;
use std::io::{self, BufWriter};
use std::mem::{self, MaybeUninit};
use std::path::{Path, PathBuf};
use std::{ptr, slice};

use crate::error::{EncodeError, GridError, ParamError};
use crate::grids::DistortionGrids;
use crate::ivf::IvfWriter;
use crate::params::{self, DefaultConfig, SVT_SUCCESS};
use crate::sys;

/// The only bit depth the fork encodes at.
const ENCODED_BIT_DEPTH: u32 = 10;
const MAX_TEN_BIT_SAMPLE: u16 = 1023;

/// Where the chroma samples sit relative to the luma samples, as in H.273.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ChromaPosition {
    Left,
    Center,
    TopLeft,
    Top,
}

/// The colour signalling written into the stream.
///
/// `matrix`, `transfer` and `primaries` are H.273 codes.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ColourDescription {
    pub matrix: u8,
    pub transfer: u8,
    pub primaries: u8,
    pub full_range: bool,
    pub chroma_position: ChromaPosition,
}

/// The highest SIMD level SVT-AV1 may pick.
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub enum CpuCap {
    /// SVT-AV1's own choice for the host.
    #[default]
    Default,
    /// At most AVX2, which only an x86_64 build has.
    Avx2,
}

/// The settings an [Encoder] is created with.
///
/// `svt_args` are extra SVT-AV1 flags in `SvtAv1EncApp` style, such as `["--tune", "0"]`.
#[derive(Debug, Clone)]
pub struct EncoderSettings {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub colour: ColourDescription,
    pub fps_numerator: u32,
    pub fps_denominator: u32,
    pub preset: i8,
    pub crf: u8,
    pub svt_args: Vec<String>,
    pub cpu_cap: CpuCap,
}

impl EncoderSettings {
    /// Caps the SIMD level SVT-AV1 picks, for profiling kernels the host would not pick by default.
    pub fn with_cpu_cap(self, cap: CpuCap) -> Self {
        Self { cpu_cap: cap, ..self }
    }
}

/// The Y, U and V planes of one 4:2:0 frame.
///
/// Strides count samples, not bytes.
#[derive(Debug, Copy, Clone)]
pub enum Planes<'a> {
    Eight {
        y: &'a [u8],
        u: &'a [u8],
        v: &'a [u8],
        strides: [usize; 3],
    },
    Ten {
        y: &'a [u16],
        u: &'a [u16],
        v: &'a [u16],
        strides: [usize; 3],
    },
}

/// What a finished encode produced.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct EncodeSummary {
    pub frames: u64,
}

/// An SVT-AV1 encoder that writes its packets to an IVF file.
///
/// Dropping it without [Encoder::finish] releases SVT but leaves the file unfinished.
pub struct Encoder {
    session: Session,
    writer: IvfWriter<BufWriter<File>>,
    output: PathBuf,
    layout: FrameLayout,
    bit_depth: u8,
    widened: Vec<u16>,
    payload: Vec<u32>,
}

impl Encoder {
    /// Sets SVT-AV1 up from `settings` and creates the IVF file at `output`.
    pub fn new(settings: &EncoderSettings, output: &Path) -> Result<Self, EncodeError> {
        #[cfg(svt_pgo_generate)]
        crate::profiling::keep_profile_runtime();

        if settings.bit_depth != 8 && settings.bit_depth != 10 {
            let reason = format!(
                "a bit depth of {} is not supported, only 8 or 10",
                settings.bit_depth
            );
            return Err(EncodeError::UnsupportedFormat { reason });
        }

        let layout = FrameLayout::new(settings.width, settings.height)?;
        check_colour(&settings.colour)?;

        let mut config = DefaultConfig::new()?;
        apply_settings(&mut config.config, settings)?;
        params::apply_params(&mut config.config, &settings.svt_args)?;
        params::check_requirements(&config.config)?;

        let file = File::create(output).map_err(|source| io_error(output, source))?;
        let buffered = BufWriter::new(file);
        let writer = IvfWriter::new(
            buffered,
            layout.ivf_width,
            layout.ivf_height,
            settings.fps_numerator,
            settings.fps_denominator,
        );
        let writer = writer.map_err(|source| io_error(output, source))?;

        let mut session = Session {
            config,
            is_started: false,
            is_eos_sent: false,
            frames_sent: 0,
        };
        session.start()?;

        Ok(Self {
            session,
            writer,
            output: output.to_path_buf(),
            layout,
            bit_depth: settings.bit_depth,
            widened: Vec::new(),
            payload: Vec::new(),
        })
    }

    /// Sends one frame, with its distortion map when given, and writes any packets SVT has ready.
    pub fn send_frame(
        &mut self,
        planes: &Planes<'_>,
        grids: Option<&DistortionGrids>,
    ) -> Result<(), EncodeError> {
        let mut io_format = match *planes {
            Planes::Eight { y, u, v, strides } => self.widen_planes([y, u, v], strides)?,
            Planes::Ten { y, u, v, strides } => self.ten_bit_format([y, u, v], strides)?,
        };

        let mut node = None;
        if let Some(grids) = grids {
            let built = self.distortion_node(grids)?;
            node = Some(built);
        }

        let node_pointer = match node.as_mut() {
            Some(node) => ptr::from_mut(node),
            None => ptr::null_mut(),
        };

        let mut header = zeroed_header();
        header.p_buffer = ptr::from_mut(&mut io_format).cast::<u8>();
        header.n_filled_len = self.layout.frame_bytes;
        header.n_alloc_len = self.layout.frame_bytes;
        header.p_app_private = node_pointer.cast();
        header.pts = self.session.frames_sent as i64;

        let handle = self.session.config.handle();
        // SAFETY: The header, its planes and the distortion node all outlive the call, and SVT copies
        // the samples and the node's block before returning.
        let status = unsafe { sys::svt_av1_enc_send_picture(handle, &mut header) };
        check_status("svt_av1_enc_send_picture", status)?;
        self.session.frames_sent += 1;

        self.write_packets(false)
    }

    /// Ends the stream, writes the remaining packets and fixes up the IVF header.
    pub fn finish(mut self) -> Result<EncodeSummary, EncodeError> {
        self.session.send_eos()?;
        self.write_packets(true)?;

        let Encoder { writer, output, .. } = self;
        let (buffered, frames) = writer.finish().map_err(|source| io_error(&output, source))?;
        let file = buffered
            .into_inner()
            .map_err(|error| io_error(&output, error.into_error()))?;
        file.sync_all().map_err(|source| io_error(&output, source))?;

        Ok(EncodeSummary { frames })
    }

    /// Widens 8-bit planes into the reused buffer and points SVT at them.
    fn widen_planes(
        &mut self,
        planes: [&[u8]; 3],
        strides: [usize; 3],
    ) -> Result<sys::EbSvtIOFormat, EncodeError> {
        self.check_bit_depth(8)?;

        let lengths = planes.map(<[u8]>::len);
        self.layout.check_planes(lengths, strides)?;

        let FrameLayout {
            width,
            height,
            chroma_width,
            chroma_height,
            ..
        } = self.layout;
        let [y_plane, u_plane, v_plane] = planes;
        let [y_stride, u_stride, v_stride] = strides;

        self.widened.clear();
        widen_plane(y_plane, y_stride, width, height, &mut self.widened);
        widen_plane(u_plane, u_stride, chroma_width, chroma_height, &mut self.widened);
        widen_plane(v_plane, v_stride, chroma_width, chroma_height, &mut self.widened);

        let (y_widened, chroma_widened) = self.widened.split_at(width * height);
        let (u_widened, v_widened) = chroma_widened.split_at(chroma_width * chroma_height);
        let widened_strides = [width, chroma_width, chroma_width];

        io_format([y_widened, u_widened, v_widened], widened_strides)
    }

    fn ten_bit_format(
        &self,
        planes: [&[u16]; 3],
        strides: [usize; 3],
    ) -> Result<sys::EbSvtIOFormat, EncodeError> {
        self.check_bit_depth(10)?;

        let lengths = planes.map(<[u16]>::len);
        self.layout.check_planes(lengths, strides)?;
        self.layout.check_ten_bit_range(planes, strides)?;

        io_format(planes, strides)
    }

    fn check_bit_depth(&self, planes_depth: u8) -> Result<(), EncodeError> {
        if planes_depth == self.bit_depth {
            return Ok(());
        }

        let reason = format!(
            "{planes_depth}-bit planes were sent to an encoder set up for {}-bit input",
            self.bit_depth,
        );
        Err(EncodeError::UnsupportedFormat { reason })
    }

    /// Builds the distortion map node, whose block lives in the reused payload buffer.
    fn distortion_node(&mut self, grids: &DistortionGrids) -> Result<sys::EbPrivDataNode, EncodeError> {
        let is_same_size =
            grids.width() == self.layout.width as u32 && grids.height() == self.layout.height as u32;
        if !is_same_size {
            let error = GridError::FrameMismatch {
                grid_width: grids.width(),
                grid_height: grids.height(),
                frame_width: self.layout.width as u32,
                frame_height: self.layout.height as u32,
            };
            return Err(error.into());
        }

        let byte_len = grids.write_payload(&mut self.payload);
        let Ok(size) = u32::try_from(byte_len) else {
            let reason = format!("a distortion map of {byte_len} bytes is too large for SVT-AV1");
            return Err(EncodeError::UnsupportedFormat { reason });
        };

        Ok(sys::EbPrivDataNode {
            node_type: sys::DISTORTION_MAP_EVENT,
            data: self.payload.as_mut_ptr().cast(),
            size,
            next: ptr::null_mut(),
        })
    }

    /// Writes every packet SVT has ready, waiting for the end of the stream when `is_done` is set.
    fn write_packets(&mut self, is_done: bool) -> Result<(), EncodeError> {
        let handle = self.session.config.handle();

        loop {
            let mut packet: *mut sys::EbBufferHeaderType = ptr::null_mut();
            // SAFETY: The handle is a started encoder. A blocking call only happens after the end of stream
            // was sent, as the fork's `verify_settings` rejects every prediction structure but random access,
            // so SVT's always-blocking low-delay mode cannot occur.
            let status = unsafe { sys::svt_av1_enc_get_packet(handle, &mut packet, u8::from(is_done)) };
            if status == sys::EB_NoErrorEmptyQueue {
                return Ok(());
            }

            if packet.is_null() {
                return Err(EncodeError::Svt {
                    call: "svt_av1_enc_get_packet",
                    code: status,
                });
            }

            // SAFETY: SVT handed out this packet, and it stays valid until it is released below.
            let packet_header = unsafe { &*packet };
            let is_eos = packet_header.flags & sys::EB_BUFFERFLAG_EOS != 0;
            let length = packet_header.n_filled_len as usize;
            let has_data = length > 0 && !packet_header.p_buffer.is_null();

            let written = if status == SVT_SUCCESS && has_data {
                // SAFETY: SVT filled `n_filled_len` bytes at `p_buffer`.
                let data = unsafe { slice::from_raw_parts(packet_header.p_buffer, length) };
                self.writer.write_packet(data, packet_header.pts)
            } else {
                Ok(())
            };

            // SAFETY: The packet came from `svt_av1_enc_get_packet` and is released once.
            unsafe { sys::svt_av1_enc_release_out_buffer(&mut packet) };

            check_status("svt_av1_enc_get_packet", status)?;
            written.map_err(|source| io_error(&self.output, source))?;

            if is_eos {
                return Ok(());
            }
        }
    }
}

impl std::fmt::Debug for Encoder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Encoder")
            .field("output", &self.output)
            .field("width", &self.layout.width)
            .field("height", &self.layout.height)
            .field("bit_depth", &self.bit_depth)
            .field("frames_sent", &self.session.frames_sent)
            .finish_non_exhaustive()
    }
}

/// Widens one 8-bit plane to 10-bit with `sample << 2`, appending it to `output` without row padding.
pub(crate) fn widen_plane(plane: &[u8], stride: usize, width: usize, rows: usize, output: &mut Vec<u16>) {
    for row in plane.chunks(stride).take(rows) {
        let samples = row[..width].iter().map(|&sample| u16::from(sample) << 2);
        output.extend(samples);
    }
}

/// The SVT handle behind an [Encoder], released on drop.
struct Session {
    config: DefaultConfig,
    is_started: bool,
    is_eos_sent: bool,
    frames_sent: u64,
}

impl Session {
    fn start(&mut self) -> Result<(), EncodeError> {
        let handle = self.config.handle();

        // SAFETY: The handle and config are valid. SVT deep-copies the config except for its zones,
        // which the config keeps alive until after the handle is released.
        let status = unsafe { sys::svt_av1_enc_set_parameter(handle, &mut self.config.config) };
        check_status("svt_av1_enc_set_parameter", status)?;

        // `svt_av1_enc_deinit` also cleans up after a failed init, as `SvtAv1EncApp` relies on.
        self.is_started = true;

        // SAFETY: The handle has its parameters set.
        let status = unsafe { sys::svt_av1_enc_init(handle) };
        check_status("svt_av1_enc_init", status)
    }

    fn send_eos(&mut self) -> Result<(), EncodeError> {
        let mut header = zeroed_header();
        header.flags = sys::EB_BUFFERFLAG_EOS;

        let handle = self.config.handle();
        self.is_eos_sent = true;

        // SAFETY: The handle is a started encoder, and an end of stream header carries no picture.
        let status = unsafe { sys::svt_av1_enc_send_picture(handle, &mut header) };
        check_status("svt_av1_enc_send_picture", status)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.is_started {
            return;
        }

        // SVT logs an error when it is torn down mid-stream, so the stream is ended first.
        if self.frames_sent > 0 && !self.is_eos_sent {
            let sent = self.send_eos();
            if let Err(error) = sent {
                tracing::warn!("could not end the stream before releasing SVT-AV1: {error}");
            }
        }

        let handle = self.config.handle();
        // SAFETY: The handle was started and is deinitialised once, before `DefaultConfig` releases it.
        let status = unsafe { sys::svt_av1_enc_deinit(handle) };
        if status != SVT_SUCCESS {
            tracing::warn!(
                code = status,
                "SVT-AV1 call `svt_av1_enc_deinit` failed with status {status:#x}"
            );
        }
    }
}

/// The sample counts of a 4:2:0 frame.
#[derive(Debug, Copy, Clone)]
struct FrameLayout {
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    ivf_width: u16,
    ivf_height: u16,
    frame_bytes: u32,
}

impl FrameLayout {
    fn new(width: u32, height: u32) -> Result<Self, EncodeError> {
        let ivf_width = u16::try_from(width).ok().filter(|&width| width > 0);
        let ivf_height = u16::try_from(height).ok().filter(|&height| height > 0);
        let (Some(ivf_width), Some(ivf_height)) = (ivf_width, ivf_height) else {
            let reason = format!("a {width}x{height} frame is not supported, each side must be 1 to 65535");
            return Err(EncodeError::UnsupportedFormat { reason });
        };

        let width = usize::from(ivf_width);
        let height = usize::from(ivf_height);
        let chroma_width = width.div_ceil(2);
        let chroma_height = height.div_ceil(2);

        let samples = width * height + 2 * chroma_width * chroma_height;
        let sample_bytes = samples * mem::size_of::<u16>();
        let Ok(frame_bytes) = u32::try_from(sample_bytes) else {
            let reason = format!("a {width}x{height} frame is too large for SVT-AV1");
            return Err(EncodeError::UnsupportedFormat { reason });
        };

        Ok(Self {
            width,
            height,
            chroma_width,
            chroma_height,
            ivf_width,
            ivf_height,
            frame_bytes,
        })
    }

    /// Checks each plane holds exactly the rows a 4:2:0 frame of this size needs.
    fn check_planes(&self, lengths: [usize; 3], strides: [usize; 3]) -> Result<(), EncodeError> {
        let names = ["Y", "U", "V"];
        let widths = [self.width, self.chroma_width, self.chroma_width];
        let rows = [self.height, self.chroma_height, self.chroma_height];

        for plane in 0..3 {
            let stride = strides[plane];
            let width = widths[plane];
            let name = names[plane];

            if stride < width {
                let reason =
                    format!("the {name} plane's stride of {stride} is less than its width of {width}");
                return Err(EncodeError::UnsupportedFormat { reason });
            }

            let shortest = stride * (rows[plane] - 1) + width;
            let longest = stride * rows[plane];
            let length = lengths[plane];
            if length < shortest || length > longest {
                let frame_width = self.width;
                let frame_height = self.height;
                let reason = format!(
                    "the {name} plane holds {length} samples, but a 4:2:0 {frame_width}x{frame_height} frame \
                     with a stride of {stride} needs between {shortest} and {longest}"
                );
                return Err(EncodeError::UnsupportedFormat { reason });
            }
        }

        Ok(())
    }

    /// Checks every in-frame 10-bit sample fits in 10 bits, as SVT wraps larger values.
    fn check_ten_bit_range(&self, planes: [&[u16]; 3], strides: [usize; 3]) -> Result<(), EncodeError> {
        let names = ["Y", "U", "V"];
        let widths = [self.width, self.chroma_width, self.chroma_width];

        for plane in 0..3 {
            let width = widths[plane];
            let rows = planes[plane].chunks(strides[plane]);

            for (row_index, row) in rows.enumerate() {
                let samples = &row[..width];
                let row_max = samples.iter().copied().max().unwrap_or(0);
                if row_max <= MAX_TEN_BIT_SAMPLE {
                    continue;
                }

                let column = samples.iter().position(|&sample| sample > MAX_TEN_BIT_SAMPLE);
                let column = column.unwrap_or(0);
                let name = names[plane];
                let reason = format!(
                    "the {name} plane holds {row_max} at row {row_index}, column {column}, \
                     above the 10-bit maximum of {MAX_TEN_BIT_SAMPLE}"
                );
                return Err(EncodeError::UnsupportedFormat { reason });
            }
        }

        Ok(())
    }
}

/// Points SVT at three planes of 16-bit samples.
fn io_format(planes: [&[u16]; 3], strides: [usize; 3]) -> Result<sys::EbSvtIOFormat, EncodeError> {
    let mut strides_u32 = [0u32; 3];
    for (stride, converted) in strides.iter().zip(&mut strides_u32) {
        let Ok(stride_u32) = u32::try_from(*stride) else {
            let reason = format!("a stride of {stride} is too large for SVT-AV1");
            return Err(EncodeError::UnsupportedFormat { reason });
        };
        *converted = stride_u32;
    }

    let [y_plane, u_plane, v_plane] = planes;
    let [y_stride, u_stride, v_stride] = strides_u32;

    // SVT only reads the planes, despite taking mutable pointers.
    Ok(sys::EbSvtIOFormat {
        luma: y_plane.as_ptr().cast::<u8>().cast_mut(),
        cb: u_plane.as_ptr().cast::<u8>().cast_mut(),
        cr: v_plane.as_ptr().cast::<u8>().cast_mut(),
        y_stride,
        cr_stride: v_stride,
        cb_stride: u_stride,
    })
}

/// Sets the size, frame rate, bit depth, colour, preset, CRF and CPU cap, and turns the distortion map on.
fn apply_settings(
    config: &mut sys::EbSvtAv1EncConfiguration,
    settings: &EncoderSettings,
) -> Result<(), EncodeError> {
    config.source_width = settings.width;
    config.source_height = settings.height;
    config.frame_rate_numerator = settings.fps_numerator;
    config.frame_rate_denominator = settings.fps_denominator;
    config.encoder_bit_depth = ENCODED_BIT_DEPTH;
    config.encoder_color_format = sys::EB_YUV420;
    config.enable_distortion_map = true;
    apply_colour(config, &settings.colour);

    let preset = settings.preset.to_string();
    params::parse_one(config, "preset", &preset)?;

    let crf = settings.crf.to_string();
    params::parse_one(config, "crf", &crf)?;

    if settings.cpu_cap == CpuCap::Avx2 {
        apply_avx2_cap(config)?;
    }

    Ok(())
}

/// Caps SVT at AVX2 through `asm`, which `svt_args` cannot set as the encoder owns it.
fn apply_avx2_cap(config: &mut sys::EbSvtAv1EncConfiguration) -> Result<(), EncodeError> {
    if !cfg!(target_arch = "x86_64") {
        let reason = "the AVX2 CPU cap needs an x86_64 build of SVT-AV1";
        let error = ParamError::Requirement { reason };
        return Err(error.into());
    }

    params::parse_one(config, "asm", "avx2")?;

    Ok(())
}

/// Sets the colour fields. AV1 can only signal left and top-left chroma siting, so others are left unknown.
fn apply_colour(config: &mut sys::EbSvtAv1EncConfiguration, colour: &ColourDescription) {
    config.matrix_coefficients = colour.matrix.into();
    config.transfer_characteristics = colour.transfer.into();
    config.color_primaries = colour.primaries.into();
    config.color_range_provided = true;

    config.color_range = if colour.full_range {
        sys::EB_CR_FULL_RANGE
    } else {
        sys::EB_CR_STUDIO_RANGE
    };

    config.chroma_sample_position = match colour.chroma_position {
        ChromaPosition::Left => sys::EB_CSP_VERTICAL,
        ChromaPosition::TopLeft => sys::EB_CSP_COLOCATED,
        ChromaPosition::Center | ChromaPosition::Top => sys::EB_CSP_UNKNOWN,
    };
}

/// Rejects H.273 codes that are reserved, or that the fork cannot encode.
fn check_colour(colour: &ColourDescription) -> Result<(), EncodeError> {
    let checks = [
        ("matrix", colour.matrix, matrix_problem(colour.matrix)),
        ("transfer", colour.transfer, transfer_problem(colour.transfer)),
        ("primaries", colour.primaries, primaries_problem(colour.primaries)),
    ];

    for (field, code, problem) in checks {
        if let Some(reason) = problem {
            return Err(EncodeError::InvalidColourCode { field, code, reason });
        }
    }

    Ok(())
}

fn matrix_problem(code: u8) -> Option<&'static str> {
    match code {
        0 => Some("is the identity matrix, which needs 4:4:4 but the fork only encodes 4:2:0"),
        1 | 2 | 4..=14 => None,
        _ => Some("is reserved in H.273"),
    }
}

fn transfer_problem(code: u8) -> Option<&'static str> {
    match code {
        1 | 2 | 4..=18 => None,
        _ => Some("is reserved in H.273"),
    }
}

fn primaries_problem(code: u8) -> Option<&'static str> {
    match code {
        1 | 2 | 4..=12 | 22 => None,
        _ => Some("is reserved in H.273"),
    }
}

fn zeroed_header() -> sys::EbBufferHeaderType {
    let header = MaybeUninit::<sys::EbBufferHeaderType>::zeroed();
    // SAFETY: Every field is an integer, float or raw pointer, for which zero is valid.
    let mut header = unsafe { header.assume_init() };
    header.size = mem::size_of::<sys::EbBufferHeaderType>() as u32;
    header.pic_type = sys::EB_AV1_INVALID_PICTURE;

    header
}

fn check_status(call: &'static str, status: sys::EbErrorType) -> Result<(), EncodeError> {
    if status == SVT_SUCCESS {
        return Ok(());
    }

    Err(EncodeError::Svt { call, code: status })
}

fn io_error(path: &Path, source: io::Error) -> EncodeError {
    EncodeError::Io {
        path: path.to_path_buf(),
        source,
    }
}
