mod metadata;
mod roundtrip;

use crate::metadata::{
    DisplaySettings,
    EncoderSettings,
    FrameRate,
    MapMetadata,
    Metric,
    MetricSettings,
    Quantisation,
    SourceInfo,
};

pub(crate) fn sample_metadata() -> MapMetadata {
    let source = SourceInfo {
        hash: "abc123".to_string(),
        width: 1920,
        height: 1080,
        frame_count: 3,
        frame_rate: FrameRate {
            numerator: 24000,
            denominator: 1001,
        },
    };
    let encoder = EncoderSettings {
        preset: 4,
        crf: 30,
        svt_params: None,
    };
    let display = DisplaySettings {
        model: "trix_fhd".to_string(),
        config: None,
    };
    let quantisation = Quantisation {
        cap: 1.0,
        floor: 0.0,
        top: 255,
    };
    let metric = MetricSettings {
        metric: Metric::Cvvdp,
        temporal: true,
        display: Some(display),
        intensity_target: None,
        quantisation,
    };

    MapMetadata {
        source,
        encoder,
        metric,
    }
}
