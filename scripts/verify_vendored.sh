#!/bin/sh
# Verify that the vendored reference sources are byte-identical to the
# pristine upstream cJSON sources recorded in HASHES.txt.
set -eu
cd "$(dirname "$0")/.."
if shasum -a 256 -c HASHES.txt; then
    echo "OK: vendored reference sources match the recorded checksums"
else
    echo "FAIL: a vendored reference file has been modified" >&2
    exit 1
fi
