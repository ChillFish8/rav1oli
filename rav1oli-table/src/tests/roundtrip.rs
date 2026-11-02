use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{ArrayRef, BinaryArray, Float64Array, RecordBatch, UInt32Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

use crate::metadata::METADATA_KEY;
use crate::tests::sample_metadata;
use crate::{
    FrameScores,
    MapMetadata,
    MapReader,
    MapSummary,
    MapWriter,
    Metric,
    Quantisation,
    Statistics,
    TableError,
};

fn cvvdp_scores(seed: u32) -> FrameScores {
    FrameScores::Cvvdp {
        score: 9.9 - f64::from(seed) * 0.1,
    }
}

fn cvvdp_summary() -> MapSummary {
    MapSummary::CvvdpTemporal { score: 9.7 }
}

fn sample_statistics(offset: f64) -> Statistics {
    Statistics {
        average: 1.0 + offset,
        standard_deviation: 2.0 + offset,
        median: 3.0 + offset,
        percentile_5: 4.0 + offset,
        percentile_95: 5.0 + offset,
        minimum: 6.0 + offset,
        maximum: 7.0 + offset,
    }
}

fn butteraugli_metadata() -> MapMetadata {
    let mut metadata = sample_metadata();
    metadata.metric.metric = Metric::Butteraugli;
    metadata.metric.temporal = false;
    metadata
}

fn butteraugli_scores(seed: u32) -> FrameScores {
    let base = f64::from(seed);
    FrameScores::Butteraugli {
        norm_q: base + 0.1,
        norm3: base + 0.2,
        norm_inf: base + 0.3,
    }
}

fn frame_scores(metadata: &MapMetadata, seed: u8) -> Vec<u8> {
    let blocks = metadata.source.blocks_per_frame();
    (0..blocks).map(|i| (i as u8).wrapping_add(seed)).collect()
}

fn write_map(path: &Path, metadata: &MapMetadata) {
    let mut writer = MapWriter::create(path, metadata).unwrap();

    for seed in 0..metadata.source.frame_count {
        let scores = frame_scores(metadata, seed as u8);
        writer.write_frame(&scores, cvvdp_scores(seed)).unwrap();
    }

    let summary = cvvdp_summary();
    writer.finish(&summary).unwrap();
}

fn write_plain_parquet(path: &Path, footer: Option<Vec<KeyValue>>) {
    let field = Field::new("frame_index", DataType::UInt32, false);
    let schema = Arc::new(Schema::new(vec![field]));
    let column = Arc::new(UInt32Array::from(vec![0]));
    let batch = RecordBatch::try_new(schema.clone(), vec![column]).unwrap();

    let properties = WriterProperties::builder().set_key_value_metadata(footer).build();
    let file = File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, Some(properties)).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
}

#[test]
fn frames_read_back_in_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    write_map(&path, &metadata);

    let reader = MapReader::open(&path).unwrap();
    let frames: Vec<_> = reader.collect::<Result<_, _>>().unwrap();

    assert_eq!(frames.len(), 3);
    for (seed, frame) in frames.iter().enumerate() {
        let expected = frame_scores(&metadata, seed as u8);
        assert_eq!(frame.index(), seed as u32);
        assert_eq!(frame.scores(), expected.as_slice());
    }
}

#[test]
fn metadata_reads_back() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    write_map(&path, &metadata);

    let reader = MapReader::open(&path).unwrap();

    assert_eq!(reader.metadata(), &metadata);
}

#[test]
fn frame_score_indexes_by_block_position() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    write_map(&path, &metadata);

    let mut reader = MapReader::open(&path).unwrap();
    let frame = reader.next().unwrap().unwrap();

    let grid_width = metadata.source.grid_width() as usize;
    let expected = frame.scores()[2 * grid_width + 5];
    assert_eq!(frame.score(5, 2), expected);
}

#[test]
fn write_frame_rejects_wrong_size() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    let mut writer = MapWriter::create(&path, &metadata).unwrap();

    let result = writer.write_frame(&[0; 10], FrameScores::Cvvdp { score: 9.9 });

    let error = result.unwrap_err();
    assert!(matches!(error, TableError::FrameSizeMismatch { actual: 10, .. }));
}

#[test]
fn write_frame_rejects_extra_frames() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    let scores = frame_scores(&metadata, 0);
    let frame_scores = FrameScores::Cvvdp { score: 9.9 };
    let mut writer = MapWriter::create(&path, &metadata).unwrap();
    for _ in 0..metadata.source.frame_count {
        writer.write_frame(&scores, frame_scores).unwrap();
    }

    let result = writer.write_frame(&scores, frame_scores);

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        TableError::FrameCountMismatch {
            expected: 3,
            actual: 4
        }
    ));
}

