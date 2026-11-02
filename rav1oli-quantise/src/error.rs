#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct InvalidQuantisation(pub &'static str);
