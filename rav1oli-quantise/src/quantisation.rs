use serde::{Deserialize, Serialize};

use crate::error::InvalidQuantisation;

/// The width and height in pixels of the area each score covers.
pub const BLOCK_SIZE: u32 = 8;

/// Turns a block's metric value into a `u8` score.
///
/// Values below `floor` score 0. Other values are clamped between 0 and `cap`, then scaled so `cap`
/// scores `top`.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quantisation {
    pub cap: f32,
    pub floor: f32,
    pub top: u8,
}

impl Quantisation {
    /// Checks the scale can be used to quantise and dequantise scores.
    pub fn check(&self) -> Result<(), InvalidQuantisation> {
        if !self.cap.is_finite() || self.cap <= 0.0 {
            return Err(InvalidQuantisation("cap must be finite and above 0"));
        }

        if !self.floor.is_finite() || self.floor < 0.0 {
            return Err(InvalidQuantisation("floor must be finite and at least 0"));
        }

        if self.floor >= self.cap {
            return Err(InvalidQuantisation("floor must be below the cap"));
        }

        if self.top == 0 {
            return Err(InvalidQuantisation("top must be above 0"));
        }

        Ok(())
    }

    /// Converts a metric value into a score code.
    ///
    /// Values below the floor, negative values and NaN score 0. Everything else is clamped to the
    /// cap and scaled so the cap scores the top code.
    pub fn quantise(&self, value: f32) -> u8 {
        let factor = self.factor();
        self.quantise_with_factor(value, factor)
    }

    /// Turns a score back into a metric value.
    ///
    /// A code of `top` is saturated, meaning the value was at or above the cap. A code of 0 means
    /// the value was below the floor or truly zero.
    pub fn dequantise(&self, code: u8) -> f32 {
        let factor = self.cap / f32::from(self.top);
        f32::from(code) * factor
    }

    /// The amount a clamped value is multiplied by to give its code.
    pub(crate) fn factor(&self) -> f32 {
        f32::from(self.top) / self.cap
    }

    /// Converts a metric value into a score code using a factor from [Quantisation::factor].
    pub(crate) fn quantise_with_factor(&self, value: f32, factor: f32) -> u8 {
        if value.is_nan() || value < self.floor {
            return 0;
        }

        let clamped = value.clamp(0.0, self.cap);
        let scaled = clamped * factor;
        scaled.round() as u8
    }
}
