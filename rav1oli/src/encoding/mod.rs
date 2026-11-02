mod grids;
#[cfg(test)]
mod tests;

#[cfg_attr(
    not(test),
    expect(unused_imports, reason = "the encode command will use the grid builder")
)]
pub use self::grids::{GridBuilder, check_frame_index};
