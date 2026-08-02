//! Differential tests: parse the same inputs with the reference C cJSON and
//! with the Rust port, and require identical trees and error positions.

use std::ffi::{c_char, c_int};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use cjson::manip::cjson_delete;
use cjson::model::CJson;
use cjson::parse::{cjson_parse_with_length_opts, get_error_ptr};
use cjson_ref_sys as _;

/// The reference C cJSON keeps global parse state (`global_error`,
/// `global_hooks`). Serialize every differential run so parallel test threads
/// cannot race on that state.
fn with_lock<R>(f: impl FnOnce() -> R) -> R {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();
    f()
}

// ---- reference C cJSON public API -----------------------------------------

#[link(name = "cjson_ref_bench")]
unsafe extern "C" {
    fn ref_cJSON_ParseWithLengthOpts(
        value: *const c_char,
        buffer_length: usize,
        return_parse_end: *mut *const c_char,
        require_null_terminated: c_int,
    ) -> *mut CJson;
    fn ref_cJSON_Delete(item: *mut CJson);
    fn ref_cJSON_GetErrorPtr() -> *const c_char;
}

fn cstr_bytes(p: *const c_char) -> Option<Vec<u8>> {
    if p.is_null() {
        return None;
    }
    let mut v = Vec::new();
    let mut i = 0usize;
    unsafe {
        while *p.add(i) != 0 {
            v.push(*p.add(i) as u8);
            i += 1;
        }
    }
    Some(v)
}

/// Recursively compare two parsed trees (both use the same `CJson` layout).
fn assert_trees_equal(ours: *const CJson, refs: *const CJson, path: &str) {
    if ours.is_null() || refs.is_null() {
        assert_eq!(ours.is_null(), refs.is_null(), "null mismatch at {path}");
        return;
    }
    let o = unsafe { &*ours };
    let r = unsafe { &*refs };
    assert_eq!(o.type_, r.type_, "type mismatch at {path}");
    assert_eq!(
        o.valueint, r.valueint,
        "valueint mismatch at {path}: {} vs {}",
        o.valueint, r.valueint
    );
    assert_eq!(
        o.valuedouble.to_bits(),
        r.valuedouble.to_bits(),
        "valuedouble mismatch at {path}: {} vs {}",
        o.valuedouble,
        r.valuedouble
    );
    assert_eq!(
        cstr_bytes(o.valuestring),
        cstr_bytes(r.valuestring),
        "valuestring mismatch at {path}"
    );
    assert_eq!(
        cstr_bytes(o.string),
        cstr_bytes(r.string),
        "string(key) mismatch at {path}"
    );

    // Compare children as ordered lists.
    let mut oc = o.child;
    let mut rc = r.child;
    let mut idx = 0usize;
    loop {
        let o_end = oc.is_null();
        let r_end = rc.is_null();
        assert_eq!(o_end, r_end, "child count mismatch at {path}[{idx}]");
        if o_end {
            break;
        }
        assert_trees_equal(oc, rc, &format!("{path}[{idx}]"));
        oc = unsafe { (*oc).next };
        rc = unsafe { (*rc).next };
        idx += 1;
    }

    // next/prev linkage should agree on whether it is NULL.
    assert_eq!(
        unsafe { (*ours).next }.is_null(),
        unsafe { (*refs).next }.is_null(),
        "next linkage mismatch at {path}"
    );
    assert_eq!(
        unsafe { (*ours).prev }.is_null(),
        unsafe { (*refs).prev }.is_null(),
        "prev linkage mismatch at {path}"
    );
}

