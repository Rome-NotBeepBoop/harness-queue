#!/usr/bin/env -S just --justfile
# Harness Queue (hq) Justfile
# Use `just` or `just run` to run hq directly without manually installing binaries.

# Default: run hq directly with cargo
default *args:
    @-cargo run -q -- {{args}}

# List available recipes
help:
    @just --list

# Test the experimental branch: run feature tests, full suite, compile, and show interactive instructions
# Pass subcommands like `tui`, `run`, or `test` to test drive: `just experimental tui`
experimental *args:
    #!/usr/bin/env bash
    set -e
    ARGS="{{args}}"
    if [ "$ARGS" = "tui" ] || [ "$ARGS" = "--tui" ]; then
        echo "==> Running hq in interactive TUI mode (experimental branch)..."
        cargo run -- --tui
    elif [ "$ARGS" = "run" ]; then
        echo "==> Running hq in CLI mode (experimental branch)..."
        cargo run --
    elif [ "$ARGS" = "test" ]; then
        echo "==> Running full workspace tests..."
        cargo test --all-targets
    elif [ -n "$ARGS" ]; then
        echo "==> Running hq with args: $ARGS"
        cargo run -- {{args}}
    else
        echo "========================================================================"
        echo "  Testing experimental branch: Terminal Fallback & Agent Session Picker"
        echo "========================================================================"
        BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
        echo "Git branch: $BRANCH"
        if [ "$BRANCH" != "experimental" ]; then
            echo "Notice: Current branch is '$BRANCH', not 'experimental'."
            echo "Run 'git checkout experimental' to switch."
        fi
        echo ""
        echo "==> Step 1/3: Running experimental unit & integration tests..."
        cargo test --test test_windows_compatibility \
                   --test test_session_discovery \
                   --test test_fallback_adapter \
                   --test test_cli_fallback_selection
        echo ""
        echo "==> Step 2/3: Running full workspace test suite (all 85+ tests)..."
        cargo test --all-targets
        echo ""
        echo "==> Step 3/3: Compiling debug binary..."
        cargo build
        echo ""
        echo "========================================================================"
        echo "✓ Experimental branch verified! All tests passed and binary is built."
        echo ""
        echo "To test interactively in your terminal:"
        echo "  just experimental tui         # Test interactive TUI fallback picker"
        echo "  just experimental run         # Test CLI fallback prompt queue"
        echo "  just experimental --clipboard # Test fallback with clipboard sync"
        echo "========================================================================"
    fi

# Run hq in debug mode (passes all arguments through)
run *args:
    @-cargo run -q -- {{args}}

# Run hq in interactive TUI mode
tui *args:
    @-cargo run -q -- --tui {{args}}

# Run hq in release mode
run-release *args:
    @-cargo run -q --release -- {{args}}

# Run the full test suite
test *filter:
    cargo test -- {{filter}}

# Run all 20 end-to-end integration scenarios
test-scenarios:
    cargo test --test test_all_scenarios

# Run terminal fallback adapter tests
test-fallback:
    cargo test --test test_fallback_adapter

# Run session discovery tests (AGY SQLite, Codex JSONL)
test-discovery:
    cargo test --test test_session_discovery

# Run Windows Terminal compatibility tests
test-windows:
    cargo test --test test_windows_compatibility

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

# Compile release binary to target/release/hq
compile:
    cargo build --release

# Build release binary (alias for compile)
build:
    cargo build --release

# Install hq to ~/.local/bin and ~/.cargo/bin
install:
    cargo build --release
    @mkdir -p ~/.local/bin
    @cp -f target/release/hq ~/.local/bin/hq
    @cargo install --path . --force
    @echo "✓ Successfully installed 'hq'!"
    @echo "  Run 'hq' or 'hq --tui' in any terminal."

# Uninstall hq binary
uninstall:
    @cargo uninstall harness-queue 2>/dev/null || true
    @rm -f ~/.local/bin/hq
    @echo "✓ Successfully uninstalled 'hq'."

# Clean build artifacts
clean:
    cargo clean
