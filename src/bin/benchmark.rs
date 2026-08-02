use core::ffi::{c_char, c_int, c_void};
use std::hint::black_box;
use std::time::Instant;

use cjson::alloc::{cjson_free, current_hooks};
use cjson::manip::cjson_delete;
use cjson::model::CJson;
use cjson::parse::cjson_parse_with_length_opts;
use cjson::print::cjson_print_unformatted;
use cjson::utils::{
    cjson_utils_get_pointer_case_sensitive, cjson_utils_sort_object_case_sensitive,
};

// The reference C library, compiled by build.rs from the pristine upstream
// sources with every public symbol prefixed `ref_` (see bench_ref_rename.h),
// so these externs resolve to the real C even though the port exports the
// same names via `#[no_mangle]`. The port side is called through its mangled
// internal entry points below.
#[link(name = "cjson_ref_bench")]
unsafe extern "C" {
    fn ref_cJSON_ParseWithLengthOpts(
        value: *const c_char,
        buffer_length: usize,
        return_parse_end: *mut *const c_char,
        require_null_terminated: c_int,
    ) -> *mut CJson;
    fn ref_cJSON_Delete(item: *mut CJson);
    fn ref_cJSON_PrintUnformatted(item: *const CJson) -> *mut c_char;
    fn ref_cJSONUtils_GetPointerCaseSensitive(
        object: *mut CJson,
        pointer: *const c_char,
    ) -> *mut CJson;
    fn ref_cJSONUtils_SortObjectCaseSensitive(object: *mut CJson);
    fn free(ptr: *mut c_void);
    // POSIX getrusage(2); on macOS ru_maxrss is in bytes. The struct is only
    // indexed for ru_maxrss (the 5th field: two timevals, then ru_maxrss).
    fn getrusage(who: c_int, usage: *mut Rusage) -> c_int;
}

#[repr(C)]
struct Rusage([i64; 16]);

fn peak_rss_bytes() -> u64 {
    let mut usage = Rusage([0; 16]);
    unsafe {
        getrusage(0, &mut usage);
    }
    usage.0[4] as u64
}

const SAMPLES: usize = 5;

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx]
}

fn with_nul(s: &str) -> Vec<u8> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn medium_json() -> String {
    let mut s = String::from("{\"items\":[");
    for i in 0..64 {
        if i != 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"id\":{},\"name\":\"item-{:02}\",\"value\":{},\"flag\":{}}}",
            i,
            i,
            i * 17,
            if i % 2 == 0 { "true" } else { "false" }
        ));
    }
    s.push_str("],\"meta\":{\"name\":\"benchmark\",\"count\":64,\"active\":true,\"tags\":[\"a\",\"b\",\"c\"]}}");
    s
}

fn unsorted_json() -> String {
    let mut s = String::from("{");
    for i in (0..48).rev() {
        if i != 47 {
            s.push(',');
        }
        s.push_str(&format!("\"key{:02}\":{}", i, i));
    }
    s.push('}');
    s
}

fn rust_parse(input: &[u8]) -> *mut CJson {
    let mut end: *const c_char = core::ptr::null();
    unsafe {
        cjson_parse_with_length_opts(
            input.as_ptr() as *const c_char,
            input.len() - 1,
            &mut end,
            0,
        )
    }
}

fn ref_parse(input: &[u8]) -> *mut CJson {
    let mut end: *const c_char = core::ptr::null();
    unsafe {
        ref_cJSON_ParseWithLengthOpts(
            input.as_ptr() as *const c_char,
            input.len() - 1,
            &mut end,
            0,
        )
    }
}

fn rust_print(item: *const CJson) -> *mut c_char {
    unsafe { cjson_print_unformatted(item) }
}

fn ref_print(item: *const CJson) -> *mut c_char {
    unsafe { ref_cJSON_PrintUnformatted(item) }
}

fn rust_lookup(object: *mut CJson, pointer: *const c_char) -> *mut CJson {
    unsafe { cjson_utils_get_pointer_case_sensitive(object, pointer as *const u8) }
}

fn ref_lookup(object: *mut CJson, pointer: *const c_char) -> *mut CJson {
    unsafe { ref_cJSONUtils_GetPointerCaseSensitive(object, pointer) }
}

