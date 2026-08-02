#!/bin/sh
# Heavy differential fuzzing campaign against the reference C cJSON.
#
# Usage: scripts/fuzz_differential.sh [iters] [seed]
#   iters  - iterations per phase (default 1000000)
#   seed   - deterministic seed in hex (default 0xFEEDBEEF)
#
# Every generated input is replayed through the Rust port and the real C.
# Exits non-zero and writes the offending input to fuzz_fail.txt on any
# divergence.
set -eu
cd "$(dirname "$0")/.."
iters="${1:-1000000}"
seed="${2:-0xFEEDBEEF}"
cargo run --release --bin fuzz_differential -- --iters "$iters" --seed "$seed"
