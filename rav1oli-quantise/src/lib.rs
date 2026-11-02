//! Turns distortion maps into per block scores
//!
//! A distortion map holds one metric value per pixel. This crate reduces it to one `u8` score for
//! every 8x8 block, keeping the highest value inside each block.

#[cfg(target_arch = "x86_64")]
mod avx2;
#[cfg(target_arch = "x86_64")]
mod avx512;
mod blocks;
mod error;
#[cfg(target_arch = "aarch64")]
mod neon;
mod quantisation;

#[cfg(test)]
mod tests;

pub use self::blocks::block_scores;
pub use self::error::InvalidQuantisation;
pub use self::quantisation::{BLOCK_SIZE, Quantisation};
