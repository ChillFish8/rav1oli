use std::env::{self, VarError};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use rav1oli_bestsource::{
    ColorFamily,
    Frame,
    IndexCache,
    OpenOptions,
    PlanarTarget,
    PlaneTarget,
    VideoFormat,
    VideoSource,
};
use rav1oli_svt::{
    ChromaPosition,
    ColourDescription,
    CpuCap,
    DistortionGrids,
    Encoder,
    EncoderSettings,
    MAX_CUT,
    MAX_DROP,
    MapMode,
    Planes,
};

const CLIP_VARIABLE: &str = "RAV1OLI_PGO_CLIP";
const START_VARIABLE: &str = "RAV1OLI_PGO_START";
const FRAMES_VARIABLE: &str = "RAV1OLI_PGO_FRAMES";
const PRESET_VARIABLE: &str = "RAV1OLI_PGO_PRESET";
const CAP_VARIABLE: &str = "RAV1OLI_PGO_CAP_AVX2";
const OUTPUT_VARIABLE: &str = "RAV1OLI_PGO_OUTPUT";

/// One minute in at 30 fps.
const DEFAULT_START: i64 = 1800;
const DEFAULT_FRAMES: i64 = 240;
/// The `rav1oli` defaults.
const DEFAULT_PRESET: i8 = 4;
const CRF: u8 = 30;
const GRID_SEED: u32 = 0x5eed;

type TrainResult<T> = Result<T, Box<dyn Error>>;

/// What one training run encodes, read from the environment.
struct TrainConfig {
    clip: PathBuf,
    start: i64,
    frames: i64,
    preset: i8,
    cpu_cap: CpuCap,
    output: PathBuf,
}

/// The planes of the frame being encoded, as BestSource writes them.
struct FrameBuffers {
    width: usize,
    height: usize,
    bytes_per_sample: usize,
    bytes: [Vec<u8>; 3],
    samples: [Vec<u16>; 3],
}

/// The xorshift32 generator the synthetic grids are drawn from.
struct XorShift32 {
    state: u32,
}

fn main() -> ExitCode {
    let result = run();
    let Err(error) = result else {
        return ExitCode::SUCCESS;
    };

    eprintln!("pgo-train: {error}");
    ExitCode::FAILURE
}

fn run() -> TrainResult<()> {
    let config = TrainConfig::from_env()?;
    let started = Instant::now();

    let output_dir = config.output.parent().unwrap_or(Path::new(""));
    let index_dir = output_dir.join("bsindex");
    let options = OpenOptions {
        cache: IndexCache::Directory(index_dir),
        ..OpenOptions::default()
    };
    let mut source = VideoSource::open(&config.clip, &options)?;
    let properties = *source.properties();

    check_format(&config.clip, &properties.format)?;
    check_range(&config, properties.num_frames)?;

    let width = u32::try_from(properties.ss_mod_width)?;
    let height = u32::try_from(properties.ss_mod_height)?;
    let fps_numerator = u32::try_from(properties.fps.0)?;
    let fps_denominator = u32::try_from(properties.fps.1)?;
    let bit_depth = u8::try_from(properties.format.bits)?;

    // The clip's colour signalling does not change what SVT-AV1 runs, so BT.709 is written for every clip.
    let colour = ColourDescription {
        matrix: 1,
        transfer: 1,
        primaries: 1,
        full_range: false,
        chroma_position: ChromaPosition::Left,
    };
    let settings = EncoderSettings {
        width,
        height,
        bit_depth,
        colour,
        fps_numerator,
        fps_denominator,
        preset: config.preset,
        crf: CRF,
        svt_args: Vec::new(),
        cpu_cap: CpuCap::Default,
        map_mode: Some(MapMode::Additive),
    };
    let settings = settings.with_cpu_cap(config.cpu_cap);
    let mut encoder = Encoder::new(&settings, &config.output)?;

    let bytes_per_sample = properties.format.bytes_per_sample();
    let mut buffers = FrameBuffers::new(width as usize, height as usize, bytes_per_sample);
    let mut random = XorShift32 { state: GRID_SEED };

    let end = config.start + config.frames;
    for index in config.start..end {
        let frame = source.frame(index)?;
        buffers.export(&frame)?;

        let grids = random_grids(&mut random, width, height)?;
        let planes = buffers.planes();
        encoder.send_frame(&planes, Some(&grids))?;
    }

    let summary = encoder.finish()?;
    let elapsed = started.elapsed().as_secs_f64();
    let shown_output = config.output.display();
    println!(
        "pgo-train: encoded {} frames at preset {} with {:?} into {shown_output} in {elapsed:.1}s",
        summary.frames, config.preset, config.cpu_cap,
    );

    Ok(())
}

