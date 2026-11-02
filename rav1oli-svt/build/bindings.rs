use std::path::Path;

/// The items bindgen keeps from the fork's API headers.
const ALLOWLIST: [&str; 5] = ["svt_.*", "Eb.*", "Svt.*", "SVT_.*", "EB_.*"];

/// Generates the bindings to SVT's encoder API into `output`.
pub fn generate_bindings(output: &Path) {
    let header = Path::new("include/wrapper.h");
    let api_dir = Path::new("vendor/svt-av1/Source/API");
    let header_text = header.to_string_lossy();
    let include_argument = format!("-I{}", api_dir.display());

    let mut builder = bindgen::Builder::default()
        .header(header_text)
        .clang_arg(include_argument)
        .prepend_enum_name(false)
        .rust_edition(bindgen::RustEdition::Edition2024);

    for pattern in ALLOWLIST {
        builder = builder.allowlist_item(pattern);
    }

    let generated = builder.generate();
    let bindings = match generated {
        Ok(bindings) => bindings,
        Err(error) => panic!("bindgen could not read {}: {error}", header.display()),
    };

    let written = bindings.write_to_file(output);
    if let Err(error) = written {
        panic!("could not write {}: {error}", output.display());
    }
}
