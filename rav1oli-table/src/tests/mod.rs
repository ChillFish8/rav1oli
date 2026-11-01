mod roundtrip;

use crate::metadata::{EncoderSettings, FrameRate, MapMetadata, Metric, MetricSettings, SourceInfo};

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
    let metric = MetricSettings {
        metric: Metric::Cvvdp,
        temporal: true,
    };

    MapMetadata {
        source,
        encoder,
        metric,
    }
}
