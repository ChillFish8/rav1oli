mod colour;
mod display;
mod scorer;
mod statistics;

/// The GPU the GPU tests score on, picked by `RAV1OLI_DEVICE`.
#[cfg(feature = "gpu-tests")]
pub(crate) fn gpu_id() -> i32 {
    let selector = std::env::var("RAV1OLI_DEVICE").unwrap_or_default();
    let (kind, ordinal) = selector.split_once(':').unwrap_or((selector.as_str(), "0"));
    let wants_integrated = match kind {
        "discrete" => false,
        "integrated" => true,
        _ => return 0,
    };

    let ordinal: usize = ordinal.parse().expect("RAV1OLI_DEVICE has a numeric ordinal");

    let count = rav1oli_vship::device_count().expect("counts devices");
    let matching: Vec<i32> = (0..count)
        .filter(|&candidate| {
            let info = rav1oli_vship::device_info(candidate).expect("describes a device");
            info.integrated == wants_integrated
        })
        .collect();

    *matching
        .get(ordinal)
        .expect("RAV1OLI_DEVICE names a device Vship lists")
}
