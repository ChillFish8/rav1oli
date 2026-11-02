use crate::simd::check_simd_cache;

const X86_64_CACHE: &str = "\
//Test C_FLAG_mavx2
C_FLAG_mavx2:INTERNAL=1
//Enable building avx512 code
ENABLE_AVX512:BOOL=ON
";

const AARCH64_CACHE: &str = "\
//Test C_FLAG_march_armv8_2_a_dotprod
C_FLAG_march_armv8_2_a_dotprod:INTERNAL=1
ENABLE_AVX512:INTERNAL=OFF
";

#[test]
fn an_x86_64_cache_with_avx2_and_avx512_passes() {
    let checked = check_simd_cache(X86_64_CACHE, "x86_64");
    assert_eq!(checked, Ok(()));
}

#[test]
fn an_aarch64_cache_with_dotprod_passes() {
    let checked = check_simd_cache(AARCH64_CACHE, "aarch64");
    assert_eq!(checked, Ok(()));
}

#[test]
fn a_failed_avx2_check_names_its_entry() {
    let cache = X86_64_CACHE.replace("C_FLAG_mavx2:INTERNAL=1", "C_FLAG_mavx2:INTERNAL=");
    let error = check_simd_cache(&cache, "x86_64").unwrap_err();

    assert!(error.contains("C_FLAG_mavx2"), "{error}");
}

#[test]
fn avx512_forced_off_names_its_entry() {
    let cache = X86_64_CACHE.replace("ENABLE_AVX512:BOOL=ON", "ENABLE_AVX512:INTERNAL=OFF");
    let error = check_simd_cache(&cache, "x86_64").unwrap_err();

    assert!(error.contains("ENABLE_AVX512"), "{error}");
}

#[test]
fn a_missing_dotprod_check_names_its_entry() {
    let error = check_simd_cache(X86_64_CACHE, "aarch64").unwrap_err();
    assert!(error.contains("C_FLAG_march_armv8_2_a_dotprod"), "{error}");
}

#[test]
fn the_entry_name_must_match_exactly() {
    let cache = "C_FLAG_mavx2_extra:INTERNAL=1\nENABLE_AVX512:BOOL=ON\n";
    let error = check_simd_cache(cache, "x86_64").unwrap_err();

    assert!(error.contains("C_FLAG_mavx2"), "{error}");
}
