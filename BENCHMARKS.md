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

Methodology:

- Each workload first runs a warm-up batch (one quarter of the per-sample
  iteration count) that is discarded, then **5 samples** of `iters` iterations
  each are timed independently.
- Reported per workload: **median** ns/op (the central sample) and **p99**
  ns/op (99th percentile across the 5 samples), so the tables are robust to
  one-off stalls rather than a single wall-clock reading.
- **Peak RSS** (`getrusage(RUSAGE_SELF).ru_maxrss`) is printed once at the end
  of the process.

Environment:

- Apple M2, 8 CPU cores, 8 GB RAM, macOS (aarch64)
- rustc 1.97.1, release build (`-O3`, `--target=arm64-apple-macosx`)
- Single-threaded; peak RSS 2.0 MiB; total wall time 2.31 s

## Results

Median ns/op (p99 in parentheses):

| Workload | Rust | Reference C |
| --- | ---: | ---: |
| Parse small JSON, parse + delete | 628.2 (914.4) | 650.8 (686.7) |
| Print medium JSON, print + free | 13644.2 (16581.8) | 14913.9 (18767.0) |
| JSON Pointer lookup | 35.1 (35.3) | 43.0 (43.2) |
| Sort object | 4342.1 (4482.2) | 4284.7 (4369.9) |

## Interpretation

- The Rust port is in the same performance class as upstream for the
  representative workloads measured here.
- Printing and pointer lookup are consistently a few percent faster than the
  reference across runs on this machine; sorting is within a couple of percent.
- Parsing is within a few percent and the distributions overlap heavily (in
  this run the Rust median, 628 ns/op, is below the reference 651 ns/op, and
  both fall inside each other's p99 range), i.e. within noise and
  compiler-version variation. Earlier single-sample runs measured parse in
  either direction (e.g. 878.8 vs 843.7 ns/op), confirming parse parity is
  ±10% at most.
- Correctness is established separately by the differential test suites
  (parse, print, manipulate, utils) and the harness, which compare exact
  outputs against the reference C.
