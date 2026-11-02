mod colour;
mod display;
mod scorer;
mod statistics;
#[cfg(test)]
pub(crate) mod tests;

pub use self::colour::FrameDescription;
#[cfg(test)]
pub use self::colour::{PlaneBuffers, check_compatible, colour_space};
#[cfg(test)]
pub use self::display::SelectedDisplay;
pub use self::display::select_display;
pub use self::scorer::{
    BUTTERAUGLI_QUANTISATION,
    CVVDP_QUANTISATION,
    MetricConfig,
    MetricScorer,
    metric_settings,
};
#[cfg(test)]
pub use self::statistics::summarise;
