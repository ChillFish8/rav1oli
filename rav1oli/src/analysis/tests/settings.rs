use rav1oli_table::{FrameRate, Metric as MapMetric};

use crate::analysis::{map_metadata, metric_config};
use crate::commands::options::{AnalysisOptions, EncoderOptions, Metric};
use crate::metrics::{MetricConfig, metric_settings};
use crate::video::SourceVideo;

fn source(width: u32, height: u32) -> SourceVideo {
    SourceVideo {
        width,
        height,
        bit_depth: 10,
        frame_count: 48,
        frame_rate: FrameRate {
            numerator: 24000,
            denominator: 1001,
        },
    }
}

fn analysis_options(metric: Metric) -> AnalysisOptions {
    AnalysisOptions {
        metric,
        no_temporal: false,
        gpu: 0,
        display_model: None,
        display_config: None,
        intensity_target: None,
        floor: None,
    }
}

fn encoder_options(svt_args: &[&str]) -> EncoderOptions {
    EncoderOptions {
        preset: 6,
        crf: 28,
        svt_args: svt_args.iter().map(|arg| arg.to_string()).collect(),
    }
}

#[test]
fn butteraugli_config_uses_intensity_and_floor() {
    let options = AnalysisOptions {
        floor: Some(1.0),
        ..analysis_options(Metric::Butteraugli)
    };

    let config = metric_config(&options, &source(64, 64)).unwrap();

    let MetricConfig::Butteraugli {
        intensity_target,
        quantisation,
    } = config
    else {
        panic!("expected a Butteraugli config");
    };
    assert_eq!(intensity_target, 203.0);
    assert_eq!(quantisation.floor, 1.0);
}

#[test]
fn cvvdp_config_selects_the_display_for_the_source_size() {
    let options = analysis_options(Metric::Cvvdp);

    let config = metric_config(&options, &source(1920, 1080)).unwrap();

    let MetricConfig::Cvvdp {
        temporal,
        fps,
        display,
        ..
    } = config
    else {
        panic!("expected a CVVDP config");
    };
    assert!(temporal);
    assert!((fps - 24000.0 / 1001.0).abs() < 1e-3);
    assert_eq!(display.settings.model, "trix_fhd");

    let options = AnalysisOptions {
        no_temporal: true,
        ..analysis_options(Metric::Cvvdp)
    };

    let config = metric_config(&options, &source(1920, 1080)).unwrap();

    let MetricConfig::Cvvdp { temporal, .. } = config else {
        panic!("expected a CVVDP config");
    };
    assert!(!temporal);
}

#[test]
fn cvvdp_config_without_a_default_display_is_an_error() {
    let options = analysis_options(Metric::Cvvdp);

    assert!(metric_config(&options, &source(640, 480)).is_err());
}

#[test]
fn map_metadata_records_source_encoder_and_metric() {
    let source = source(64, 62);
    let options = encoder_options(&["--tune", "0"]);
    let analysis = analysis_options(Metric::Butteraugli);
    let config = metric_config(&analysis, &source).unwrap();

    let metadata = map_metadata(&source, "abc123".to_owned(), &options, &config);

    assert_eq!(metadata.source.hash, "abc123");
    assert_eq!((metadata.source.width, metadata.source.height), (64, 62));
    assert_eq!(metadata.source.frame_count, 48);
    assert_eq!(metadata.source.frame_rate, source.frame_rate);
    assert_eq!(metadata.encoder.preset, 6);
    assert_eq!(metadata.encoder.crf, 28);
    assert_eq!(metadata.encoder.svt_params.as_deref(), Some("--tune 0"));
    assert_eq!(metadata.metric, metric_settings(&config));
    assert_eq!(metadata.metric.metric, MapMetric::Butteraugli);
    assert_eq!(metadata.quantisation, config.quantisation());
}
