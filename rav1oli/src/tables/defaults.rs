use rav1oli_table::Metric;

const CVVDP: &str = include_str!("../../tables/cvvdp.json");
const BUTTERAUGLI: &str = include_str!("../../tables/butteraugli.json");

pub fn default_json(metric: Metric) -> &'static str {
    match metric {
        Metric::Cvvdp => CVVDP,
        Metric::Butteraugli => BUTTERAUGLI,
    }
}
