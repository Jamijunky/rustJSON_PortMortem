#!/usr/bin/env bash
# One-command reproduction of the full verification battery.
#
#   ./scripts/verify.sh                   # full run (fuzz uses 1M iters/phase)
#   VERIFY_ITERS=100000 ./scripts/verify.sh  # quicker fuzz phase
#
# Runs every check listed in DECISIONS.md, continues past failures, and prints
# a PASS/FAIL summary at the end. Exits non-zero if any check failed.
# `coverage.sh` is reported as SKIP (not a failure) when `cargo-llvm-cov` is
# not installed.
#
# Requires: cmake, clang (for the UBSan run), and the Rust toolchain.
set -uo pipefail
cd "$(dirname "$0")/.."

iters="${VERIFY_ITERS:-1000000}"
ncpu="$(sysctl -n hw.ncpu 2>/dev/null || echo 8)"

results=()
pass=0
fail=0
skip=0

run() {
    local label="$1"
    shift
    if "$@"; then
        results+=("PASS  $label")
        pass=$((pass + 1))
        printf 'PASS  %s\n' "$label"
    else
        results+=("FAIL  $label")
        fail=$((fail + 1))
        printf 'FAIL  %s\n' "$label"
    fi
}

harness_release() {
    rm -rf harness/build-utils
    cmake -S harness -B harness/build-utils -DCMAKE_BUILD_TYPE=Release -DENABLE_CJSON_UTILS=ON
    cmake --build harness/build-utils -j"$ncpu"
    ctest --test-dir harness/build-utils --output-on-failure
}

harness_asan() {
    rm -rf harness/build-asan-utils
    cmake -S harness -B harness/build-asan-utils -DCMAKE_BUILD_TYPE=Debug -DENABLE_CJSON_UTILS=ON \
        "-DCMAKE_C_FLAGS=-fsanitize=address -fno-omit-frame-pointer" \
        "-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address"
    cmake --build harness/build-asan-utils -j"$ncpu"
    ASAN_OPTIONS=detect_leaks=0 ctest --test-dir harness/build-asan-utils --output-on-failure
}

run "cargo fmt --check" cargo fmt --check
run "verify_vendored.sh" ./scripts/verify_vendored.sh
run "verify_harness.sh" ./scripts/verify_harness.sh
run "cargo test" cargo test
run "cargo test --release" cargo test --release
run "harness release (22 tests)" harness_release
run "harness ASan (22 tests)" harness_asan
run "harness UBSan (22 tests)" ./scripts/harness_ubsan.sh
run "fuzz_differential.sh (${iters} iters/phase)" ./scripts/fuzz_differential.sh "$iters"
run "oracle_check.sh" ./scripts/oracle_check.sh

if command -v cargo-llvm-cov >/dev/null 2>&1; then
    run "coverage.sh" ./scripts/coverage.sh
else
    results+=("SKIP  coverage.sh (cargo-llvm-cov not installed)")
    skip=$((skip + 1))
    printf 'SKIP  coverage.sh (cargo-llvm-cov not installed)\n'
fi

printf '\n==== verify.sh summary ====\n'
for r in "${results[@]}"; do
    printf '%s\n' "$r"
done
printf '%d passed, %d failed, %d skipped\n' "$pass" "$fail" "$skip"

[[ "$fail" -eq 0 ]]
