/// The CMake cache entries that must be on for SVT's SIMD kernels to keep their target features.
///
/// SVT's `check_flag` names each flag check `C_FLAG` followed by the flag with every symbol made `_`.
pub fn required_simd_entries(target_arch: &str) -> &'static [&'static str] {
    match target_arch {
        "x86_64" => &["C_FLAG_mavx2", "ENABLE_AVX512"],
        "aarch64" => &["C_FLAG_march_armv8_2_a_dotprod"],
        _ => &[],
    }
}

/// Checks every required SIMD entry is on in the text of a `CMakeCache.txt`.
///
/// A failed flag check makes SVT silently drop that flag, so the error names the first entry that is off.
pub fn check_simd_cache(cache: &str, target_arch: &str) -> Result<(), String> {
    for entry in required_simd_entries(target_arch) {
        let value = cache_value(cache, entry);
        let Some(value) = value else {
            let reason = format!("SVT-AV1's CMake configure did not set `{entry}`");
            return Err(reason);
        };

        if !is_cmake_true(value) {
            let reason = format!(
                "SVT-AV1's CMake configure left `{entry}` off, so its {target_arch} SIMD kernels would build \
                 without their target features"
            );
            return Err(reason);
        }
    }

    Ok(())
}

/// The value of `entry` in a cache whose lines read `NAME:TYPE=VALUE`.
fn cache_value<'a>(cache: &'a str, entry: &str) -> Option<&'a str> {
    cache.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        let (name, _) = key.split_once(':')?;
        let is_entry = name == entry;
        is_entry.then_some(value)
    })
}

/// Whether CMake reads `value` as true in an `if()`.
fn is_cmake_true(value: &str) -> bool {
    let upper = value.to_ascii_uppercase();
    let is_constant = ["1", "ON", "YES", "TRUE", "Y"].contains(&upper.as_str());
    let number = value.parse::<f64>();
    let is_non_zero = number.is_ok_and(|number| number != 0.0);

    is_constant || is_non_zero
}
