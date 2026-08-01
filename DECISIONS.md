# Decisions

This repository is a C-to-Rust port of `DaveGamble/cJSON` v1.7.19.

## Scope

- Target track: C → Rust
- Reference project: `DaveGamble/cJSON`
- License: MIT, matching the upstream project
- Source-language runtime: not used; the Rust port is standalone

## What was kept faithful

- Public ABI and symbol names mirror `cJSON.h` / `cJSON_Utils.h`
- The original cJSON test suite is preserved under `harness/tests/`
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

- `build.rs` compiles the pristine upstream sources with every public symbol
  prefixed `ref_` (`bench_ref_rename.h`) into `libcjson_ref_bench.a`. The port
  exports the same names via `#[no_mangle]`, so without the prefix a release
  build's codegen-units can fold the port's wrappers into the same object as
  the referenced internals, silently satisfying the externs with the port
  itself. The prefix guarantees the tests and the benchmark always call the
  real C (confirmed via `otool`/`nm` in both debug and release).
- The utils differential test replays the full json-patch-tests corpus plus
  deterministic fuzz inputs through both implementations.

## Verification strategy

- `cargo test`
- `cargo test --release`
- `cmake -S harness -B harness/build-utils -DCMAKE_BUILD_TYPE=Release -DENABLE_CJSON_UTILS=ON`
- `cmake --build harness/build-utils`
- `ctest --test-dir harness/build-utils --output-on-failure`
- `cmake -S harness -B harness/build-asan-utils -DCMAKE_BUILD_TYPE=Debug -DENABLE_CJSON_UTILS=ON "-DCMAKE_C_FLAGS=-fsanitize=address -fno-omit-frame-pointer" "-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address"`
- `cmake --build harness/build-asan-utils`
- `ASAN_OPTIONS=detect_leaks=0 ctest --test-dir harness/build-asan-utils --output-on-failure`
- `cargo run --release --bin benchmark`

## Notes for judges

- `build.rs` compiles the reference upstream C sources from `~/cjson-ref`
- The port intentionally mirrors C control flow in the core modules so the
  behavior can be audited against upstream
- `README.md` summarizes the equivalence evidence and the expected commands
