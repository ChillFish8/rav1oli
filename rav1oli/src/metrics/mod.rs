mod colour;
mod display;
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
    expect(unused_imports, reason = "the colour mapping is only used by tests")
)]
pub use self::colour::{FrameDescription, PlaneBuffers, check_compatible, colour_space};
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "display selection is only used by tests")
)]
pub use self::display::{SelectedDisplay, select_display};
#[cfg_attr(
    not(all(test, feature = "gpu-tests")),
    expect(unused_imports, reason = "the scorer is only driven by the GPU tests")
)]
pub use self::scorer::MetricScorer;
pub use self::scorer::{BUTTERAUGLI_QUANTISATION, CVVDP_QUANTISATION, MetricConfig, metric_settings};
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the statistics are only used by tests")
)]
pub use self::statistics::summarise;
