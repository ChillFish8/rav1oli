mod colour;
mod display;
mod quantise;
#[cfg_attr(
    all(test, not(feature = "gpu-tests")),
    expect(dead_code, reason = "the scorer is only driven by the GPU tests")
)]
mod scorer;
mod statistics;
#[cfg(test)]
mod tests;

#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::colour::{FrameDescription, PlaneBuffers, check_compatible, colour_space};
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::display::{SelectedDisplay, select_display};
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::quantise::{BUTTERAUGLI_QUANTISATION, CVVDP_QUANTISATION, block_scores, quantise};
#[cfg_attr(
    not(all(test, feature = "gpu-tests")),
    expect(unused_imports, reason = "the scorer is only driven by the GPU tests")
)]
pub use self::scorer::MetricScorer;
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::scorer::{MetricConfig, metric_settings};
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the metrics are only used by tests")
)]
pub use self::statistics::summarise;
