mod display;
mod quantise;
mod statistics;
#[cfg(test)]
mod tests;

#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::display::select_display;
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::quantise::{BUTTERAUGLI_QUANTISATION, CVVDP_QUANTISATION, block_scores, quantise};
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::statistics::summarise;
