mod encode;
mod output;
mod paths;
mod planes;
mod source;
#[cfg(test)]
mod tests;

pub use self::encode::encode_video;
pub use self::output::{persist, temporary_sibling};
pub use self::paths::{check_file_path, check_output_directory, is_same_file};
pub use self::source::{SourceVideo, colour_description, describe_source, encoder_settings};
