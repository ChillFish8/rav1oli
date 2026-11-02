use rav1oli_table::{EncoderSettings as MapEncoderSettings, MapMetadata, SourceInfo};

use crate::commands::options::{AnalysisOptions, EncoderOptions, Metric};
use crate::metrics::{MetricConfig, metric_settings, select_display};
use crate::video::SourceVideo;

/// The metric and its settings selected by `options`, for a video of the `source` size.
pub fn metric_config(options: &AnalysisOptions, source: &SourceVideo) -> anyhow::Result<MetricConfig> {
    let quantisation = options.quantisation();

    match options.metric {
        Metric::Cvvdp => {
            let model = options.display_model.as_deref();
            let config = options.display_config.as_deref();
            let display = select_display(model, config, source.width, source.height)?;
            let numerator = source.frame_rate.numerator as f32;
            let denominator = source.frame_rate.denominator as f32;

            Ok(MetricConfig::Cvvdp {
                temporal: !options.no_temporal,
                fps: numerator / denominator,
                display,
                quantisation,
            })
        },
        Metric::Butteraugli => Ok(MetricConfig::Butteraugli {
            intensity_target: options.intensity_target(),
            quantisation,
        }),
    }
}

/// The source details stored in the distortion map.
pub fn source_info(source: &SourceVideo, hash: String) -> SourceInfo {
    SourceInfo {
        hash,
        width: source.width,
        height: source.height,
        frame_count: source.frame_count,
        frame_rate: source.frame_rate,
    }
}

/// The encoder settings stored in the distortion map.
pub fn encoder_info(options: &EncoderOptions) -> MapEncoderSettings {
    MapEncoderSettings {
        preset: options.preset,
        crf: options.crf,
        svt_params: options.svt_params_string(),
    }
}

/// The metadata stored in the distortion map.
pub fn map_metadata(
    source: &SourceVideo,
    hash: String,
    options: &EncoderOptions,
    config: &MetricConfig,
) -> MapMetadata {
    MapMetadata {
        source: source_info(source, hash),
        encoder: encoder_info(options),
        metric: metric_settings(config),
        quantisation: config.quantisation(),
    }
}
