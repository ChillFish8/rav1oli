mod cache;
mod decoder;
mod lock;
#[cfg(test)]
pub(crate) mod tests;

pub use self::cache::CacheLocation;
#[expect(unused_imports, reason = "the decoder is only used by tests")]
pub use self::decoder::{Decoder, Frames};
