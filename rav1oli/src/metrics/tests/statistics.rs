use crate::metrics::summarise;

#[test]
fn summarise_matches_ffvship() {
    let statistics = summarise(&[5.0, 1.0, 3.0, 2.0, 4.0]).unwrap();

    assert_eq!(statistics.average, 3.0);
    assert!((statistics.standard_deviation - 2.0_f64.sqrt()).abs() < 1e-12);
    assert_eq!(statistics.median, 3.0);
    assert_eq!(statistics.percentile_5, 1.0);
    assert_eq!(statistics.percentile_95, 5.0);
    assert_eq!(statistics.minimum, 1.0);
    assert_eq!(statistics.maximum, 5.0);
}

#[test]
fn summarise_of_nothing_is_an_error() {
    let result = summarise(&[]);
    assert!(result.is_err());
}

#[test]
fn summarise_uses_ffvship_percentile_indices() {
    let values: Vec<f64> = (0..40).rev().map(f64::from).collect();

    let statistics = summarise(&values).unwrap();

    assert_eq!(statistics.percentile_5, 2.0);
    assert_eq!(statistics.percentile_95, 38.0);
    assert_eq!(statistics.median, 20.0);
}

#[test]
fn summarise_with_a_nan_does_not_panic() {
    let result = summarise(&[1.0, f64::NAN, 3.0]);
    assert!(result.is_ok());
}