fn run_case(input: &[u8], require_null_terminated: bool) {
    let input_c = {
        let mut v = input.to_vec();
        v.push(0);
        v
    };
    let mut ours_end: *const c_char = ptr::null();
    let mut refs_end: *const c_char = ptr::null();

    let ours = unsafe {
        cjson_parse_with_length_opts(
            input_c.as_ptr() as *const c_char,
            input.len(),
            &mut ours_end,
            require_null_terminated as c_int,
        )
    };
    let refs = unsafe {
        ref_cJSON_ParseWithLengthOpts(
            input_c.as_ptr() as *const c_char,
            input.len(),
            &mut refs_end,
            require_null_terminated as c_int,
        )
    };

    let input_disp = String::from_utf8_lossy(input);
    if ours.is_null() != refs.is_null() {
        unsafe { cjson_delete(ours) };
        unsafe { ref_cJSON_Delete(refs) };
        panic!(
            "parse outcome mismatch for {:?}: ours={} refs={}",
            input_disp,
            ours.is_null(),
            refs.is_null()
        );
    }

    if !ours.is_null() {
        assert_trees_equal(ours, refs, "root");
    }

    // Error position and parse-end must agree.
    let ours_err = unsafe { get_error_ptr() };
    let refs_err = unsafe { ref_cJSON_GetErrorPtr() };
    let base = input_c.as_ptr() as *const c_char;
    let ours_err_off = if ours_err.is_null() {
        None
    } else {
        Some(unsafe { ours_err.offset_from(base) })
    };
    let refs_err_off = if refs_err.is_null() {
        None
    } else {
        Some(unsafe { refs_err.offset_from(base) })
    };
    assert_eq!(
        ours_err_off, refs_err_off,
        "error pointer offset mismatch for {:?}",
        input_disp
    );

    let ours_end_off = if ours_end.is_null() {
        None
    } else {
        Some(unsafe { ours_end.offset_from(base) })
    };
    let refs_end_off = if refs_end.is_null() {
        None
    } else {
        Some(unsafe { refs_end.offset_from(base) })
    };
    assert_eq!(
        ours_end_off, refs_end_off,
        "return_parse_end offset mismatch for {:?}",
        input_disp
    );

    unsafe { cjson_delete(ours) };
    unsafe { ref_cJSON_Delete(refs) };
}

fn corpus() -> Vec<Vec<u8>> {
    let mut inputs: Vec<Vec<u8>> = Vec::new();

    // Everything under <ref>/tests/inputs (non-expected files).
    let inputs_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/cjson-ref/tests/inputs");
    if let Ok(entries) = std::fs::read_dir(&inputs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".expected") {
                inputs.push(std::fs::read(&path).unwrap_or_default());
            }
        }
    }
    assert!(
        inputs_dir.is_dir(),
        "vendored reference inputs not found at {}",
        inputs_dir.display()
    );
    assert!(
        !inputs.is_empty(),
        "vendored reference inputs directory is empty"
    );

    // A curated set of edge cases.
    let mut extra: Vec<String> = vec![
        // literals
        "null",
        "true",
        "false",
        "",
        "  ",
        "nul",
        "tru",
        "fals",
        "nullx",
        "truex",
        // numbers
        "0",
        "-0",
        "0.0",
        "-0.0",
        "1",
        "-1",
        "1.5",
        "-1.5",
        "1e5",
        "1E5",
        "1e-5",
        "1e+5",
        "1.5e300",
        "-1.5e-300",
        "1e999",
        "-1e999",
        "12345678901234567890123",
        "2147483647",
        "2147483648",
        "-2147483648",
        "-2147483649",
        "1e-400",
        "0.0000000000000000001",
        "3.141592653589793",
        "1.",
        ".5",
        "+1",
        "01",
        "1e",
        "1e+",
        "-",
        "+",
        "0x10",
        "1.2.3",
        "abc",
        "1e309",
        // strings
        "\"\"",
        "\"hello\"",
        "\"a\\\"b\"",
        "\"tab\\tnewline\\n\"",
        "\"\\\\\"",
        "\"\\/\"",
        "\"\\b\\f\\r\"",
        "\"\\u0041\"",
        "\"\\u00e9\"",
        "\"\\uD83D\\uDE00\"",
        "\"\\uD800\"",
        "\"\\uDC00\"",
        "\"\\uD83D\\uZZZZ\"",
        "\"unterminated",
        "\"",
        "\"a\\",
        "\"\\uD83D\\u\"",
        "invalid\"",
        "\"\\u1\"",
        "\"\\u123\"",
        // arrays / objects
        "[]",
        "{}",
        "[1,2,3]",
        "[ ]",
        "[1,2,]",
        "[1,2,3",
        "[[[[]]]]",
        "[\"a\",\"b\"]",
        "{\"a\":1}",
        "{\"a\":1,}",
        "{ }",
        "{\"a\"}",
        "{\"a\":}",
        "{:1}",
        "{1:2}",
        "{\"a\":1,\"b\":[true,false,null]}",
        "{\"a\" : 1}",
        "{\"\\u0061\":1}",
        "{\"\":0}",
        "{\"a\":1,\"a\":2}",
        // whitespace / BOM / mixed
        "  {\"a\":1}  ",
        "\u{feff}{\"a\":1}",
        "\t\r\n [1]\t",
        "null null",
        "[null, null]",
        "{}\n{}",
        "1 2",
        // raw edge cases
        "\"\\uFFFF\"",
        "\"\\uDBFF\\uDFFF\"",
        "\"\\uD800\\uDC00\"",
        "[1e999]",
        "{\"x\":1e999}",
        "{\"n\":null}",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    extra.push(format!("[{}]", "[1],".repeat(500)));
    extra.push(format!("[{}]", "[1],".repeat(1100)));
    for s in extra {
        inputs.push(s.into_bytes());
    }
    inputs
}

