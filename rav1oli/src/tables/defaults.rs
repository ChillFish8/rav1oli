use rav1oli_svt::MapMode;
use rav1oli_table::Metric;

const CVVDP: &str = include_str!("../../tables/cvvdp.json");
const BUTTERAUGLI: &str = include_str!("../../tables/butteraugli.json");
const CVVDP_ADDITIVE: &str = include_str!("../../tables/cvvdp-additive.json");
const BUTTERAUGLI_ADDITIVE: &str = include_str!("../../tables/butteraugli-additive.json");

pub fn default_json(metric: Metric, mode: MapMode) -> &'static str {
    match (metric, mode) {
        (Metric::Cvvdp, MapMode::Redistribute) => CVVDP,
        (Metric::Butteraugli, MapMode::Redistribute) => BUTTERAUGLI,
        (Metric::Cvvdp, MapMode::Additive) => CVVDP_ADDITIVE,
        (Metric::Butteraugli, MapMode::Additive) => BUTTERAUGLI_ADDITIVE,
    }
}
