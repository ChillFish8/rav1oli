use std::sync::Arc;

use arrow_schema::{DataType, Field, Schema, SchemaRef};

use crate::metadata::Metric;

pub(crate) const FRAME_INDEX_COLUMN: &str = "frame_index";
pub(crate) const SCORES_COLUMN: &str = "scores";
pub(crate) const CVVDP_SCORE_COLUMN: &str = "score";
pub(crate) const NORM_Q_COLUMN: &str = "norm_q";
pub(crate) const NORM3_COLUMN: &str = "norm3";
pub(crate) const NORM_INF_COLUMN: &str = "norm_inf";

/// The columns of a distortion map, one row per frame.
///
/// The frame scores of the metric follow the block scores.
pub(crate) fn map_schema(metric: Metric) -> SchemaRef {
    let frame_index = Field::new(FRAME_INDEX_COLUMN, DataType::UInt32, false);
    let scores = Field::new(SCORES_COLUMN, DataType::Binary, false);
    let mut fields = vec![frame_index, scores];

    let score_columns: &[&str] = match metric {
        Metric::Cvvdp => &[CVVDP_SCORE_COLUMN],
        Metric::Butteraugli => &[NORM_Q_COLUMN, NORM3_COLUMN, NORM_INF_COLUMN],
    };
    for name in score_columns {
        let field = Field::new(*name, DataType::Float64, false);
        fields.push(field);
    }

    let schema = Schema::new(fields);
    Arc::new(schema)
}
