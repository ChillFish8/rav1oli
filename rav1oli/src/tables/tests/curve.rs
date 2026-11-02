use rav1oli_quantise::Quantisation;
use rav1oli_table::Metric;

use crate::metrics::BUTTERAUGLI_QUANTISATION;
use crate::tables::{Curve, Tables};

fn table_json(metric: &str, drop: &str, cut: &str) -> String {
    format!(r#"{{"version": 1, "metric": "{metric}", "drop": {drop}, "cut": {cut}}}"#)
}

fn error_of(json: &str) -> String {
    let error = Tables::parse(json).expect_err("the table should be rejected");
    format!("{error:#}")
}

fn curve(points: &[(f64, f64)]) -> Curve {
    Curve::new("drop", points, 80).expect("the curve should be valid")
}

#[test]
fn parses_a_full_table() {
    let json = table_json("cvvdp", "[[0.0, 0], [1.0, 10]]", "[[0.5, 5]]");
    let tables = Tables::parse(&json).unwrap();

    assert_eq!(tables.metric, Metric::Cvvdp);
    assert_eq!(tables.drop.points(), [(0.0, 0), (1.0, 10)]);
    assert_eq!(tables.cut.points(), [(0.5, 5)]);
}

#[test]
fn a_missing_curve_uses_the_default() {
    let json = r#"{"version": 1, "metric": "butteraugli", "drop": [[1.0, 3]]}"#;
    let tables = Tables::parse(json).unwrap();
    let defaults = Tables::default_for(Metric::Butteraugli);

    assert_eq!(tables.drop.points(), [(1.0, 3)]);
    assert_eq!(tables.cut.points(), defaults.cut.points());
}

#[test]
fn an_unknown_version_is_an_error() {
    let message = error_of(r#"{"version": 2, "metric": "cvvdp"}"#);

    assert!(message.contains("version 2"), "{message}");
    assert!(message.contains("only version 1"), "{message}");
}

#[test]
fn a_missing_version_is_an_error() {
    let message = error_of(r#"{"metric": "cvvdp"}"#);

    assert!(message.contains("version"), "{message}");
}

#[test]
fn an_unknown_metric_is_an_error() {
    let message = error_of(r#"{"version": 1, "metric": "ssim"}"#);

    assert!(message.contains("ssim"), "{message}");
}

#[test]
fn an_unknown_key_is_an_error() {
    let message = error_of(r#"{"version": 1, "metric": "cvvdp", "cuts": [[0.0, 0]]}"#);

    assert!(message.contains("cuts"), "{message}");
}

#[test]
fn an_empty_curve_is_an_error() {
    let message = error_of(&table_json("cvvdp", "[]", "[[0.0, 0]]"));

    assert!(message.contains("drop"), "{message}");
    assert!(message.contains("at least one point"), "{message}");
}

#[test]
fn non_increasing_distortions_are_an_error() {
    let message = error_of(&table_json(
        "cvvdp",
        "[[0.0, 0], [1.0, 5], [1.0, 6]]",
        "[[0.0, 0]]",
    ));

    assert!(message.contains("drop"), "{message}");
    assert!(message.contains("point 2"), "{message}");
}

#[test]
fn a_negative_distortion_is_an_error() {
    let message = error_of(&table_json("cvvdp", "[[0.0, 0]]", "[[-1.0, 0]]"));

    assert!(message.contains("cut"), "{message}");
    assert!(message.contains("point 0"), "{message}");
}

#[test]
fn a_drop_above_80_is_an_error() {
    let message = error_of(&table_json("cvvdp", "[[0.0, 0], [1.0, 81]]", "[[0.0, 0]]"));

    assert!(message.contains("drop"), "{message}");
    assert!(message.contains("point 1"), "{message}");
    assert!(message.contains("80"), "{message}");
}

#[test]
fn a_cut_above_230_is_an_error() {
    let message = error_of(&table_json("cvvdp", "[[0.0, 0]]", "[[0.0, 231]]"));

    assert!(message.contains("cut"), "{message}");
    assert!(message.contains("230"), "{message}");
}

#[test]
fn a_fractional_value_is_an_error() {
    let message = error_of(&table_json("cvvdp", "[[0.0, 2.5]]", "[[0.0, 0]]"));

    assert!(message.contains("whole"), "{message}");
    assert!(message.contains("point 0"), "{message}");
}

#[test]
fn below_the_first_point_uses_its_value() {
    let curve = curve(&[(1.0, 4.0), (2.0, 8.0)]);

    assert_eq!(curve.evaluate(0.0), 4);
}

#[test]
fn above_the_last_point_uses_its_value() {
    let curve = curve(&[(1.0, 4.0), (2.0, 8.0)]);

    assert_eq!(curve.evaluate(5.0), 8);
}

#[test]
fn points_evaluate_exactly() {
    let curve = curve(&[(0.5, 0.0), (1.5, 8.0), (3.0, 24.0)]);

    assert_eq!(curve.evaluate(0.5), 0);
    assert_eq!(curve.evaluate(1.5), 8);
    assert_eq!(curve.evaluate(3.0), 24);
}

#[test]
fn between_points_interpolates_and_rounds() {
    let curve = curve(&[(0.0, 0.0), (1.0, 10.0)]);

    assert_eq!(curve.evaluate(0.25), 3);
    assert_eq!(curve.evaluate(0.24), 2);
}

#[test]
fn a_single_point_is_constant() {
    let curve = curve(&[(1.0, 7.0)]);

    assert_eq!(curve.evaluate(0.0), 7);
    assert_eq!(curve.evaluate(1.0), 7);
    assert_eq!(curve.evaluate(9.0), 7);
}

#[test]
fn a_table_for_another_metric_is_an_error() {
    let tables = Tables::default_for(Metric::Cvvdp);
    let error = tables.check_metric(Metric::Butteraugli).unwrap_err();
    let message = format!("{error:#}");

    assert!(message.contains("cvvdp"), "{message}");
    assert!(message.contains("butteraugli"), "{message}");
    assert!(tables.check_metric(Metric::Cvvdp).is_ok());
}

#[test]
fn lookups_follow_the_quantisation() {
    let tables = Tables::default_for(Metric::Butteraugli);
    let quantisation = Quantisation {
        cap: 8.0,
        floor: 0.5,
        top: 255,
    };
    let lookups = tables.lookups(&quantisation);

    assert_eq!(lookups.drops[0], 0);
    assert_eq!(lookups.cuts[0], 0);
    assert_eq!(lookups.drops[255], 80);
    assert_eq!(lookups.cuts[255], 230);
    assert_eq!(lookups.drops[48], 8);
}

#[test]
fn lookups_work_with_the_default_quantisation() {
    let tables = Tables::default_for(Metric::Butteraugli);
    let lookups = tables.lookups(&BUTTERAUGLI_QUANTISATION);

    assert_eq!(lookups.drops[255], 80);
}

#[test]
fn load_names_the_path_on_failure() {
    let error = Tables::load(std::path::Path::new("/nonexistent/table.json")).unwrap_err();
    let message = format!("{error:#}");

    assert!(message.contains("/nonexistent/table.json"), "{message}");
}

#[test]
fn a_negative_version_is_an_error() {
    let message = error_of(r#"{"version": -1, "metric": "cvvdp"}"#);

    assert!(message.contains("version -1"), "{message}");
    assert!(message.contains("only version 1"), "{message}");
}

#[test]
fn a_fractional_version_is_an_error() {
    let message = error_of(r#"{"version": 1.5, "metric": "cvvdp"}"#);

    assert!(message.contains("whole-number version"), "{message}");
}

#[test]
fn an_infinite_distortion_names_the_curve_and_point() {
    let error = Curve::new("cut", &[(0.0, 0.0), (f64::INFINITY, 1.0)], 230).unwrap_err();
    let message = error.to_string();

    assert!(message.contains("cut curve's point 1"), "{message}");
    assert!(message.contains("finite distortion"), "{message}");
}
