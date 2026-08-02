#!/bin/sh
# Verify that the harness test suite is byte-identical to the vendored
# pristine upstream cJSON test suite (which is itself pinned by the
# checksums in HASHES.txt to upstream commit fb16e5cf).
set -eu
cd "$(dirname "$0")/.."

# 1) The full tests/ tree must match byte-for-byte.
if diff -rq harness/tests vendor/cjson-ref/tests >/dev/null; then
    echo "OK: harness/tests is byte-identical to vendor/cjson-ref/tests"
else
    echo "FAIL: harness/tests differs from vendor/cjson-ref/tests" >&2
    exit 1
fi

# 2) The declarations-only shims at the harness root must match the
#    vendored reference headers / top-level test driver.
for f in cJSON.h cJSON_Utils.h test.c; do
    if cmp -s "harness/$f" "vendor/cjson-ref/$f"; then
        echo "OK: harness/$f matches vendor/cjson-ref/$f"
    else
        echo "FAIL: harness/$f differs from vendor/cjson-ref/$f" >&2
        exit 1
    fi
done
