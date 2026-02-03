#!/usr/bin/env bash
#
# Build script for claudewrapper
#
# Usage:
#   ./build.sh          - Build release binary
#   ./build.sh debug    - Build debug binary
#   ./build.sh clean    - Clean build artifacts
#   ./build.sh test     - Run tests
#   ./build.sh install  - Build and install to ~/.cargo/bin
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Project root directory (where this script lives)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Print colored status message
info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Check if Rust toolchain is installed
check_rust() {
    if ! command -v cargo &> /dev/null; then
        error "Cargo not found. Please install Rust: https://rustup.rs"
    fi
    info "Rust toolchain found: $(rustc --version)"
}

# Build release binary
build_release() {
    info "Building release binary..."
    cargo build --release
    info "Build complete: target/release/claudewrapper"
}

# Build debug binary
build_debug() {
    info "Building debug binary..."
    cargo build
    info "Build complete: target/debug/claudewrapper"
}

# Clean build artifacts
clean() {
    info "Cleaning build artifacts..."
    cargo clean
    info "Clean complete"
}

# Run tests
run_tests() {
    info "Running tests..."
    cargo test
    info "Tests complete"
}

# Install binary to ~/.cargo/bin
install_binary() {
    info "Building and installing..."
    cargo install --path .
    info "Installed to ~/.cargo/bin/claudewrapper"
}

# Main entry point
main() {
    check_rust

    case "${1:-release}" in
        release)
            build_release
            ;;
        debug)
            build_debug
            ;;
        clean)
            clean
            ;;
        test)
            run_tests
            ;;
        install)
            install_binary
            ;;
        *)
            echo "Usage: $0 {release|debug|clean|test|install}"
            exit 1
            ;;
    esac
}

main "$@"
