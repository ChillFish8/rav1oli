# The vcpkg triplet `static-deps` installs for. Override on Intel macOS.
vcpkg_triplet := env("RAV1OLI_VCPKG_TRIPLET", if os() == "macos" { "arm64-osx-release" } else { "x64-linux-release" })

# Cargo runs build scripts from the crate dir, so a relative SVT-AV1 profile path is made absolute first.
svt_profile := env("RAV1OLI_SVT_PGO_PROFILE", "")
svt_profile_path := if svt_profile == "" { "" } else { absolute_path(svt_profile) }
svt_profile_assignment := "RAV1OLI_SVT_PGO_PROFILE=" + quote(svt_profile_path) + " "
svt_profile_env := if svt_profile == "" { "" } else { svt_profile_assignment }

# The clip `pgo-svt` trains on.
pgo_clip := env("RAV1OLI_PGO_CLIP", "")

format:
    cargo +nightly fmt --all

# Builds the workspace, fetching submodules and static deps when missing.
#
# Takes `--static`, `--release`, and `--vulkan` or `--cuda` for the static Vship backend, Vulkan by default.
# `--release` also builds SVT-AV1 with LTO and PGO, which need `llvm-link`, `llvm-ar` and `llvm-profdata` from
# clang's LLVM version. It uses the profile in `RAV1OLI_SVT_PGO_PROFILE`, or trains one with `pgo-svt` first.
build *FLAGS:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "${RAV1OLI_SVT_PGO_PROFILE:-}" ]; then
        export RAV1OLI_SVT_PGO_PROFILE={{ quote(svt_profile_path) }}
    fi

    is_static=false
    profile_flag=""
    features=""
    vship_backend=""
    for flag in {{FLAGS}}; do
        case "$flag" in
            --static) is_static=true ;;
            --release)
                profile_flag="--release"
                features="rav1oli-svt/lto"
                ;;
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

    if [ -n "$profile_flag" ] && [ -z "${RAV1OLI_SVT_PGO_PROFILE:-}" ]; then
        just pgo-svt
        export RAV1OLI_SVT_PGO_PROFILE="$PWD/target/pgo/svt.profdata"
    fi

    if [ "$is_static" = false ]; then
        cargo build --workspace $profile_flag --features "$features"
        exit 0
    fi

    prefix="$PWD/vcpkg_installed/{{vcpkg_triplet}}"
    marker="$prefix/lib/pkgconfig/bestsource.pc"
    if [ ! -f "$marker" ] || [ -n "$(find vcpkg.json vcpkg/ports -newer "$marker")" ]; then
        just static-deps
    fi

    export BESTSOURCE_LIB_DIR="$prefix/lib"
    export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
    features="$features,rav1oli-bestsource/static,rav1oli-vship/static,rav1oli-vship/${vship_backend:-vulkan}"
    cargo build --workspace $profile_flag --features "$features"

clippy *ARGS:
    {{ svt_profile_env }}cargo clippy {{ARGS}}

run *ARGS:
    {{ svt_profile_env }}cargo run --release -- {{ARGS}}

# Runs benchmarks, passing any extra arguments through.
bench *ARGS:
    {{ svt_profile_env }}cargo bench {{ARGS}}

