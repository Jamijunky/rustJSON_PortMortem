# Reference sources — provenance and checksums

These are pristine copies of the upstream cJSON v1.7.19 reference sources
used by `build.rs`, the differential tests, and the harness. They are
byte-identical to the upstream commit and are never modified by this project.

- Upstream: https://github.com/DaveGamble/cJSON
- Commit:   `fb16e5cf358798aabb049655975cde8427101056` (v1.7.19)
- License:  MIT

`HASHES.txt` (185 SHA-256 lines) covers the **entire** vendored tree:

- the reference implementation: `cJSON.c`, `cJSON.h`, `cJSON_Utils.c`,
  `cJSON_Utils.h`, plus the top-level `test.c` test driver;
- the **complete upstream `tests/` directory** — every `tests/*.c` Unity test
  file, `tests/common.h`, `tests/CMakeLists.txt`, the `tests/unity/` framework
  (its source, docs, examples, and helper scripts), `tests/inputs/*`, and the
  whole `tests/json-patch-tests/` corpus.

Verify with: `shasum -a 256 -c HASHES.txt` (or run `scripts/verify_vendored.sh`).

## How the harness suite is pinned

The checksums pin `vendor/cjson-ref/tests/**` to the upstream commit. The
harness copies under `harness/tests/` are asserted byte-identical to that
vendored tree — and the three harness root copies (`harness/cJSON.h`,
`harness/cJSON_Utils.h`, `harness/test.c`) are asserted byte-identical to the
vendored originals — by `scripts/verify_harness.sh`:

```
harness/tests/**  ──(diff -rq, must be empty)──>  vendor/cjson-ref/tests/**  ──(SHA-256)──>  upstream fb16e5cf
harness/{cJSON.h,cJSON_Utils.h,test.c}  ──(cmp)──>  vendor/cjson-ref/{cJSON.h,cJSON_Utils.h,test.c}
```

So "the original test suite is unmodified" is mechanically checkable: the
harness suite equals the vendored suite (script-verified), and the vendored
suite equals the upstream commit (checksum-verified).
