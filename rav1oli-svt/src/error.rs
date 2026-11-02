/// A problem with the SVT-AV1 flags passed to the encoder.
#[derive(Debug, thiserror::Error)]
pub enum ParamError {
    #[error("SVT-AV1 flag `{name}` needs a value")]
    MissingValue { name: String },

    #[error("`{token}` is a value with no SVT-AV1 flag before it")]
    StrayValue { token: String },

    #[error("`{name}` is not an SVT-AV1 flag the fork accepts")]
    Unknown { name: String },

    #[error("SVT-AV1 rejected `{value}` as the value of `{name}`")]
    Rejected { name: String, value: String },

    #[error("`{name}` is set by rav1oli and cannot be passed as an SVT-AV1 flag")]
    EncoderOwned { name: String },

    #[error("SVT-AV1 could not create a default config (status {status:#x})")]
    Setup { status: i32 },

    #[error("{reason}")]
    Requirement { reason: &'static str },

    #[error("SVT-AV1 rejected the flags `{flags}` together (status {status:#x}), see its log for why")]
    Combination { flags: String, status: i32 },
}

/// Which of the two distortion grids a [GridError] is about.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum GridKind {
    Drops,
    Cuts,
}

impl std::fmt::Display for GridKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            GridKind::Drops => "64x64 qindex drop grid",
            GridKind::Cuts => "8x8 lambda cut grid",
        };

        formatter.write_str(name)
    }
}

/// A distortion grid that does not fit its frame.
#[derive(Debug, thiserror::Error)]
pub enum GridError {
    #[error(
        "the {grid} has {actual} cells, but a {width}x{height} frame needs {columns}x{rows} = {expected}"
    )]
    Size {
        grid: GridKind,
        width: u32,
        height: u32,
        columns: usize,
        rows: usize,
        expected: usize,
        actual: usize,
    },

    #[error("the {grid} holds {value} at row {row}, column {column}, above its maximum of {max}")]
    OutOfRange {
        grid: GridKind,
        row: usize,
        column: usize,
        value: u8,
        max: u8,
    },

    #[error(
        "the grids were built for a {grid_width}x{grid_height} frame, \
         but the encoder takes {frame_width}x{frame_height}"
    )]
    FrameMismatch {
        grid_width: u32,
        grid_height: u32,
        frame_width: u32,
        frame_height: u32,
    },
}

/// A problem setting up the encoder or encoding a frame.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error(transparent)]
    Param(#[from] ParamError),

    #[error(transparent)]
    Grid(#[from] GridError),

    #[error("SVT-AV1 call `{call}` failed with status {code:#x}")]
    Svt { call: &'static str, code: i32 },

    #[error("could not write the IVF output {}", path.display())]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[error("{reason}")]
    UnsupportedFormat { reason: String },

    #[error("the colour {field} code {code} {reason}")]
    InvalidColourCode {
        field: &'static str,
        code: u8,
        reason: &'static str,
    },
}
