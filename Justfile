# Harness Queue (hq) Justfile
# Use `just` or `just run` to run hq directly without manually installing binaries.

# Default: run hq directly with cargo
default *args:
    @cargo run -q -- {{args}}

# List available recipes
help:
    @just --list

# Run hq in debug mode (passes all arguments through)
run *args:
    @cargo run -q -- {{args}}

# Run hq in release mode
run-release *args:
    @cargo run -q --release -- {{args}}

# Run the full test suite
test *filter:
    cargo test -- {{filter}}

# Run all 20 end-to-end integration scenarios
test-scenarios:
    cargo test --test test_all_scenarios

# Run Orca IDE integration tests
test-orca:
    cargo test --test test_orca_adapter

# Run Ghostty integration tests
test-ghostty:
    cargo test --test test_ghostty_adapter

# Run Herdr integration tests
test-herdr:
    cargo test --test test_herdr_adapter

# Run CLI UX tests
test-cli:
    cargo test --test test_cli_ux

# Run Codex harness tests
test-codex:
    cargo test --test test_codex_harness

# Check code for compilation errors and lints
check:
    cargo check
    cargo clippy --all-targets

# Format code with rustfmt
fmt:
    cargo fmt --all

# Build release binary (when ready for distribution)
build:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean
