mod output;
mod source;
#[cfg(test)]
mod tests;

#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "the output paths are only used by tests until the analyse command writes outputs"
    )
)]
pub use self::output::{persist, temporary_sibling};
#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the settings are only used by tests")
)]
pub use self::source::{
    SourceVideo,
    colour_description,
    describe_source,
    encoder_settings,
    map_metadata,
    metric_config,
};
