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

# run cargo tests
cargo-test:
  cargo test --workspace --locked

# format rust and toml files
fmt:
  @just cargo-fmt-check
  taplo fmt

# generate schema files for all contracts
generate-schema:
  ./scripts/schema.sh

# run optimization script
optimize:
  ./scripts/optimize.sh
