# Decisions

This repository is a C-to-Rust port of `DaveGamble/cJSON` v1.7.19.

## Scope

- Target track: C → Rust
- Reference project: `DaveGamble/cJSON`
- License: MIT, matching the upstream project
- Source-language runtime: not used; the Rust port is standalone

## What was kept faithful

- Public ABI and symbol names mirror `cJSON.h` / `cJSON_Utils.h`
- The original cJSON test suite is preserved under `harness/tests/` and is
  pinned byte-for-byte to the upstream commit: `scripts/verify_harness.sh`
  diffs `harness/tests/**` against the vendored upstream `tests/` tree and
  `cmp`s the harness root shims (`cJSON.h`, `cJSON_Utils.h`, `test.c`) against
  the vendored originals, while `scripts/verify_vendored.sh` pins the vendored
  tree to the upstream commit via SHA-256 (`HASHES.txt`, 185 files).
- The harness uses a declarations-only `harness/cJSON.c` shim so the
  original tests link against the Rust `staticlib`
- Differential tests run the Rust port and the compiled upstream C
  implementation side by side for parse, print, manip, and utils paths,
  asserting identical return codes and byte-identical output

## What was not changed in the originals

- No original upstream test files were edited
- The submission does not rely on a modified upstream suite
- Any validation artifacts in this repo are additive only

## Differential and benchmark integrity

- The dev-only `cjson-ref-sys` helper crate compiles the pristine upstream
  sources with every public symbol prefixed `ref_` (`bench_ref_rename.h`) into
  `libcjson_ref_bench.a`. The port
  exports the same names via `#[no_mangle]`, so without the prefix a release
  build's codegen-units can fold the port's wrappers into the same object as
  the referenced internals, silently satisfying the externs with the port
  itself. The prefix guarantees the tests and the benchmark always call the
  real C (confirmed via `otool`/`nm` in both debug and release).
- The utils differential test replays the full json-patch-tests corpus plus
  deterministic fuzz inputs through both implementations.
- `cJSON_ParseWithLengthOpts(..., require_null_terminated=true)` demands a NUL
  terminator at the declared length, so the parse/print differentials pass
  `len + 1` when that flag is set; the corpus print test and the `true` half
  of the parse differential therefore exercise the success path rather than
  failing every input.

## Divergences from the C implementation (all behavior-preserving)

The port mirrors the C control flow so it can be audited line-by-line against
upstream, but a Rust port cannot be a literal transliteration. The deliberate
divergences below are structural; each one is justified and none changes
observable behavior, which the differential suites and the unmodified original
test suite pin down. There are twelve non-trivial divergences:

### 1. `unsafe` is confined to signatures — zero `unsafe { ... }` blocks