#[test]
fn finish_rejects_missing_frames() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    let scores = frame_scores(&metadata, 0);
    let mut writer = MapWriter::create(&path, &metadata).unwrap();
    writer
        .write_frame(&scores, FrameScores::Cvvdp { score: 9.9 })
        .unwrap();

    let summary = cvvdp_summary();
    let result = writer.finish(&summary);

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        TableError::FrameCountMismatch {
            expected: 3,
            actual: 1
        }
    ));
}

#[test]
fn open_rejects_file_without_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plain.parquet");
    write_plain_parquet(&path, None);

    let result = MapReader::open(&path);

    assert!(matches!(result, Err(TableError::MissingMetadata)));
}

#[test]
fn open_rejects_unexpected_columns() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plain.parquet");
    let summary = cvvdp_summary();
    let json = sample_metadata().to_json(&summary).unwrap();
    let footer = KeyValue::new(METADATA_KEY.to_string(), json);
    write_plain_parquet(&path, Some(vec![footer]));

    let result = MapReader::open(&path);

    assert!(matches!(result, Err(TableError::InvalidLayout(_))));
}

#[test]
fn cvvdp_frame_scores_and_summary_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    write_map(&path, &metadata);

    let reader = MapReader::open(&path).unwrap();
    let summary = reader.summary().clone();
    let frames: Vec<_> = reader.collect::<Result<_, _>>().unwrap();

    let read_scores: Vec<FrameScores> = frames.iter().map(|frame| frame.frame_scores()).collect();
    let expected: Vec<FrameScores> = (0..3).map(cvvdp_scores).collect();
    assert_eq!(read_scores, expected);
    assert_eq!(summary, cvvdp_summary());
}

#[test]
fn butteraugli_frame_scores_and_summary_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = butteraugli_metadata();
    let summary = MapSummary::Butteraugli {
        norm_q: sample_statistics(0.0),
        norm3: sample_statistics(1.0),
        norm_inf: sample_statistics(2.0),
    };
    let mut writer = MapWriter::create(&path, &metadata).unwrap();

    for seed in 0..metadata.source.frame_count {
        let scores = frame_scores(&metadata, seed as u8);
        writer.write_frame(&scores, butteraugli_scores(seed)).unwrap();
    }

    writer.finish(&summary).unwrap();

    let reader = MapReader::open(&path).unwrap();
    let read_summary = reader.summary().clone();
    let frames: Vec<_> = reader.collect::<Result<_, _>>().unwrap();

    let read_scores: Vec<FrameScores> = frames.iter().map(|frame| frame.frame_scores()).collect();
    let expected: Vec<FrameScores> = (0..3).map(butteraugli_scores).collect();
    assert_eq!(read_scores, expected);
    assert_eq!(read_summary, summary);
}

#[test]
fn writing_the_wrong_frame_scores_variant_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    let scores = frame_scores(&metadata, 0);
    let mut writer = MapWriter::create(&path, &metadata).unwrap();

    let result = writer.write_frame(&scores, butteraugli_scores(0));

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        TableError::FrameScoresMismatch {
            expected: Metric::Cvvdp
        }
    ));
}

#[test]
fn finishing_with_the_wrong_summary_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let mut metadata = sample_metadata();
    metadata.metric.temporal = false;
    let mut writer = MapWriter::create(&path, &metadata).unwrap();

    for seed in 0..metadata.source.frame_count {
        let scores = frame_scores(&metadata, seed as u8);
        writer.write_frame(&scores, cvvdp_scores(seed)).unwrap();
    }

    let summary = cvvdp_summary();
    let result = writer.finish(&summary);

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        TableError::SummaryMismatch {
            expected: Metric::Cvvdp
        }
    ));
}

#[test]
fn a_map_whose_writer_was_dropped_cannot_be_opened() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    let scores = frame_scores(&metadata, 0);
    let mut writer = MapWriter::create(&path, &metadata).unwrap();
    writer.write_frame(&scores, cvvdp_scores(0)).unwrap();
    drop(writer);

    let result = MapReader::open(&path);

    assert!(matches!(result, Err(TableError::Parquet(_))));
}

