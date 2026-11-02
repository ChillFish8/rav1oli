use super::{BUTTERAUGLI, CVVDP};

#[test]
fn values_below_the_floor_score_zero() {
    let code = BUTTERAUGLI.quantise(0.4);
    assert_eq!(code, 0);
}

#[test]
fn values_at_the_floor_quantise_normally() {
    let code = BUTTERAUGLI.quantise(0.5);
    assert_eq!(code, 16);
}

#[test]
fn values_at_or_above_the_cap_score_the_top_code() {
    let at_cap = BUTTERAUGLI.quantise(8.0);
    let above_cap = BUTTERAUGLI.quantise(12.0);
    assert_eq!(at_cap, 255);
    assert_eq!(above_cap, 255);
}

#[test]
fn cvvdp_halfway_scores_128() {
    let code = CVVDP.quantise(0.5);
    assert_eq!(code, 128);
}

#[test]
fn cvvdp_zero_scores_zero() {
    let code = CVVDP.quantise(0.0);
    assert_eq!(code, 0);
}

#[test]
fn negative_and_nan_values_score_zero() {
    let negative = CVVDP.quantise(-1.0);
    let not_a_number = CVVDP.quantise(f32::NAN);
    assert_eq!(negative, 0);
    assert_eq!(not_a_number, 0);
}