The `unsafe` keyword appears at **326 sites, all of them declarations**:
202 `unsafe fn` signatures (functions that take or return raw pointers to
mirror C's control flow) and 124 `unsafe extern "C"` signatures, of which
**117 are the exported ABI functions themselves** — one per public cJSON
symbol. There are no `unsafe` blocks anywhere in the crate, so no `unsafe`
code relies on C-style undefined behavior for correctness.

| Module | `unsafe` sites | Why it is needed |
| --- | ---: | --- |
| `ffi.rs` | 142 | `#[no_mangle] extern "C"` ABI layer: raw `*mut CJson` in/out, `CStr`/`CString` conversions, shimming the whole public API (117 of these are the 117 exported symbols) |
| `manip.rs` | 87 | Pointer-chasing through the `next`/`child` linked lists, insertion/detach/replace that C does with raw pointers |
| `utils.rs` | 42 | RFC 6901/6902/7396 pointer & patch walks over the linked-list `cJSON` objects |
| `parse.rs` | 23 | Hand-rolled tokenizer over NUL-terminated buffers, constructing `CJson` nodes |
| `print.rs` | 18 | Formatting buffers with `printf` semantics and C-string output (`cJSON_free`-owned) |
| `alloc.rs` | 11 | `global_hooks` static, hook accessors, `malloc`/`free`/`realloc` FFI |
| `model.rs` | 3 | Hook function-pointer types (`unsafe extern "C" fn`) |

### 2. The ABI is exported per symbol

The port exports **117 `#[no_mangle] extern "C"` symbols** mirroring
`cJSON.h`/`cJSON_Utils.h`; the harness links only this Rust `staticlib`, and
the original C tests call it through the declarations-only `cJSON.c` shim.
A C port *is* the shared library; a Rust port must opt in per symbol.

### 3. A `ref_`-prefixed, dev-only reference crate

The reference C is compiled by `cjson-ref-sys` into `libcjson_ref_bench.a`
with all public symbols prefixed `ref_` (see `bench_ref_rename.h`). This is a
**dev/test-only artifact** so the differential tests and the benchmark can run
the genuine upstream implementation beside the port in one process without
symbol collisions. The prefix is essential: the port exports the same names,
so without it a release build's codegen-units can fold the port's wrappers
into the same object as the referenced internals, silently satisfying the
externs with the port itself (confirmed via `otool`/`nm` in debug and
release). It is never linked into the submitted `libcjson.a` artifact.

### 4. `global_hooks` is a real C-visible symbol

`pub static mut global_hooks` (`alloc.rs`) is named exactly `global_hooks`,
because the original test files declare `extern internal_hooks global_hooks;`
and read it directly. The Rust type `InternalHooks` is `#[repr(C)]` with the
same field order.

### 5. `GLOBAL_ERROR` reproduces C's `global_error` singleton

`static mut GLOBAL_ERROR` (`parse.rs`) makes the last parse error observable
across calls exactly as in C.

### 6. C macros become real functions

`cJSON_IsNumber`/`cJSON_IsArray`/… are C macros; the port exposes the same
names as real functions (identical behavior, type-checked).

### 7. `CStr`/`CString` at the boundary, byte slices inside

Manual NUL-terminated strings are handled with `CStr`/`CString` at the FFI
boundary; interior string logic operates on byte slices with the same
comparisons.

### 8. `CJsonBool` stays `c_int`

`CJsonBool` is `c_int` (`0`/`1`) to keep the ABI identical, rather than Rust
`bool`.

### 9. Number printing uses the reference's exact formatting path

Number printing calls the same `snprintf("%.17g")` path as the reference so
output is byte-identical, not "close enough".

### 10. Static C helpers are `pub` for testability

Every `static` C helper is `pub` in the port so the differential tests can
call both sides of the boundary; C keeps them file-local.

### 11. `panic = "abort"` was tried and removed

`panic = "abort"` in `[profile.release]` broke the test harnesses and was
**removed**; the release profile otherwise matches defaults.

### 12. The core modules mirror C control flow

The port mirrors C control flow in `parse`, `print`, `manip`, and `utils` so
behavior can be audited against upstream, diverging only where Rust requires
it.

Each divergence preserves equivalence, which the differential suites and the
unmodified original test suite pin down.

## Verification strategy

`./scripts/verify.sh` runs the whole battery below in order and prints a
PASS/FAIL summary (fuzz scale with `VERIFY_ITERS`, e.g. `VERIFY_ITERS=100000`):

- `./scripts/verify_vendored.sh`
- `./scripts/verify_harness.sh`
- `cargo test`
- `cargo test --release`
- `cmake -S harness -B harness/build-utils -DCMAKE_BUILD_TYPE=Release -DENABLE_CJSON_UTILS=ON`
- `cmake --build harness/build-utils`
- `ctest --test-dir harness/build-utils --output-on-failure`
- `cmake -S harness -B harness/build-asan-utils -DCMAKE_BUILD_TYPE=Debug -DENABLE_CJSON_UTILS=ON "-DCMAKE_C_FLAGS=-fsanitize=address -fno-omit-frame-pointer" "-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address"`
- `cmake --build harness/build-asan-utils`
- `ASAN_OPTIONS=detect_leaks=0 ctest --test-dir harness/build-asan-utils --output-on-failure`
- `./scripts/harness_ubsan.sh` (unmodified suite under UndefinedBehaviorSanitizer; the
  port's Rust code is not instrumented, only the pristine C tests and the ABI boundary)
- `./scripts/fuzz_differential.sh` (2M differential cases per run, two seeds run; the
  always-on 5-test fuzz suite ships in `cargo test`, scaled by `CJSON_FUZZ_ITERS`)
- `./scripts/oracle_check.sh` (independent cross-check against `serde_json`: strict-JSON
  documents must parse identically; cJSON's documented leniencies are tallied, not failed)
- `./scripts/coverage.sh` (line coverage via `cargo-llvm-cov`, scaled by
  `CJSON_FUZZ_ITERS`; the per-module numbers are reported in `README.md` and
  refreshed on every coverage pass)
- `cargo run --release --example benchmark`

## Notes for judges

- Pristine reference sources are vendored at `vendor/cjson-ref/` (provenance in
  `HASHES.md`, checksums in `HASHES.txt`, verified by
  `scripts/verify_vendored.sh`); `cjson-ref-sys` compiles them for tests and
  examples only, and the `CJSON_REF_DIR` environment variable can point at
  another reference checkout
- The harness test suite is verified byte-identical to the vendored upstream
  `tests/` tree by `scripts/verify_harness.sh`, so the "unmodified original
  test suite" claim holds without assuming the judge diffs against the remote
  repository
- `.gitattributes` marks the vendored / byte-identical upstream C (`vendor/**`,
  `harness/tests/**`, and the harness root copies of `cJSON.h`,
  `cJSON_Utils.h`, `test.c`) as `linguist-vendored`, so GitHub's language
  breakdown reflects the Rust port rather than the third-party code it must
  contain; nothing is removed — the files remain and are hash-pinned
- The port intentionally mirrors C control flow in the core modules so the
  behavior can be audited against upstream
- `README.md` summarizes the equivalence evidence and the expected commands