# Trains an SVT-AV1 PGO profile into `target/pgo/svt.profdata` from distortion map encodes.
#
# The clip comes from `RAV1OLI_PGO_CLIP`. The instrumented build uses the release profile, LTO and the
# caller's RUSTFLAGS, so the profile matches the release build that uses it. It trains at presets 4 and 8.
# x86_64 hosts with AVX2 run both again capped at AVX2, so those kernels are profiled too.
pgo-svt:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ if pgo_clip == "" { error("set RAV1OLI_PGO_CLIP to the training clip") } else { "" } }}

    clang="${CC:-clang}"
    if ! clang_path="$(command -v "$clang")"; then
        echo "$clang not found, but training needs it to find a matching llvm-profdata" >&2
        exit 1
    fi

    clang_major="$("$clang" --version | sed -n 's/.*version \([0-9][0-9]*\).*/\1/p' | head -n 1)"
    clang_dir="$(dirname "$clang_path")"
    llvm_profdata=""
    for candidate in "$clang_dir/llvm-profdata" llvm-profdata \
        "$clang_dir/llvm-profdata-$clang_major" "llvm-profdata-$clang_major"; do
        if ! command -v "$candidate" > /dev/null; then
            continue
        fi

        version_text="$("$candidate" --version)"
        major="$(echo "$version_text" | sed -n 's/.*version \([0-9][0-9]*\).*/\1/p' | head -n 1)"
        if [ "$major" = "$clang_major" ]; then
            llvm_profdata="$candidate"
            break
        fi
    done

    if [ -z "$llvm_profdata" ]; then
        echo "no llvm-profdata shares the LLVM major version of $clang_path, which is $clang_major" >&2
        exit 1
    fi

    pgo_dir="$PWD/target/pgo"
    raw_dir="$pgo_dir/raw"
    profile="$pgo_dir/svt.profdata"
    rm -rf "$raw_dir"
    mkdir -p "$raw_dir"

    unset RAV1OLI_SVT_PGO_PROFILE
    export RAV1OLI_SVT_PGO_GENERATE="$raw_dir"
    export RAV1OLI_PGO_OUTPUT="$pgo_dir/train.ivf"
    features="rav1oli-svt/lto,rav1oli-svt/pgo-train"
    cargo build --release -p rav1oli-svt --example pgo-train --features "$features"
    train=(cargo run --release -p rav1oli-svt --example pgo-train --features "$features")

    has_avx2=false
    if [ "{{ arch() }}" != "x86_64" ]; then
        echo "skipping the AVX2-capped runs, as AVX2 kernels only exist on x86_64"
    elif ! rustc --print cfg -C target-cpu=native | grep -q 'target_feature="avx2"'; then
        echo "skipping the AVX2-capped runs, as this host has no AVX2"
    else
        has_avx2=true
    fi

    # Preset 4 profiles the slow paths and preset 8 the fast ones. Each run writes its own raw profile.
    for preset in 4 8; do
        export RAV1OLI_PGO_PRESET="$preset"
        export LLVM_PROFILE_FILE="$raw_dir/preset-$preset-%p.profraw"
        "${train[@]}"

        if [ "$has_avx2" = true ]; then
            export LLVM_PROFILE_FILE="$raw_dir/preset-$preset-avx2-%p.profraw"
            RAV1OLI_PGO_CAP_AVX2=1 "${train[@]}"
        fi
    done

    shopt -s nullglob
    raw_profiles=("$raw_dir"/*.profraw)
    if [ "${#raw_profiles[@]}" -eq 0 ]; then
        echo "the training runs wrote no raw profiles into $raw_dir" >&2
        exit 1
    fi

    "$llvm_profdata" merge -o "$profile" "${raw_profiles[@]}"

    summary="$("$llvm_profdata" show "$profile")"
    total_functions="$(echo "$summary" | sed -n 's/^Total functions: //p')"
    maximum_count="$(echo "$summary" | sed -n 's/^Maximum function count: //p')"
    echo "the profile covers ${total_functions:-no} functions, with a top count of ${maximum_count:-0}"
    if [ "${total_functions:-0}" -le 1000 ] || [ "${maximum_count:-0}" -le 0 ]; then
        echo "the profile needs more than 1000 functions and a maximum function count above 0" >&2
        exit 1
    fi

    echo "SVT-AV1 profile written to $profile"
    echo "Pass it to a build with RAV1OLI_SVT_PGO_PROFILE=$profile"

# Fetches the vendored Vship, BestSource and libp2p sources at their pinned commits.
submodules:
    git submodule update --init --recursive

# Runs tests with nextest, passing any extra arguments through.
test *ARGS:
    {{ svt_profile_env }}cargo nextest run {{ARGS}}

# Tests the NEON block scores.
#
# Any aarch64 host runs them natively. x86_64 Linux needs the `aarch64-unknown-linux-gnu` Rust target,
# the `aarch64-linux-gnu-gcc` linker and `qemu-aarch64` with the Arm sysroot in `/usr/aarch64-linux-gnu`.
# No other host can run them.
test-neon *ARGS:
    {{ if arch() == "aarch64" {
        "cargo nextest run -p rav1oli-quantise " + ARGS
    } else if arch() + os() == "x86_64linux" {
        "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc " +
        "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER='qemu-aarch64 -L /usr/aarch64-linux-gnu' " +
        "cargo nextest run -p rav1oli-quantise --target aarch64-unknown-linux-gnu " + ARGS
    } else {
        error("the NEON tests need an aarch64 host, or x86_64 Linux with QEMU")
    } }}

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
