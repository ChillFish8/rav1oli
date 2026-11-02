use anyhow::bail;
use rav1oli_table::Statistics;

/// Summarises per-frame values with the same definitions FFVship prints.
pub fn summarise(values: &[f64]) -> anyhow::Result<Statistics> {
    if values.is_empty() {
        bail!("cannot summarise an empty set of values");
    }

    let non_finite = values.iter().enumerate().find(|(_, value)| !value.is_finite());
    if let Some((index, value)) = non_finite {
        bail!("cannot summarise value {index}, it is {value} rather than a finite number");
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);

    let count = sorted.len();
    let average = sorted.iter().sum::<f64>() / count as f64;
    let variance = sorted.iter().map(|value| (value - average).powi(2)).sum::<f64>() / count as f64;

    Ok(Statistics {
        average,
        standard_deviation: variance.sqrt(),
        median: sorted[count / 2],
        percentile_5: sorted[count / 20],
        percentile_95: sorted[19 * count / 20],
        minimum: sorted[0],
        maximum: sorted[count - 1],
    })
}