#[test]
fn differential_parse_against_reference() {
    with_lock(|| {
        for require_null_terminated in [false, true] {
            for input in corpus() {
                run_case(&input, require_null_terminated);
            }
        }
    });
}

// ---- deterministic fuzzing -------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Generate a small random JSON document from a grammar.
fn random_json(rng: &mut Rng, depth: usize) -> Vec<u8> {
    let mut out = Vec::new();
    match rng.below(if depth > 3 { 6 } else { 7 }) {
        0 => out.extend_from_slice(&[&b"null"[..], &b"true"[..], &b"false"[..]][rng.below(3)]),
        1 => {
            // number
            out.extend_from_slice(b"-".repeat(rng.below(2)).as_slice());
            out.extend_from_slice(b"0".repeat(1 + rng.below(4)).as_slice());
            if rng.below(2) == 0 {
                out.push(b'.');
                out.extend_from_slice(b"0".repeat(1 + rng.below(5)).as_slice());
            }
            if rng.below(2) == 0 {
                out.push(if rng.below(2) == 0 { b'e' } else { b'E' });
                if rng.below(2) == 0 {
                    out.push(if rng.below(2) == 0 { b'+' } else { b'-' });
                }
                out.extend_from_slice(b"0".repeat(1 + rng.below(3)).as_slice());
            }
        }
        2 => {
            // string
            out.push(b'"');
            for _ in 0..rng.below(6) {
                match rng.below(5) {
                    0 => out.push(b'a' + (rng.below(26) as u8)),
                    1 => {
                        let esc: &[u8] = b"\\\"\\n\\t\\\\\\/\\b\\f\\r";
                        let mut e = vec![b'\\'];
                        e.push(esc[1 + 2 * rng.below(7)]);
                        out.extend_from_slice(&e);
                    }
                    2 => out.extend_from_slice(b"\\u"),
                    3 => {
                        for _ in 0..4 {
                            out.push(b"0123456789abcdefABCDEF"[rng.below(22)]);
                        }
                    }
                    4 => out.push(0x7f - rng.below(4) as u8),
                    _ => unreachable!(),
                }
            }
            out.push(b'"');
        }
        3 => {
            // array
            out.push(b'[');
            for i in 0..rng.below(4) {
                if i > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(random_json(rng, depth + 1).as_slice());
            }
            if rng.below(4) == 0 {
                out.push(b',');
            }
            out.push(b']');
        }
        4 => {
            // object
            out.push(b'{');
            for i in 0..rng.below(4) {
                if i > 0 {
                    out.push(b',');
                }
                out.push(b'"');
                out.push(b'k' + (i as u8));
                out.push(b'"');
                out.push(b':');
                out.extend_from_slice(random_json(rng, depth + 1).as_slice());
            }
            out.push(b'}');
        }
        5 => out.extend_from_slice(random_json(rng, depth + 1).as_slice()),
        6 => out.extend_from_slice(random_json(rng, depth + 1).as_slice()),
        _ => unreachable!(),
    }
    out
}

#[test]
fn differential_parse_fuzz() {
    with_lock(|| {
        let seed = 0xC0FFEEu64;
        let mut rng = Rng(seed);

        for i in 0..50_000u64 {
            let mut input = if i % 2 == 0 {
                random_json(&mut rng, 0)
            } else {
                // pure garbage
                let len = rng.below(32);
                (0..len).map(|_| rng.below(256) as u8).collect()
            };
            // occasionally mutate: insert/delete/corrupt a byte
            match rng.below(4) {
                0 => {} // keep
                1 if !input.is_empty() => {
                    let idx = rng.below(input.len());
                    input[idx] = rng.below(256) as u8;
                }
                2 => {
                    let idx = rng.below(input.len() + 1);
                    input.insert(idx, rng.below(256) as u8);
                }
                3 if input.len() > 1 => {
                    let idx = rng.below(input.len());
                    input.remove(idx);
                }
                _ => {}
            }

            let require_null_terminated = rng.below(2) == 0;
            run_case(&input, require_null_terminated);
        }
    });
}
