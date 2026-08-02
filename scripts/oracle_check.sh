#!/usr/bin/env bash
# Independent-oracle cross-check: structural comparison of the Rust port
# against serde_json (an unrelated JSON implementation) over the vendored
# upstream corpus plus generated documents.
#
#   ./scripts/oracle_check.sh             # vendored corpus + 100k generated
#   ./scripts/oracle_check.sh --gen 500000
#
# Exit code is 0 only if no document that serde_json accepts is rejected by
# the port and no structural mismatch is found. cJSON's documented leniencies
# (number overflow, lone surrogates, raw control bytes, duplicate keys) are
# tallied and reported but are not failures.
set -euo pipefail
cd "$(dirname "$0")/.."

gen=100000
if [[ "${1:-}" == "--gen" ]]; then
    gen="$2"
fi

inputs=()
for f in vendor/cjson-ref/tests/inputs/test*; do
    case "$f" in
        *.expected) ;;
        *) inputs+=("$f") ;;
    esac
done
for f in \
    vendor/cjson-ref/tests/json-patch-tests/cjson-utils-tests.json \
    vendor/cjson-ref/tests/json-patch-tests/spec_tests.json \
    vendor/cjson-ref/tests/json-patch-tests/tests.json \
    vendor/cjson-ref/tests/json-patch-tests/package.json; do
    inputs+=("$f")
done

exec cargo run --release --example oracle_check -- "${inputs[@]}" --gen "$gen"
