#!/usr/bin/env bash
set -e

# Harness Queue (hq) Installer
REPO="Rome-NotBeepBoop/harness-queue"
INSTALL_DIR="${HOME}/.local/bin"
CARGO_BIN="${HOME}/.cargo/bin"

BOLD='\033[1m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BOLD}${CYAN}=== Harness Queue (hq) Installer ===${NC}\n"

mkdir -p "${INSTALL_DIR}"

# 1. Try cargo install if cargo is available
if command -v cargo >/dev/null 2>&1; then
    echo -e "${YELLOW}› Detected Rust/Cargo toolchain.${NC}"
    echo -e "› Installing 'hq' via cargo..."
    cargo install --git "https://github.com/${REPO}.git" --force
    if [ -f "${CARGO_BIN}/hq" ]; then
        cp -f "${CARGO_BIN}/hq" "${INSTALL_DIR}/hq" 2>/dev/null || true
    fi
else
    # 2. Try prebuilt release binary from GitHub
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    ARCH="$(uname -m)"

    case "${ARCH}" in
        x86_64|amd64) TARGET_ARCH="x86_64" ;;
        arm64|aarch64) TARGET_ARCH="aarch64" ;;
        *) TARGET_ARCH="${ARCH}" ;;
    esac

    case "${OS}" in
        darwin) TARGET_OS="apple-darwin" ;;
        linux) TARGET_OS="unknown-linux-gnu" ;;
        *) TARGET_OS="${OS}" ;;
    esac

    TARGET="${TARGET_ARCH}-${TARGET_OS}"
    RELEASE_URL="https://github.com/${REPO}/releases/latest/download/hq-${TARGET}.tar.gz"

    echo -e "› Fetching latest prebuilt binary for ${TARGET}..."
    TMP_DIR="$(mktemp -d)"
    trap 'rm -rf "${TMP_DIR}"' EXIT

    if curl -sSL -f "${RELEASE_URL}" -o "${TMP_DIR}/hq.tar.gz" 2>/dev/null; then
        tar -xzf "${TMP_DIR}/hq.tar.gz" -C "${TMP_DIR}"
        mv -f "${TMP_DIR}/hq" "${INSTALL_DIR}/hq"
        chmod +x "${INSTALL_DIR}/hq"
    else
        echo -e "${RED}! No prebuilt release binary available for ${TARGET} yet.${NC}"
        echo -e "› Please install Rust (https://rustup.rs) and re-run:"
        echo -e "    ${BOLD}cargo install --git https://github.com/${REPO}.git${NC}"
        echo -e "› Or with Homebrew:"
        echo -e "    ${BOLD}brew install --build-from-source https://raw.githubusercontent.com/${REPO}/main/Formula/hq.rb${NC}"
        exit 1
    fi
fi

# Verify installation
INSTALLED_BIN=""
if command -v hq >/dev/null 2>&1; then
    INSTALLED_BIN="$(command -v hq)"
elif [ -x "${INSTALL_DIR}/hq" ]; then
    INSTALLED_BIN="${INSTALL_DIR}/hq"
elif [ -x "${CARGO_BIN}/hq" ]; then
    INSTALLED_BIN="${CARGO_BIN}/hq"
fi

if [ -n "${INSTALLED_BIN}" ]; then
    echo -e "\n${GREEN}✓ Harness Queue (hq) installed successfully!${NC}"
    echo -e "  Location: ${INSTALLED_BIN}"
    echo -e "\n${BOLD}Quick Start:${NC}"
    echo -e "  • ${CYAN}hq${NC}          Launch default CLI mode in your terminal split"
    echo -e "  • ${CYAN}hq --tui${NC}    Launch interactive TUI mode"
    echo -e "  • ${CYAN}hq --help${NC}   View full command options\n"

    # PATH reminder if not in current PATH
    case ":$PATH:" in
        *":${INSTALL_DIR}:"*|*":${CARGO_BIN}:"*) ;;
        *)
            echo -e "${YELLOW}Note: Add ~/.local/bin or ~/.cargo/bin to your PATH:${NC}"
            echo -e "    export PATH=\"\$HOME/.local/bin:\$PATH\""
            ;;
    esac
else
    echo -e "${RED}! Could not verify 'hq' binary installation.${NC}"
    exit 1
fi
