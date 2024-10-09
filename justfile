# List all recipes
default:
  just --list --unsorted

# cargo compile
cargo-compile:
	cargo test --workspace --no-run --locked

# cargo clippy check
cargo-clippy-check:
    cargo clippy --no-deps --workspace --locked --tests --benches --examples -- -Dwarnings

# cargo fmt check
cargo-fmt-check:
	cargo fmt --all --check
