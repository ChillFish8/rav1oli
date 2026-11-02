mod encode;
mod grids;

use std::path::{Path, PathBuf};

use rav1oli_table::{FrameScores, MapSummary, MapWriter, Statistics, hash_source};

use crate::analysis::map_metadata;
use crate::commands::options::EncoderOptions;
use crate::decode::Decoder;
use crate::metrics::{BUTTERAUGLI_QUANTISATION, MetricConfig};
use crate::video::describe_source;

pub(crate) fn encoder_options(crf: u8) -> EncoderOptions {
    EncoderOptions {
        preset: 12,
        crf,
        svt_args: Vec::new(),
    }
}

fn statistics(value: f64) -> Statistics {
    Statistics {
        average: value,
        standard_deviation: 0.0,
        median: value,
        percentile_5: value,
        percentile_95: value,
        minimum: value,
        maximum: value,
    }
}

/// Writes a Butteraugli map for `video` at `map` without scoring, with scores rising each frame.
pub(crate) fn write_synthetic_map(video: &Path, map: &Path, crf: u8) -> PathBuf {
    write_map(video, map, crf, |frame, block| {
        (frame * 20 + block * 2).min(255) as u8
    })
}

/// Writes a Butteraugli map for `video` at `map` where every block scores `score`.
pub(crate) fn write_constant_map(video: &Path, map: &Path, crf: u8, score: u8) -> PathBuf {
    write_map(video, map, crf, |_, _| score)
}

fn write_map(video: &Path, map: &Path, crf: u8, score_for: impl Fn(usize, usize) -> u8) -> PathBuf {
    let decoder = Decoder::open_uncached(video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let hash = hash_source(video).unwrap();
    let config = MetricConfig::Butteraugli {
        intensity_target: 203.0,
        quantisation: BUTTERAUGLI_QUANTISATION,
    };
    let options = encoder_options(crf);
    let metadata = map_metadata(&source, hash, &options, &config);

    let blocks = metadata.source.blocks_per_frame();
    let mut writer = MapWriter::create(map, &metadata).unwrap();
    for frame in 0..source.frame_count {
        let scores: Vec<u8> = (0..blocks)
            .map(|block| score_for(frame as usize, block))
            .collect();
        let value = f64::from(frame) + 1.0;
        let frame_scores = FrameScores::Butteraugli {
            norm_q: value,
            norm3: value,
            norm_inf: value,
        };
        writer.write_frame(&scores, frame_scores).unwrap();
    }

    let summary = MapSummary::Butteraugli {
        norm_q: statistics(1.0),
        norm3: statistics(1.0),
        norm_inf: statistics(1.0),
    };
    writer.finish(&summary).unwrap();

    map.to_path_buf()
}
