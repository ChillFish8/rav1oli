mod blocks;
mod paths;
mod quantisation;

use crate::Quantisation;

pub(crate) const BUTTERAUGLI: Quantisation = Quantisation {
    cap: 8.0,
    floor: 0.5,
    top: 255,
};

pub(crate) const CVVDP: Quantisation = Quantisation {
    cap: 1.0,
    floor: 0.0,
    top: 255,
};
