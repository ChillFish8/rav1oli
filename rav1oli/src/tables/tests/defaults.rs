use rav1oli_table::Metric;

use crate::tables::Tables;

const CVVDP_DROP: [(f64, u8); 6] = [(0.05, 0), (0.15, 8), (0.3, 20), (0.5, 36), (0.75, 56), (1.0, 80)];
const CVVDP_CUT: [(f64, u8); 6] = [
    (0.05, 0),
    (0.15, 40),
    (0.3, 90),
    (0.5, 140),
    (0.75, 190),
    (1.0, 230),
];
const BUTTERAUGLI_DROP: [(f64, u8); 5] = [(0.5, 0), (1.5, 8), (3.0, 24), (5.0, 48), (8.0, 80)];
const BUTTERAUGLI_CUT: [(f64, u8); 5] = [(0.5, 0), (1.5, 40), (3.0, 110), (5.0, 180), (8.0, 230)];

#[test]
fn both_default_files_parse_and_validate() {
    for metric in [Metric::Cvvdp, Metric::Butteraugli] {
        let tables = Tables::default_for(metric);
        assert_eq!(tables.metric, metric);
    }
}

#[test]
fn defaults_match_the_spec_points() {
    let cvvdp = Tables::default_for(Metric::Cvvdp);
    assert_eq!(cvvdp.drop.points(), CVVDP_DROP);
    assert_eq!(cvvdp.cut.points(), CVVDP_CUT);

    let butteraugli = Tables::default_for(Metric::Butteraugli);
    assert_eq!(butteraugli.drop.points(), BUTTERAUGLI_DROP);
    assert_eq!(butteraugli.cut.points(), BUTTERAUGLI_CUT);
}
