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
}
