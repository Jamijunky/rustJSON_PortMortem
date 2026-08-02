#!/usr/bin/env bash
# Coverage measurement for the Rust port.
#
#   ./scripts/coverage.sh            # line/region summary across all tests
#   ./scripts/coverage.sh --html     # also emit an HTML report (target/llvm-cov/html)
#
# Requires: rustup component add llvm-tools-preview; cargo install cargo-llvm-cov
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--html" ]]; then
    shift
    cargo llvm-cov --html "$@"
    echo "HTML report: target/llvm-cov/html"
else
    exec cargo llvm-cov --summary-only "$@"
fi
