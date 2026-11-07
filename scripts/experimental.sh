#!/usr/bin/env bash
trap 'exit 0' INT TERM
ARGS="$*"
if [ "$ARGS" = "tui" ] || [ "$ARGS" = "--tui" ]; then
    echo "==> Running hq in interactive TUI mode (experimental branch)..."
    cargo build -q
    ./target/debug/hq --tui || true
elif [ "$ARGS" = "run" ]; then
    echo "==> Running hq in CLI mode (experimental branch)..."
    cargo build -q
    ./target/debug/hq || true
elif [ "$ARGS" = "test" ]; then
    echo "==> Running full workspace tests..."
    cargo test --all-targets
elif [ -n "$ARGS" ]; then
    echo "==> Running hq with args: $ARGS"
    cargo build -q
    ./target/debug/hq "$@" || true
else
    set -e
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
