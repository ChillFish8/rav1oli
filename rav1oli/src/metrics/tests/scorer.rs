use rav1oli_table::{DisplaySettings, Metric, Quantisation};
use rav1oli_vship::DisplayModel;

use crate::metrics::{
    BUTTERAUGLI_QUANTISATION,
    CVVDP_QUANTISATION,
    MetricConfig,
    SelectedDisplay,
    metric_settings,
};

fn trix_display() -> SelectedDisplay {
    SelectedDisplay {
        model: DisplayModel::TrixFhd,
        width: 1920,
        height: 1080,
        settings: DisplaySettings {
            model: "trix_fhd".to_string(),
            config: None,
        },
    }
}

#[test]
fn settings_record_display_intensity_and_quantisation() {
    let cvvdp = MetricConfig::Cvvdp {
        temporal: true,
        fps: 25.0,
        display: trix_display(),
        quantisation: CVVDP_QUANTISATION,
    };
    let butteraugli = MetricConfig::Butteraugli {
        intensity_target: 203.0,
        quantisation: BUTTERAUGLI_QUANTISATION,
    };

    let cvvdp_settings = metric_settings(&cvvdp);
    let butteraugli_settings = metric_settings(&butteraugli);

    assert_eq!(cvvdp_settings.metric, Metric::Cvvdp);
    assert!(cvvdp_settings.temporal);
    assert_eq!(cvvdp_settings.display, Some(trix_display().settings));
    assert_eq!(cvvdp_settings.intensity_target, None);
    assert_eq!(cvvdp.quantisation(), CVVDP_QUANTISATION);

    assert_eq!(butteraugli_settings.metric, Metric::Butteraugli);
    assert_eq!(butteraugli_settings.display, None);
    assert_eq!(butteraugli_settings.intensity_target, Some(203.0));
    assert_eq!(butteraugli.quantisation(), BUTTERAUGLI_QUANTISATION);
}

#[test]
fn a_custom_floor_is_used_for_the_map_quantisation() {
    let quantisation = Quantisation {
        floor: 1.0,
        ..BUTTERAUGLI_QUANTISATION
    };
    let config = MetricConfig::Butteraugli {
        intensity_target: 203.0,
        quantisation,
    };

    let used = config.quantisation();

    assert_eq!(used.floor, 1.0);
    assert_eq!(used.cap, 8.0);
}

#[cfg(feature = "gpu-tests")]
mod gpu {
    use std::path::Path;

    use rav1oli_bestsource::Frame;
    use rav1oli_table::{FrameScores, MapSummary};

    use super::*;
    use crate::decode::tests::copy_fixture;
    use crate::decode::{CacheLocation, Decoder};
    use crate::metrics::tests::gpu_id;
    use crate::metrics::{MetricScorer, select_display};

    fn fixture_frames() -> Vec<Frame> {
        let directory = tempfile::tempdir().unwrap();
        let video = copy_fixture(directory.path());
        let mut decoder = Decoder::open(&video, &CacheLocation::BesideVideo).unwrap();

        decoder.frames().map(|frame| frame.unwrap()).collect()
    }

    fn small_display(directory: &Path) -> SelectedDisplay {
        let config = r#"{
            "small_test": {
                "name": "64x64 test display",
                "colorspace": "SDR",
                "resolution": [64, 64],
                "diagonal_size_inches": 27,
                "viewing_distance_meters": 0.5,
                "contrast": 2000,
                "max_luminance": 203,
                "E_ambient": 20
            }
        }"#;
        let path = directory.join("displays.json");
        std::fs::write(&path, config).unwrap();

        select_display(Some("small_test"), Some(&path), 64, 64).unwrap()
    }

    fn cvvdp_config(temporal: bool, display: SelectedDisplay) -> MetricConfig {
        MetricConfig::Cvvdp {
            temporal,
            fps: 25.0,
            display,
            quantisation: CVVDP_QUANTISATION,
        }
    }

    #[test]
    fn identical_frames_score_as_undistorted_with_butteraugli() {
        let frames = fixture_frames();
        let config = MetricConfig::Butteraugli {
            intensity_target: 203.0,
            quantisation: BUTTERAUGLI_QUANTISATION,
        };
        let gpu = gpu_id();
        let mut scorer = MetricScorer::new(config, gpu, &frames[0], &frames[0]).unwrap();
        let settings = scorer.settings();
        let quantisation = scorer.quantisation();

        let scored = scorer.score(&frames[0], &frames[0]).unwrap();

        assert_eq!(settings.metric, Metric::Butteraugli);
        assert_eq!(quantisation, BUTTERAUGLI_QUANTISATION);
        assert_eq!(scored.block_scores.len(), 8 * 8);
        assert!(scored.block_scores.iter().all(|score| *score == 0));

        let FrameScores::Butteraugli { norm_q, .. } = scored.frame_scores else {
            panic!("expected butteraugli scores");
        };

        assert!(norm_q.abs() < 1e-6, "{norm_q}");
    }

    #[test]
    fn cvvdp_rolling_score_is_reported_per_frame() {
        let frames = fixture_frames();
        let directory = tempfile::tempdir().unwrap();
        let display = small_display(directory.path());
        let config = cvvdp_config(true, display);
        let gpu = gpu_id();
        let mut scorer = MetricScorer::new(config, gpu, &frames[0], &frames[0]).unwrap();

        let mut last = 0.0;
        for frame in &frames {
            let scored = scorer.score(frame, frame).unwrap();

            let FrameScores::Cvvdp { score } = scored.frame_scores else {
                panic!("expected cvvdp scores");
            };

            last = score;
        }

        let summary = scorer.finish().unwrap();

        assert_eq!(summary, MapSummary::CvvdpTemporal { score: last });
    }

    #[test]
    fn cvvdp_without_temporal_summarises_per_frame_scores() {
        let frames = fixture_frames();
        let directory = tempfile::tempdir().unwrap();
        let display = small_display(directory.path());
        let config = cvvdp_config(false, display.clone());
        let gpu = gpu_id();
        let mut scorer = MetricScorer::new(config, gpu, &frames[0], &frames[1]).unwrap();

        let mut scores = Vec::new();
        for pair in frames.windows(2) {
            let scored = scorer.score(&pair[0], &pair[1]).unwrap();

            let FrameScores::Cvvdp { score } = scored.frame_scores else {
                panic!("expected cvvdp scores");
            };

            scores.push(score);
        }

        let summary = scorer.finish().unwrap();
        assert!(matches!(summary, MapSummary::CvvdpPerFrame(_)), "{summary:?}");

        for (pair, score) in frames.windows(2).zip(scores) {
            let config = cvvdp_config(false, display.clone());
            let gpu = gpu_id();
            let mut fresh = MetricScorer::new(config, gpu, &pair[0], &pair[1]).unwrap();
            let scored = fresh.score(&pair[0], &pair[1]).unwrap();

            let FrameScores::Cvvdp { score: expected } = scored.frame_scores else {
                panic!("expected cvvdp scores");
            };

            assert!((score - expected).abs() < 1e-6, "{score} vs {expected}");
        }
    }

    #[test]
    fn a_display_that_does_not_match_the_video_is_an_error() {
        let frames = fixture_frames();
        let config = cvvdp_config(true, trix_display());

        let gpu = gpu_id();
        let error = MetricScorer::new(config, gpu, &frames[0], &frames[0]).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("1920x1080"), "{message}");
        assert!(message.contains("64x64"), "{message}");
    }
}
