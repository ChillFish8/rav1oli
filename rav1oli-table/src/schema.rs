use std::sync::Arc;

use arrow_schema::{DataType, Field, Schema, SchemaRef};

pub(crate) const FRAME_INDEX_COLUMN: &str = "frame_index";
pub(crate) const SCORES_COLUMN: &str = "scores";

/// The columns of a distortion map, one row per frame.
pub(crate) fn map_schema() -> SchemaRef {
    let frame_index = Field::new(FRAME_INDEX_COLUMN, DataType::UInt32, false);
    let scores = Field::new(SCORES_COLUMN, DataType::Binary, false);
    let schema = Schema::new(vec![frame_index, scores]);
    Arc::new(schema)
}
