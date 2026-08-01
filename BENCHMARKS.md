# Benchmarks

Reproducible benchmark command:

```sh
cargo run --release --bin benchmark
```

The reference C side is linked from the pristine upstream sources compiled
with every public symbol prefixed `ref_` (see `bench_ref_rename.h`), so the
Rust port (called through its internal entry points) and the real C run in
the same process. This was verified with `otool`/`nm`: `ref_cJSON_ParseWithLengthOpts`
is the real C parser, not a branch into the port.

Environment:

- Apple M2, 8 CPU cores, 8 GB RAM, macOS (aarch64)
- rustc 1.97.1, release build (`-O3`, `--target=arm64-apple-macosx`)
- Single-threaded, one sample per workload

## Results

| Workload | Rust | Reference C |
| --- | ---: | ---: |
| Parse small JSON, parse + delete | 878.8 ns/op | 843.7 ns/op |
| Print medium JSON, print + free | 13677.1 ns/op | 15115.9 ns/op |
| JSON Pointer lookup | 35.1 ns/op | 43.0 ns/op |
| Sort object | 4111.3 ns/op | 4278.6 ns/op |

## Interpretation

- The Rust port is in the same performance class as upstream for the
  representative workloads measured here.
- Printing, pointer lookup, and sorting are consistently a few percent faster
  than the reference across runs on this machine.
- Parsing is within roughly ±10% in either direction across runs (observed
  ranges overlap: Rust 0.73–0.89 µs/op, reference 0.76–0.84 µs/op), i.e.
  within noise and compiler-version variation.
- Correctness is established separately by the differential test suites
  (parse, print, manipulate, utils) and the harness, which compare exact
  outputs against the reference C.
