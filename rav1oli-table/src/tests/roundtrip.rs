use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{RecordBatch, UInt32Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

use crate::metadata::METADATA_KEY;
use crate::tests::sample_metadata;
use crate::{MapMetadata, MapReader, MapWriter, TableError};

fn frame_scores(metadata: &MapMetadata, seed: u8) -> Vec<u8> {
    let blocks = metadata.source.blocks_per_frame();
    (0..blocks).map(|i| (i as u8).wrapping_add(seed)).collect()
}

fn write_map(path: &Path, metadata: &MapMetadata) {
    let mut writer = MapWriter::create(path, metadata).unwrap();

    for seed in 0..metadata.source.frame_count {
        let scores = frame_scores(metadata, seed as u8);
        writer.write_frame(&scores).unwrap();
    }

    writer.finish().unwrap();
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

    let result = writer.write_frame(&[0; 10]);

    let error = result.unwrap_err();
    assert!(matches!(error, TableError::FrameSizeMismatch { actual: 10, .. }));
}

#[test]
fn write_frame_rejects_extra_frames() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.parquet");
    let metadata = sample_metadata();
    let scores = frame_scores(&metadata, 0);
    let mut writer = MapWriter::create(&path, &metadata).unwrap();
    for _ in 0..metadata.source.frame_count {
        writer.write_frame(&scores).unwrap();
    }

    let result = writer.write_frame(&scores);

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
    writer.write_frame(&scores).unwrap();

    let result = writer.finish();

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
    let json = sample_metadata().to_json().unwrap();
    let footer = KeyValue::new(METADATA_KEY.to_string(), json);
    write_plain_parquet(&path, Some(vec![footer]));

    let result = MapReader::open(&path);

    assert!(matches!(result, Err(TableError::InvalidLayout(_))));
}