#[test]
fn metric_columns_must_match_the_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shaped.parquet");
    let metadata = butteraugli_metadata();
    let summary = cvvdp_summary();
    let json = metadata.to_json(&summary).unwrap();
    let footer = KeyValue::new(METADATA_KEY.to_string(), json);
    let fields = vec![
        Field::new("frame_index", DataType::UInt32, false),
        Field::new("scores", DataType::Binary, false),
        Field::new("score", DataType::Float64, false),
    ];
    let schema = Arc::new(Schema::new(fields));
    let columns: Vec<ArrayRef> = vec![
        Arc::new(UInt32Array::from(vec![0])),
        Arc::new(BinaryArray::from(vec![b"a".as_slice()])),
        Arc::new(Float64Array::from(vec![9.9])),
    ];
    let batch = RecordBatch::try_new(schema.clone(), columns).unwrap();
    let properties = WriterProperties::builder()
        .set_key_value_metadata(Some(vec![footer]))
        .build();
    let file = File::create(&path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, Some(properties)).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();

    let result = MapReader::open(&path);

    assert!(matches!(result, Err(TableError::InvalidLayout(_))));
}

#[test]
fn every_mismatched_summary_is_an_error() {
    let temporal = MapSummary::CvvdpTemporal { score: 9.7 };
    let per_frame = MapSummary::CvvdpPerFrame(sample_statistics(0.0));
    let butteraugli = MapSummary::Butteraugli {
        norm_q: sample_statistics(0.0),
        norm3: sample_statistics(1.0),
        norm_inf: sample_statistics(2.0),
    };
    let cases = [
        (Metric::Cvvdp, true, per_frame.clone()),
        (Metric::Cvvdp, true, butteraugli.clone()),
        (Metric::Cvvdp, false, temporal.clone()),
        (Metric::Cvvdp, false, butteraugli),
        (Metric::Butteraugli, false, temporal),
        (Metric::Butteraugli, false, per_frame),
    ];

    for (metric, is_temporal, summary) in cases {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("map.parquet");
        let mut metadata = sample_metadata();
        metadata.metric.metric = metric;
        metadata.metric.temporal = is_temporal;
        let mut writer = MapWriter::create(&path, &metadata).unwrap();

        for seed in 0..metadata.source.frame_count {
            let scores = frame_scores(&metadata, seed as u8);
            let frame_scores = match metric {
                Metric::Cvvdp => cvvdp_scores(seed),
                Metric::Butteraugli => butteraugli_scores(seed),
            };
            writer.write_frame(&scores, frame_scores).unwrap();
        }

        let result = writer.finish(&summary);

        let error = result.unwrap_err();
        assert!(matches!(error, TableError::SummaryMismatch { .. }));
    }
}

#[test]
fn the_reader_exposes_the_stored_quantisation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let mut metadata = sample_metadata();
    metadata.quantisation = Quantisation {
        cap: 8.0,
        floor: 0.5,
        top: 255,
    };
    write_map(&path, &metadata);

    let reader = MapReader::open(&path).unwrap();

    assert_eq!(reader.quantisation(), metadata.quantisation);
}

#[test]
fn frames_dequantise_with_the_stored_quantisation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let mut metadata = sample_metadata();
    metadata.quantisation = Quantisation {
        cap: 4.0,
        floor: 0.0,
        top: 255,
    };
    let mut scores = vec![0; metadata.source.blocks_per_frame()];
    scores[0] = 255;
    let mut writer = MapWriter::create(&path, &metadata).unwrap();

    for seed in 0..metadata.source.frame_count {
        writer.write_frame(&scores, cvvdp_scores(seed)).unwrap();
    }

    let summary = cvvdp_summary();
    writer.finish(&summary).unwrap();

    let mut reader = MapReader::open(&path).unwrap();
    let frame = reader.next().unwrap().unwrap();

    assert_eq!(frame.value(0, 0), 4.0);
    assert_eq!(frame.value(1, 0), 0.0);
}

#[test]
fn opening_a_map_with_zero_top_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("zero_top.parquet");
    let mut metadata = sample_metadata();
    metadata.quantisation.top = 0;
    let summary = cvvdp_summary();
    let json = metadata.to_json(&summary).unwrap();
    let footer = KeyValue::new(METADATA_KEY.to_string(), json);
    write_plain_parquet(&path, Some(vec![footer]));

    let result = MapReader::open(&path);

    assert!(matches!(result, Err(TableError::InvalidQuantisation(_))));
}

#[test]
fn creating_a_map_with_an_unusable_quantisation_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unusable.parquet");
    let usable = sample_metadata().quantisation;
    let zero_cap = Quantisation { cap: 0.0, ..usable };
    let nan_cap = Quantisation {
        cap: f32::NAN,
        ..usable
    };
    let floor_at_cap = Quantisation {
        floor: usable.cap,
        ..usable
    };
    let zero_top = Quantisation { top: 0, ..usable };

    for quantisation in [zero_cap, nan_cap, floor_at_cap, zero_top] {
        let mut metadata = sample_metadata();
        metadata.quantisation = quantisation;

        let result = MapWriter::create(&path, &metadata);

        assert!(matches!(result, Err(TableError::InvalidQuantisation(_))));
    }

    assert!(!path.exists());
}
