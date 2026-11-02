use anyhow::{Context, bail};
use rav1oli_bestsource::Frame;
use rav1oli_quantise::block_scores;
use rav1oli_table::{FrameScores, MapSummary, Metric, MetricSettings, Quantisation};
use rav1oli_vship::{Butteraugli, Cvvdp, CvvdpOptions, DistortionMap};

use super::colour::{FrameDescription, PlaneBuffers, check_compatible, colour_space};
use super::display::SelectedDisplay;
use super::statistics::summarise;

pub const BUTTERAUGLI_QUANTISATION: Quantisation = Quantisation {
    cap: 8.0,
    floor: 0.5,
    top: 255,
};

pub const CVVDP_QUANTISATION: Quantisation = Quantisation {
    cap: 1.0,
    floor: 0.0,
    top: 255,
};

/// The Butteraugli norm reported as `norm_q`.
const BUTTERAUGLI_QNORM: i32 = 2;

/// The metric to score with and the settings it runs with.
#[derive(Debug, Clone)]
pub enum MetricConfig {
    Cvvdp {
        temporal: bool,
        fps: f32,
        display: SelectedDisplay,
        quantisation: Quantisation,
    },
    Butteraugli {
        intensity_target: f32,
        quantisation: Quantisation,
    },
}

impl MetricConfig {
    /// The scale block scores are quantised with.
    pub fn quantisation(&self) -> Quantisation {
        match self {
            Self::Cvvdp { quantisation, .. } | Self::Butteraugli { quantisation, .. } => *quantisation,
        }
    }
}

/// The settings to record in a map's metadata for `config`.
pub fn metric_settings(config: &MetricConfig) -> MetricSettings {
    match config {
        MetricConfig::Cvvdp {
            temporal, display, ..
        } => MetricSettings {
            metric: Metric::Cvvdp,
            temporal: *temporal,
            display: Some(display.settings.clone()),
            intensity_target: None,
        },
        MetricConfig::Butteraugli { intensity_target, .. } => MetricSettings {
            metric: Metric::Butteraugli,
            temporal: false,
            display: None,
            intensity_target: Some(*intensity_target),
        },
    }
}

/// The result of scoring one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredFrame {
    pub block_scores: Vec<u8>,
    pub frame_scores: FrameScores,
}

#[derive(Debug)]
enum Engine {
    Cvvdp {
        metric: Cvvdp,
        temporal: bool,
        scores: Vec<f64>,
    },
    Butteraugli {
        metric: Butteraugli,
        norm_q: Vec<f64>,
        norm3: Vec<f64>,
        norm_inf: Vec<f64>,
    },
}

impl Engine {
    fn frames_scored(&self) -> usize {
        match self {
            Self::Cvvdp { scores, .. } => scores.len(),
            Self::Butteraugli { norm_q, .. } => norm_q.len(),
        }
    }
}

/// Scores frame pairs on a GPU, keeping what is needed to summarise the whole video.
#[derive(Debug)]
pub struct MetricScorer {
    config: MetricConfig,
    engine: Engine,
    map: DistortionMap,
    reference_buffers: PlaneBuffers,
    distorted_buffers: PlaneBuffers,
}

impl MetricScorer {
    /// Prepares the metric for frames laid out like `reference` and `distorted`.
    pub fn new(config: MetricConfig, gpu: i32, reference: &Frame, distorted: &Frame) -> anyhow::Result<Self> {
        let context = format!("failed to check GPU {gpu}");
        rav1oli_vship::check_device(gpu).with_context(|| context)?;

        let reference_description = FrameDescription::from_frame(reference);
        let distorted_description = FrameDescription::from_frame(distorted);
        check_compatible(&reference_description, &distorted_description)?;

        if let MetricConfig::Cvvdp { display, .. } = &config {
            check_display_size(display, &reference_description)?;
        }

        let reference_space = colour_space(&reference_description)?;
        let distorted_space = colour_space(&distorted_description)?;
        let (engine, map) = match &config {
            MetricConfig::Cvvdp {
                temporal,
                fps,
                display,
                ..
            } => {
                let options = CvvdpOptions {
                    fps: *fps,
                    resize_to_display: false,
                    display: display.model.clone(),
                };
                let context = format!("failed to set up CVVDP on GPU {gpu}");
                let metric =
                    Cvvdp::new(reference_space, distorted_space, &options, gpu).with_context(|| context)?;
                let map = metric.distortion_map();
                let engine = Engine::Cvvdp {
                    metric,
                    temporal: *temporal,
                    scores: Vec::new(),
                };
                (engine, map)
            },
            MetricConfig::Butteraugli { intensity_target, .. } => {
                let context = format!("failed to set up Butteraugli on GPU {gpu}");
                let metric = Butteraugli::new(
                    reference_space,
                    distorted_space,
                    BUTTERAUGLI_QNORM,
                    *intensity_target,
                    gpu,
                )
                .with_context(|| context)?;
                let map = metric.distortion_map();
                let engine = Engine::Butteraugli {
                    metric,
                    norm_q: Vec::new(),
                    norm3: Vec::new(),
                    norm_inf: Vec::new(),
                };
                (engine, map)
            },
        };

        let reference_buffers = PlaneBuffers::new(&reference_description)
            .context("failed to allocate the reference frame buffers")?;
        let distorted_buffers = PlaneBuffers::new(&distorted_description)
            .context("failed to allocate the distorted frame buffers")?;

        Ok(Self {
            config,
            engine,
            map,
            reference_buffers,
            distorted_buffers,
        })
    }

