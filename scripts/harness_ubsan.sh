#!/usr/bin/env bash
# Run the upstream cJSON test suite compiled with UndefinedBehaviorSanitizer
# against the Rust port's staticlib. Exercises the pristine C tests plus the
# Rust/C ABI boundary; the port's Rust code itself is not instrumented.
#
# Requires: cmake, clang, and a `cargo build --release` of the port (the
# harness CMake triggers it automatically).
set -euo pipefail
cd "$(dirname "$0")/.."

for variant in "build-ubsan" "build-ubsan-utils"; do
    flags=(-DENABLE_CJSON_TEST=ON)
    if [[ "$variant" == *-utils ]]; then
        flags+=(-DENABLE_CJSON_UTILS=ON)
    fi
    rm -rf "harness/$variant"
    cmake -S harness -B "harness/$variant" \
        -DCMAKE_BUILD_TYPE=Debug \
        -DCMAKE_C_FLAGS="-fsanitize=undefined -fno-omit-frame-pointer" \
        "${flags[@]}"
    cmake --build "harness/$variant" -j"$(sysctl -n hw.ncpu 2>/dev/null || echo 8)"
    (cd "harness/$variant" && ctest --output-on-failure)
done