fn rust_sort(object: *mut CJson) {
    unsafe { cjson_utils_sort_object_case_sensitive(object) }
}

fn ref_sort(object: *mut CJson) {
    unsafe { ref_cJSONUtils_SortObjectCaseSensitive(object) }
}

fn measure(label: &str, iters: usize, mut f: impl FnMut()) {
    let warmup = (iters / 4).max(1);
    for _ in 0..warmup {
        f();
    }
    let mut samples: Vec<f64> = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        samples.push(start.elapsed().as_nanos() as f64 / iters as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[SAMPLES / 2];
    let p99 = percentile(&samples, 99.0);
    let total_ms = samples.iter().sum::<f64>() * iters as f64 / 1_000_000.0;
    println!(
        "{label:<30} {SAMPLES} x {iters:>8} iters  median {median:>10.1} ns/op  p99 {p99:>10.1} ns/op  {total_ms:>10.2} ms"
    );
}

fn main() {
    let small = with_nul(
        r#"{"name":"cjson","version":1,"items":[1,2,3,4],"ok":true,"nested":{"a":1,"b":"x"}}"#,
    );
    let medium = with_nul(&medium_json());
    let unsorted = with_nul(&unsorted_json());
    let pointer = with_nul("/items/37/name");

    let started = Instant::now();
    println!("cjson-rs benchmark harness");
    println!(
        "release build, single-threaded, warm-up + {SAMPLES} samples per workload (median / p99)"
    );

    measure("rust parse small", 40_000, || {
        let item = rust_parse(&small);
        black_box(item);
        if !item.is_null() {
            unsafe { cjson_delete(item) };
        }
    });

    measure("ref  parse small", 40_000, || {
        let item = ref_parse(&small);
        black_box(item);
        if !item.is_null() {
            unsafe { ref_cJSON_Delete(item) };
        }
    });

    let rust_print_tree = rust_parse(&medium);
    let ref_print_tree = ref_parse(&medium);
    measure("rust print medium", 10_000, || {
        let out = rust_print(rust_print_tree);
        black_box(out);
        if !out.is_null() {
            let hooks = unsafe { current_hooks() };
            unsafe { cjson_free(&hooks, out as *mut c_void) };
        }
    });
    measure("ref  print medium", 10_000, || {
        let out = ref_print(ref_print_tree);
        black_box(out);
        if !out.is_null() {
            unsafe { free(out as *mut c_void) };
        }
    });
    if !rust_print_tree.is_null() {
        unsafe { cjson_delete(rust_print_tree) };
    }
    if !ref_print_tree.is_null() {
        unsafe { ref_cJSON_Delete(ref_print_tree) };
    }

    let rust_lookup_tree = rust_parse(&medium);
    let ref_lookup_tree = ref_parse(&medium);
    measure("rust pointer lookup", 200_000, || {
        let found = rust_lookup(rust_lookup_tree, pointer.as_ptr() as *const c_char);
        black_box(found);
    });
    measure("ref  pointer lookup", 200_000, || {
        let found = ref_lookup(ref_lookup_tree, pointer.as_ptr() as *const c_char);
        black_box(found);
    });
    if !rust_lookup_tree.is_null() {
        unsafe { cjson_delete(rust_lookup_tree) };
    }
    if !ref_lookup_tree.is_null() {
        unsafe { ref_cJSON_Delete(ref_lookup_tree) };
    }

    measure("rust sort object", 8_000, || {
        let item = rust_parse(&unsorted);
        if !item.is_null() {
            rust_sort(item);
            black_box(item);
            unsafe { cjson_delete(item) };
        }
    });

    measure("ref  sort object", 8_000, || {
        let item = ref_parse(&unsorted);
        if !item.is_null() {
            ref_sort(item);
            black_box(item);
            unsafe { ref_cJSON_Delete(item) };
        }
    });

    let total_s = started.elapsed().as_secs_f64();
    println!(
        "peak RSS: {:.1} MiB   total wall time: {:.2} s",
        peak_rss_bytes() as f64 / (1024.0 * 1024.0),
        total_s
    );
}
