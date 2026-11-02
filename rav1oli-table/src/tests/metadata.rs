use rav1oli_quantise::block_scores;

use super::sample_metadata;
use crate::metadata::DisplaySettings;
use crate::{FrameRate, MapSummary, Quantisation, SourceInfo, Statistics};

fn sample_statistics(offset: f64) -> Statistics {
    Statistics {
        average: 1.0 + offset,
        standard_deviation: 0.5 + offset,
        median: 0.9 + offset,
        percentile_5: 0.1 + offset,
        percentile_95: 2.0 + offset,
        minimum: offset,
        maximum: 3.0 + offset,
    }
}

fn custom_display(config: &str) -> Option<DisplaySettings> {
    Some(DisplaySettings {
        model: "custom".to_string(),
        config: Some(config.to_string()),
    })
}

#[test]
fn a_different_display_model_is_reported() {
    let stored = sample_metadata();
    let mut current = stored.clone();
    current.metric.display = Some(DisplaySettings {
        model: "standard_4k".to_string(),
        config: None,
    });

    let mismatches = stored.check_against(&current).unwrap();

    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].setting, "display model");
}

#[test]
fn a_different_custom_config_is_reported_without_dumping_json() {
    let mut stored = sample_metadata();
    let mut current = stored.clone();
    stored.metric.display = custom_display("{\"a\":1}");
    current.metric.display = custom_display("{\"b\":2}");

    let mismatches = stored.check_against(&current).unwrap();

    assert_eq!(mismatches.len(), 1);
    let mismatch = &mismatches[0];
    assert_eq!(mismatch.setting, "display config");
    assert_eq!(mismatch.stored, "custom");
    assert_eq!(mismatch.current, "a different custom");
    assert!(!mismatch.stored.contains('{'));
    assert!(!mismatch.current.contains('{'));
}

#[test]
fn a_custom_config_against_none_is_reported() {
    let stored = sample_metadata();
    let mut current = stored.clone();
    current.metric.display = Some(DisplaySettings {
        model: "trix_fhd".to_string(),
        config: Some("{}".to_string()),
    });

    let mismatches = stored.check_against(&current).unwrap();

    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].stored, "none");
    assert_eq!(mismatches[0].current, "custom");
}

#[test]
fn a_different_intensity_target_is_reported() {
    let mut stored = sample_metadata();
    let mut current = stored.clone();
    stored.metric.intensity_target = Some(203.0);
    current.metric.intensity_target = Some(400.0);

    let mismatches = stored.check_against(&current).unwrap();

    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].setting, "intensity target");
    assert_eq!(mismatches[0].stored, "203");
    assert_eq!(mismatches[0].current, "400");
}

#[test]
fn different_quantisation_is_reported() {
    let mut stored = sample_metadata();
    let mut current = stored.clone();
    stored.quantisation.cap = 8.0;
    current.quantisation.cap = 1.0;

    let mismatches = stored.check_against(&current).unwrap();

    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].setting, "quantisation");
}

#[test]
fn summary_json_names_the_variant() {
    let summary = MapSummary::CvvdpTemporal { score: 0.25 };

    let json = serde_json::to_string(&summary).unwrap();

    assert!(json.contains("\"kind\":\"cvvdp_temporal\""));
}

#[test]
fn summary_round_trips_through_json() {
    let summaries = vec![
        MapSummary::CvvdpTemporal { score: 0.25 },
        MapSummary::CvvdpPerFrame(sample_statistics(0.0)),
        MapSummary::Butteraugli {
            norm_q: sample_statistics(0.0),
            norm3: sample_statistics(1.0),
            norm_inf: sample_statistics(2.0),
        },
    ];

    for summary in summaries {
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: MapSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, summary);
    }
}

#[test]
fn dequantise_inverts_the_scale() {
    let quantisation = Quantisation {
        cap: 8.0,
        floor: 0.5,
        top: 255,
    };

    let saturated = quantisation.dequantise(255);
    let zero = quantisation.dequantise(0);
    let middle = quantisation.dequantise(16);

    assert_eq!(saturated, 8.0);
    assert_eq!(zero, 0.0);
    assert_eq!(middle, 16.0 / 255.0 * 8.0);
}

#[test]
fn grid_matches_the_table_layout() {
    let quantisation = Quantisation {
        cap: 8.0,
        floor: 0.5,
        top: 255,
    };
    let (width, height) = (1918_usize, 1080_usize);
    let values = vec![0.0_f32; width * height];
    let info = SourceInfo {
        hash: String::new(),
        width: width as u32,
        height: height as u32,
        frame_count: 1,
        frame_rate: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    };

    let codes = block_scores(&values, width, height, &quantisation);

    assert_eq!(codes.len(), info.blocks_per_frame());
}
