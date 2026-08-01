# cjson-rs — a faithful Rust port of cJSON

A line-by-line, behavior-identical Rust port of
[cJSON](https://github.com/DaveGamble/cJSON) **v1.7.19** (commit
`fb16e5cf358798aabb049655975cde8427101056`), including the `cJSON_Utils`
helpers (RFC 6901 JSON pointers, RFC 6902 JSON Patch, RFC 7396 Merge Patch,
sorting, and patch generation).

The port is proven equivalent by running the **unmodified original cJSON test
suite** against the Rust implementation through a declarations-only C shim,
plus differential tests that replay the reference C code against the port.

```
cJSON.c (reference)  ──┐                 ┌──  tests/*.c  (unmodified originals)
                       │  FFI boundary   │
      Rust port ───────┼─────────────────┼──  C shim: cJSON.c declarations
                       │  #![no_mangle]  │
cJSON_Utils.c (ref) ───┘                 └──  CMake harness
```

## Results

* **22/22** tests from the original suite pass against the Rust port
  (`19` core + `3` utils), and **22/22** under AddressSanitizer.
* **34** Rust unit/integration tests pass (`cargo test`, and also under
  `cargo test --release`), including differential suites that run the port and
  the compiled reference C library side by side — asserting identical return
  codes and byte-identical output — plus deterministic fuzzers.
* **121/121** JSON-Patch corpus entries (from `json-patch-tests`) apply and
  round-trip identically to the reference.

Submission-facing docs:

* [`DECISIONS.md`](DECISIONS.md) explains the porting
  choices and verification strategy.
* [`BENCHMARKS.md`](BENCHMARKS.md) records reproducible
  performance numbers from the shipped benchmark harness.

## Layout

| Path | Purpose |
|------|---------|
| `src/model.rs` | `cJSON` struct layout matching `cJSON.h` exactly |
| `src/alloc.rs` | allocation hooks (`malloc`/`free`/`realloc`) |
| `src/parse.rs` | parser: `cJSON_Parse`, `cJSON_ParseWithOpts`, `parse_value`… |
| `src/print.rs` | printer: `cJSON_Print`, `cJSON_PrintUnformatted`, `print_value`… |
| `src/manip.rs` | create/delete/add/get/detach/replace item functions |
| `src/float.rs` | number parsing/printing (`pow10`, `dtoa`, `print_number`) |
| `src/utils.rs` | `cJSON_Utils`: pointers, patches, merge patch, sort, generate |
| `src/ffi.rs` | `#[no_mangle] extern "C"` symbols for the whole public API |
| `tests/diff_*.rs` | differential tests vs. the compiled reference |
| `harness/` | CMake harness; runs the *unmodified* original C test suite |

The Rust sources are a direct port of `cJSON.c` / `cJSON_Utils.c`; function
names, `static` helper names, comments, and control flow mirror the C files so
the mapping is auditable. Only imports, manual C-string handling, and `unsafe`
blocks differ (inherent to Rust), and every `static` C helper is `pub` in the
port so the differential tests can call both sides.

## Reference provenance

The reference sources live at `~/cjson-ref` and are **never modified**; SHA-256
hashes are recorded in `/tmp/cjson_hashes.txt`. `build.rs` compiles
`cJSON.c` + `cJSON_Utils.c` from there into `libcjson_ref_bench.a` with every
public symbol prefixed `ref_` (see `bench_ref_rename.h`): the port exports the
same names via `#[no_mangle]`, so the prefix lets the differential tests and
the benchmark link the real C alongside the port and always call the genuine
implementation (verified with `otool`/`nm`, in both debug and release builds).
`harness/tests/` contains byte-identical copies of the original `tests/*.c`;
`harness/cJSON.c` is a new **declarations-only** shim so the originals'
`#include "../cJSON.c"` resolves to declarations, with all definitions coming
from the Rust `staticlib`.

## Building and testing

```sh
# Rust tests (includes differential tests vs. the compiled reference C)
cargo test
cargo test --release

# Run the original C test suite against the Rust port
cmake -S harness -B harness/build-utils -DCMAKE_BUILD_TYPE=Release -DENABLE_CJSON_UTILS=ON
cmake --build harness/build-utils
ctest --test-dir harness/build-utils --output-on-failure

# Same, under AddressSanitizer (leak detection is disabled: broken on macOS)
cmake -S harness -B harness/build-asan-utils -DCMAKE_BUILD_TYPE=Debug \
      -DENABLE_CJSON_UTILS=ON \
      "-DCMAKE_C_FLAGS=-fsanitize=address -fno-omit-frame-pointer" \
      "-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address"
cmake --build harness/build-asan-utils
ASAN_OPTIONS=detect_leaks=0 ctest --test-dir harness/build-asan-utils --output-on-failure

# Reproducible benchmark summary vs. the reference C implementation
cargo run --release --bin benchmark
```

## Equivalence notes

* Object/array member order is preserved exactly as the C code preserves it
  (insertion order; `SortObject`/`GeneratePatches` reorder only when the C
  code does).
* `cJSON_Compare` compares object members **pairwise in order**, exactly like
  the reference — the differential tests use it to mirror the original suite,
  which does the same.
* `cJSONUtils_ApplyPatches`/`GeneratePatches` replicate the case-insensitive
  quirks of the C implementation (e.g. duplicate keys differing only by case
  are treated as the same key); the suite's case-sensitive variants
  (`…CaseSensitive`) are also ported and used by the unmodified tests.

## License

MIT — see `LICENSE`. The original cJSON copyright is retained; this port is
derived from the MIT-licensed cJSON project.
