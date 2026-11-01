

format:
    cargo +nightly fmt --all

clippy:
    cargo clippy

run *ARGS:
    cargo run --release -- {{ARGS}}