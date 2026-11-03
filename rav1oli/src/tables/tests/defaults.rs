use rav1oli_svt::MapMode;
use rav1oli_table::Metric;

use crate::tables::Tables;

const CVVDP_DROP: [(f64, i16); 6] = [(0.05, 0), (0.15, 8), (0.3, 20), (0.5, 36), (0.75, 56), (1.0, 80)];
const CVVDP_CUT: [(f64, i16); 6] = [
    (0.05, 0),
    (0.15, 40),
    (0.3, 90),
    (0.5, 140),
    (0.75, 190),
    (1.0, 230),
];
const CVVDP_OFFSET: [(f64, i16); 6] = [
    (0.05, 0),
    (0.15, -2),
    (0.3, -6),
    (0.5, -11),
    (0.75, -17),
    (1.0, -24),
];
const BUTTERAUGLI_DROP: [(f64, i16); 6] = [(0.5, 0), (1.0, 8), (1.6, 20), (2.2, 36), (3.2, 56), (4.5, 80)];
const BUTTERAUGLI_CUT: [(f64, i16); 3] = [(0.5, 0), (1.6, 40), (3.2, 110)];
const BUTTERAUGLI_OFFSET: [(f64, i16); 6] =
    [(0.5, 0), (1.0, -4), (1.6, -8), (2.2, -12), (3.2, -18), (4.5, -24)];
const FLAT_WEIGHT: [(f64, i16); 1] = [(0.0, 0)];

#[test]
fn every_default_file_parses_and_validates() {
    for metric in [Metric::Cvvdp, Metric::Butteraugli] {
        for mode in [MapMode::Additive, MapMode::Redistribute] {
            let tables = Tables::default_for(metric, mode);
            assert_eq!(tables.metric, metric);
            assert_eq!(tables.mode, mode);
        }
    }
}

#[test]
fn additive_defaults_match_the_spec_points() {
    let cvvdp = Tables::default_for(Metric::Cvvdp, MapMode::Additive);
    assert_eq!(cvvdp.superblock.points(), CVVDP_DROP);
    assert_eq!(cvvdp.block.points(), CVVDP_CUT);

    let butteraugli = Tables::default_for(Metric::Butteraugli, MapMode::Additive);
    assert_eq!(butteraugli.superblock.points(), BUTTERAUGLI_DROP);
    assert_eq!(butteraugli.block.points(), BUTTERAUGLI_CUT);
}

#[test]
fn redistribute_defaults_match_the_spec_points() {
    let cvvdp = Tables::default_for(Metric::Cvvdp, MapMode::Redistribute);
    assert_eq!(cvvdp.superblock.points(), CVVDP_OFFSET);
    assert_eq!(cvvdp.block.points(), FLAT_WEIGHT);

    let butteraugli = Tables::default_for(Metric::Butteraugli, MapMode::Redistribute);
    assert_eq!(butteraugli.superblock.points(), BUTTERAUGLI_OFFSET);
    assert_eq!(butteraugli.block.points(), FLAT_WEIGHT);
}
