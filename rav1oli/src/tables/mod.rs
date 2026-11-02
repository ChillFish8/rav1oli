mod curve;
mod defaults;
#[cfg(test)]
mod tests;

#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the encode command will use the tables")
)]
pub use self::curve::{Curve, Tables};