    pub fn settings(&self) -> MetricSettings {
        metric_settings(&self.config)
    }

    pub fn quantisation(&self) -> Quantisation {
        self.config.quantisation()
    }

    /// Scores one frame pair and quantises the distortion map into block scores.
    pub fn score(&mut self, reference: &Frame, distorted: &Frame) -> anyhow::Result<ScoredFrame> {
        let frame_index = self.engine.frames_scored();

        let context = || format!("failed to export reference frame {frame_index}");
        self.reference_buffers.export(reference).with_context(context)?;
        let context = || format!("failed to export distorted frame {frame_index}");
        self.distorted_buffers.export(distorted).with_context(context)?;

        let reference_planes = self.reference_buffers.planes();
        let distorted_planes = self.distorted_buffers.planes();
        let frame_scores = match &mut self.engine {
            Engine::Cvvdp {
                metric,
                temporal,
                scores,
            } => {
                if !*temporal {
                    let context = || format!("failed to reset the CVVDP history before frame {frame_index}");
                    metric.reset().with_context(context)?;
                }

                let context = || format!("failed to score frame {frame_index} with CVVDP");
                let score = metric
                    .compute_with_map(&reference_planes, &distorted_planes, &mut self.map)
                    .with_context(context)?;
                scores.push(score);
                FrameScores::Cvvdp { score }
            },
            Engine::Butteraugli {
                metric,
                norm_q,
                norm3,
                norm_inf,
            } => {
                let context = || format!("failed to score frame {frame_index} with Butteraugli");
                let score = metric
                    .compute_with_map(&reference_planes, &distorted_planes, &mut self.map)
                    .with_context(context)?;
                norm_q.push(score.norm_q);
                norm3.push(score.norm3);
                norm_inf.push(score.norm_inf);
                FrameScores::Butteraugli {
                    norm_q: score.norm_q,
                    norm3: score.norm3,
                    norm_inf: score.norm_inf,
                }
            },
        };

        let quantisation = self.config.quantisation();
        let blocks = block_scores(
            self.map.data(),
            self.map.width(),
            self.map.height(),
            &quantisation,
        );

        Ok(ScoredFrame {
            block_scores: blocks,
            frame_scores,
        })
    }

    /// Summarises every frame scored so far.
    pub fn finish(self) -> anyhow::Result<MapSummary> {
        match self.engine {
            Engine::Cvvdp {
                temporal: true,
                scores,
                ..
            } => {
                let Some(score) = scores.last().copied() else {
                    bail!("no frames were scored");
                };

                Ok(MapSummary::CvvdpTemporal { score })
            },
            Engine::Cvvdp { scores, .. } => {
                let statistics = summarise(&scores).context("failed to summarise the CVVDP scores")?;

                Ok(MapSummary::CvvdpPerFrame(statistics))
            },
            Engine::Butteraugli {
                norm_q,
                norm3,
                norm_inf,
                ..
            } => {
                let norm_q = summarise(&norm_q).context("failed to summarise the norm_q scores")?;
                let norm3 = summarise(&norm3).context("failed to summarise the norm3 scores")?;
                let norm_inf = summarise(&norm_inf).context("failed to summarise the norm_inf scores")?;

                Ok(MapSummary::Butteraugli {
                    norm_q,
                    norm3,
                    norm_inf,
                })
            },
        }
    }
}

fn check_display_size(display: &SelectedDisplay, video: &FrameDescription) -> anyhow::Result<()> {
    let display_size = (i64::from(display.width), i64::from(display.height));
    if display_size == (video.width, video.height) {
        return Ok(());
    }

    let name = &display.settings.model;
    let (display_width, display_height) = (display.width, display.height);
    let (video_width, video_height) = (video.width, video.height);
    let message = format!(
        "display model `{name}` is {display_width}x{display_height} \
         but the video is {video_width}x{video_height}"
    );

    bail!(message);
}
