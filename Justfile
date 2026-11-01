# The vcpkg triplet `static-deps` installs for. Override on Intel macOS.
vcpkg_triplet := env("RAV1OLI_VCPKG_TRIPLET", if os() == "macos" { "arm64-osx-release" } else { "x64-linux-release" })

format:
    cargo +nightly fmt --all

clippy:
    cargo clippy

run *ARGS:
    cargo run --release -- {{ARGS}}

# Fetches the vendored Vship, BestSource and libp2p sources at their pinned commits.
submodules:
    git submodule update --init --recursive

# Lints and tests the Vship wrapper built statically from its submodule with Vulkan.
test-vship:
    cargo clippy -p rav1oli-vship --features static,vulkan,gpu-tests --all-targets -- -D warnings
    cargo test --release -p rav1oli-vship --features static,vulkan,gpu-tests

# Lints and tests the BestSource wrapper built from its submodules against the system FFmpeg.
test-bestsource:
    cargo clippy -p rav1oli-bestsource --all-targets -- -D warnings
    cargo test -p rav1oli-bestsource

# Lints and tests the BestSource wrapper linked statically from the vcpkg tree. Run `static-deps` first.
test-bestsource-static:
    #!/usr/bin/env bash
    set -euo pipefail
    prefix="$PWD/vcpkg_installed/{{vcpkg_triplet}}"
    export BESTSOURCE_LIB_DIR="$prefix/lib"
    export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
    cargo clippy -p rav1oli-bestsource --features static --all-targets -- -D warnings
    cargo test -p rav1oli-bestsource --features static

# Builds the static BestSource and FFmpeg tree from vcpkg.json into vcpkg_installed/.
static-deps:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d .vcpkg ]; then
        git clone https://github.com/microsoft/vcpkg.git .vcpkg
    fi
    git -C .vcpkg fetch --quiet origin
    baseline="$(jq -r '."builtin-baseline"' vcpkg.json)"
    git -C .vcpkg checkout --quiet "$baseline"
    if [ ! -x .vcpkg/vcpkg ]; then
        .vcpkg/bootstrap-vcpkg.sh -disableMetrics
    fi
    .vcpkg/vcpkg install --triplet {{vcpkg_triplet}} --host-triplet {{vcpkg_triplet}}