impl TrainConfig {
    fn from_env() -> TrainResult<Self> {
        let clip = env::var_os(CLIP_VARIABLE).filter(|clip| !clip.is_empty());
        let Some(clip) = clip else {
            let reason = format!("{CLIP_VARIABLE} must name the clip to train on");
            return Err(reason.into());
        };

        let start = count_variable(START_VARIABLE, DEFAULT_START)?;
        let frames = count_variable(FRAMES_VARIABLE, DEFAULT_FRAMES)?;
        if frames == 0 {
            let reason = format!("{FRAMES_VARIABLE} must be at least 1");
            return Err(reason.into());
        }

        let preset = preset_variable()?;
        let cpu_cap = cpu_cap_variable()?;

        let output = env::var_os(OUTPUT_VARIABLE).filter(|output| !output.is_empty());
        let output = match output {
            Some(output) => PathBuf::from(output),
            None => env::temp_dir().join("rav1oli-pgo-train.ivf"),
        };

        Ok(Self {
            clip: PathBuf::from(clip),
            start,
            frames,
            preset,
            cpu_cap,
            output,
        })
    }
}

/// A count of frames from `name`, or `default` when it is unset.
fn count_variable(name: &str, default: i64) -> TrainResult<i64> {
    let value = env::var(name);
    let text = match value {
        Ok(text) => text,
        Err(VarError::NotPresent) => return Ok(default),
        Err(VarError::NotUnicode(_)) => {
            let reason = format!("{name} is not valid utf-8");
            return Err(reason.into());
        },
    };

    let count = text.parse::<i64>();
    match count {
        Ok(count) if count >= 0 => Ok(count),
        _ => {
            let reason = format!("{name} is `{text}`, which is not a whole number of frames");
            Err(reason.into())
        },
    }
}

/// The SVT-AV1 preset, which SVT itself checks the range of.
fn preset_variable() -> TrainResult<i8> {
    let value = env::var(PRESET_VARIABLE);
    let text = match value {
        Ok(text) => text,
        Err(VarError::NotPresent) => return Ok(DEFAULT_PRESET),
        Err(VarError::NotUnicode(_)) => {
            let reason = format!("{PRESET_VARIABLE} is not valid utf-8");
            return Err(reason.into());
        },
    };

    let preset = text.parse::<i8>();
    match preset {
        Ok(preset) => Ok(preset),
        Err(_) => {
            let reason = format!("{PRESET_VARIABLE} is `{text}`, which is not a preset number");
            Err(reason.into())
        },
    }
}

fn cpu_cap_variable() -> TrainResult<CpuCap> {
    let value = env::var(CAP_VARIABLE);
    let text = match value {
        Ok(text) => text,
        Err(VarError::NotPresent) => return Ok(CpuCap::Default),
        Err(VarError::NotUnicode(_)) => {
            let reason = format!("{CAP_VARIABLE} is not valid utf-8");
            return Err(reason.into());
        },
    };

    match text.as_str() {
        "" | "0" => Ok(CpuCap::Default),
        "1" => Ok(CpuCap::Avx2),
        _ => {
            let reason = format!("{CAP_VARIABLE} is `{text}`, but it must be 0 or 1");
            Err(reason.into())
        },
    }
}

/// Checks the clip is 8-bit or 10-bit 4:2:0 YUV, the only input the fork encodes.
fn check_format(clip: &Path, format: &VideoFormat) -> TrainResult<()> {
    let is_yuv = format.color_family == ColorFamily::Yuv && !format.float && !format.alpha;
    let is_four_two_zero = format.sub_sampling_w == 1 && format.sub_sampling_h == 1;
    let is_supported_depth = format.bits == 8 || format.bits == 10;
    if is_yuv && is_four_two_zero && is_supported_depth {
        return Ok(());
    }

    let shown_clip = clip.display();
    let reason = format!("{shown_clip} is {format:?}, but training needs 8-bit or 10-bit 4:2:0 YUV");
    Err(reason.into())
}

