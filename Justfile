

format:
    cargo +nightly fmt --all

clippy:
    cargo clippy

run *ARGS:
    cargo run --release -- {{ARGS}}

# Fetches the vendored Vship sources at their pinned commit.
submodules:
    git submodule update --init --recursive

# Lints and tests the Vship wrapper built statically from its submodule with Vulkan.
test-vship:
    cargo clippy -p rav1oli-vship --features static,vulkan,gpu-tests --all-targets -- -D warnings
    cargo test --release -p rav1oli-vship --features static,vulkan,gpu-tests
