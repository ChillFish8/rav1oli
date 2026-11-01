# The vcpkg triplet `static-deps` installs for. Override on Intel macOS.
vcpkg_triplet := env("RAV1OLI_VCPKG_TRIPLET", if os() == "macos" { "arm64-osx-release" } else { "x64-linux-release" })

format:
    cargo +nightly fmt --all

# Builds the workspace, fetching submodules and static deps when missing.
#
# Takes `--static`, `--release`, and `--vulkan` or `--cuda` for the static Vship backend, Vulkan by default.
build *FLAGS:
    #!/usr/bin/env bash
    set -euo pipefail
    is_static=false
    profile_flag=""
    vship_backend=""
    for flag in {{FLAGS}}; do
        case "$flag" in
            --static) is_static=true ;;
            --release) profile_flag="--release" ;;
            --vulkan | --cuda)
                if [ -n "$vship_backend" ] && [ "$vship_backend" != "${flag#--}" ]; then
                    echo "--vulkan and --cuda cannot be used together" >&2
                    exit 1
                fi
                vship_backend="${flag#--}"
                ;;
            *) echo "unknown flag $flag, expected --static, --release, --vulkan or --cuda" >&2; exit 1 ;;
        esac
    done

    if [ -n "$vship_backend" ] && [ "$is_static" = false ]; then
        echo "--$vship_backend only applies to a --static build" >&2
        exit 1
    fi

    if git submodule status --recursive | grep -q '^-'; then
        just submodules
    fi

    if [ "$is_static" = false ]; then
        cargo build --workspace $profile_flag
        exit 0
    fi

    prefix="$PWD/vcpkg_installed/{{vcpkg_triplet}}"
    marker="$prefix/lib/pkgconfig/bestsource.pc"
    if [ ! -f "$marker" ] || [ -n "$(find vcpkg.json vcpkg/ports -newer "$marker")" ]; then
        just static-deps
    fi

    export BESTSOURCE_LIB_DIR="$prefix/lib"
    export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
    features="rav1oli-bestsource/static,rav1oli-vship/static,rav1oli-vship/${vship_backend:-vulkan}"
    cargo build --workspace $profile_flag --features "$features"

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