fn check_range(config: &TrainConfig, clip_frames: i64) -> TrainResult<()> {
    let end = config.start + config.frames;
    if end <= clip_frames {
        return Ok(());
    }

    let shown_clip = config.clip.display();
    let start = config.start;
    let reason =
        format!("{shown_clip} has {clip_frames} frames, so frames {start} to {end} are past its end");
    Err(reason.into())
}

impl FrameBuffers {
    fn new(width: usize, height: usize, bytes_per_sample: usize) -> Self {
        let chroma_samples = (width / 2) * (height / 2);
        let sample_counts = [width * height, chroma_samples, chroma_samples];
        let bytes = sample_counts.map(|samples| vec![0u8; samples * bytes_per_sample]);

        Self {
            width,
            height,
            bytes_per_sample,
            bytes,
            samples: Default::default(),
        }
    }

    fn strides(&self) -> [usize; 3] {
        let chroma_width = self.width / 2;
        [self.width, chroma_width, chroma_width]
    }

    /// Copies `frame` into the buffers, widening 10-bit bytes into samples.
    fn export(&mut self, frame: &Frame) -> TrainResult<()> {
        let frame_width = frame.ss_mod_width() as usize;
        let frame_height = frame.ss_mod_height() as usize;
        if frame_width != self.width || frame_height != self.height {
            let (width, height) = (self.width, self.height);
            let reason = format!("a {frame_width}x{frame_height} frame arrived in a {width}x{height} clip");
            return Err(reason.into());
        }

        let strides = self.strides();
        let bytes_per_sample = self.bytes_per_sample;
        let planes = self
            .bytes
            .iter_mut()
            .zip(strides)
            .map(|(data, stride)| PlaneTarget {
                data,
                stride: stride * bytes_per_sample,
            })
            .collect();
        let mut target = PlanarTarget { planes, alpha: None };
        frame.export_planar(&mut target)?;

        if bytes_per_sample == 1 {
            return Ok(());
        }

        for (bytes, samples) in self.bytes.iter().zip(&mut self.samples) {
            let (pairs, _) = bytes.as_chunks::<2>();
            let widened = pairs.iter().map(|pair| u16::from_ne_bytes(*pair));
            samples.clear();
            samples.extend(widened);
        }

        Ok(())
    }

    fn planes(&self) -> Planes<'_> {
        let strides = self.strides();

        if self.bytes_per_sample == 1 {
            let [y_plane, u_plane, v_plane] = &self.bytes;
            return Planes::Eight {
                y: y_plane,
                u: u_plane,
                v: v_plane,
                strides,
            };
        }

        let [y_plane, u_plane, v_plane] = &self.samples;
        Planes::Ten {
            y: y_plane,
            u: u_plane,
            v: v_plane,
            strides,
        }
    }
}

impl XorShift32 {
    fn next(&mut self) -> u32 {
        let mut state = self.state;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.state = state;

        state
    }

    /// A value between 0 and `max`, or 0 for about one cell in four.
    fn cell(&mut self, max: u8) -> u8 {
        let is_forced_zero = self.next().is_multiple_of(4);
        if is_forced_zero {
            return 0;
        }

        let range = u32::from(max) + 1;
        let value = self.next() % range;
        value as u8
    }
}

/// Grids covering the full drop and cut ranges, so every picture runs the distortion map paths.
fn random_grids(random: &mut XorShift32, width: u32, height: u32) -> TrainResult<DistortionGrids> {
    let drop_cells = width.div_ceil(64) as usize * height.div_ceil(64) as usize;
    let cut_cells = width.div_ceil(8) as usize * height.div_ceil(8) as usize;

    let drops = (0..drop_cells).map(|_| random.cell(MAX_DROP)).collect();
    let cuts = (0..cut_cells).map(|_| random.cell(MAX_CUT)).collect();
    let grids = DistortionGrids::additive(width, height, drops, cuts)?;

    Ok(grids)
}
