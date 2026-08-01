//! Differential tests: parse the same inputs with the reference C cJSON and
//! with the Rust port, then require the printed output to be byte-identical.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use cjson::manip::cjson_delete;
use cjson::model::CJson;
use cjson::parse::cjson_parse_with_length_opts;
use cjson::print::{
    cjson_print, cjson_print_buffered, cjson_print_preallocated, cjson_print_unformatted,
};

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
    fn ref_cJSON_Print(item: *const CJson) -> *mut c_char;
    fn ref_cJSON_PrintUnformatted(item: *const CJson) -> *mut c_char;
    fn ref_cJSON_PrintBuffered(item: *const CJson, prebuffer: c_int, fmt: c_int) -> *mut c_char;
    fn ref_cJSON_PrintPreallocated(
        item: *mut CJson,
        buffer: *mut c_char,
        length: c_int,
        fmt: c_int,
    ) -> c_int;
    fn ref_cJSON_free(ptr: *mut c_void);
}

fn cstr_bytes(p: *const c_char) -> Vec<u8> {
    let mut v = Vec::new();
    let mut i = 0usize;
    unsafe {
        while *p.add(i) != 0 {
            v.push(*p.add(i) as u8);
            i += 1;
        }
    }
    v
}

/// Parse `input` with both implementations. Returns (ours, refs); both NULL if
/// the parse failed (we only compare printing on successfully parsed trees).
fn parse_both(input: &[u8], require_null_terminated: bool) -> (*mut CJson, *mut CJson) {
    let input_c = {
        let mut v = input.to_vec();
        v.push(0);
        v
    };
    let ours = unsafe {
        cjson_parse_with_length_opts(
            input_c.as_ptr() as *const c_char,
            input.len(),
            ptr::null_mut(),
            require_null_terminated as c_int,
        )
    };
    let refs = unsafe {
        ref_cJSON_ParseWithLengthOpts(
            input_c.as_ptr() as *const c_char,
            input.len(),
            ptr::null_mut(),
            require_null_terminated as c_int,
        )
    };
    if ours.is_null() != refs.is_null() {
        unsafe { cjson_delete(ours) };
        unsafe { ref_cJSON_Delete(refs) };
        panic!(
            "parse outcome mismatch for {:?}",
            String::from_utf8_lossy(input)
        );
    }
    (ours, refs)
}

fn check_print(input: &[u8]) {
    let (ours, refs) = parse_both(input, true);
    if ours.is_null() {
        unsafe { cjson_delete(ours) };
        unsafe { ref_cJSON_Delete(refs) };
        return;
    }

    let run = |label: &str,
               ours_f: unsafe fn(*const CJson) -> *mut c_char,
               refs_f: unsafe extern "C" fn(*const CJson) -> *mut c_char| {
        let a = unsafe { ours_f(ours) };
        let b = unsafe { refs_f(refs) };
        assert!(
            !a.is_null() && !b.is_null(),
            "{label}: null print for {input:?}"
        );
        let ab = cstr_bytes(a);
        let bb = cstr_bytes(b);
        unsafe {
            ref_cJSON_free(a as *mut c_void);
            ref_cJSON_free(b as *mut c_void);
        }
        assert_eq!(ab, bb, "{label} output mismatch for {:?}", input);
    };

    // whole tree
    run("Print", cjson_print, ref_cJSON_Print);
    run(
        "PrintUnformatted",
        cjson_print_unformatted,
        ref_cJSON_PrintUnformatted,
    );

    // buffered, various prebuffer sizes to exercise ensure()/realloc paths
    for prebuffer in [0, 1, 2, 7, 255, 256, 257, 4096] {
        for fmt in [0, 1] {
            let a = unsafe { cjson_print_buffered(ours, prebuffer, fmt) };
            let b = unsafe { ref_cJSON_PrintBuffered(refs, prebuffer, fmt) };
            assert!(
                !a.is_null() && !b.is_null(),
                "PrintBuffered({prebuffer},{fmt}) null for {input:?}"
            );
            let ab = cstr_bytes(a);
            let bb = cstr_bytes(b);
            unsafe {
                ref_cJSON_free(a as *mut c_void);
                ref_cJSON_free(b as *mut c_void);
            }
            assert_eq!(
                ab, bb,
                "PrintBuffered({prebuffer},{fmt}) mismatch for {:?}",
                input
            );
        }
    }

    // preallocated, with a large enough buffer
    for fmt in [0, 1] {
        let mut buf_ours = vec![0xCCu8; 8192];
        let mut buf_refs = vec![0xCCu8; 8192];
        let ro = unsafe {
            cjson_print_preallocated(
                ours as *mut CJson,
                buf_ours.as_mut_ptr() as *mut c_char,
                8192,
                fmt,
            )
        };
        let rr = unsafe {
            ref_cJSON_PrintPreallocated(
                refs as *mut CJson,
                buf_refs.as_mut_ptr() as *mut c_char,
                8192,
                fmt,
            )
        };
        assert_eq!(
            ro, rr,
            "PrintPreallocated({fmt}) return mismatch for {input:?}"
        );
        if ro != 0 {
            // only compare up to and including the NUL terminator
            assert_eq!(
                cstr_bytes(buf_ours.as_ptr() as *const c_char),
                cstr_bytes(buf_refs.as_ptr() as *const c_char),
                "PrintPreallocated({fmt}) mismatch for {:?}",
                input
            );
        }
    }

    // exercise sub-objects: print every child individually too
    let mut child = unsafe { (*ours).child };
    while !child.is_null() {
        // walk refs in parallel so `rc` is the same element as `child`
        let mut oc = unsafe { (*ours).child };
        let mut rc = unsafe { (*refs).child };
        while oc != child {
            oc = unsafe { (*oc).next };
            rc = unsafe { (*rc).next };
        }
        let a = unsafe { cjson_print(child) };
        let b = unsafe { ref_cJSON_Print(rc) };
        let ab = cstr_bytes(a);
        let bb = cstr_bytes(b);
        unsafe {
            ref_cJSON_free(a as *mut c_void);
            ref_cJSON_free(b as *mut c_void);
        }
        assert_eq!(ab, bb, "child Print mismatch for {:?}", input);
        child = unsafe { (*child).next };
    }

    unsafe { cjson_delete(ours) };
    unsafe { ref_cJSON_Delete(refs) };
}

fn corpus() -> Vec<Vec<u8>> {
    let mut inputs: Vec<Vec<u8>> = Vec::new();

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

    let extra: Vec<&str> = vec![
        "null",
        "true",
        "false",
        "0",
        "-0",
        "1.5",
        "1e300",
        "-1e-300",
        "1e999",
        "2147483647",
        "2147483648",
        "-2147483648",
        "12345678901234567890123",
        "0.0000000000000000001",
        "3.141592653589793",
        "\"\"",
        "\"hello\"",
        "\"a\\\"b\"",
        "\"\\u0041\\u00e9\\uD83D\\uDE00\"",
        "\"\\uFFFF\"",
        "\"tab\\t\\n\\r\\b\\f\\\\\"",
        "\"\\/\"",
        "[]",
        "{}",
        "[1,2,3]",
        "[ ]",
        "[1,2,]",
        "[[[[]]]]",
        "[\"a\",\"b\",[true,false,null]]",
        "{\"a\":1}",
        "{\"a\":1,}",
        "{\"a\":1,\"b\":[true,false,null]}",
        "{\"\":0}",
        "{\"a\":1,\"a\":2}",
        "{\"\\u0061\":1}",
        "  {\"a\":1}  ",
        "\u{feff}{\"a\":1}",
    ];
    for s in extra {
        inputs.push(s.as_bytes().to_vec());
    }
    inputs
}

#[test]
fn differential_print_against_reference() {
    with_lock(|| {
        for input in corpus() {
            check_print(&input);
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

fn random_json(rng: &mut Rng, depth: usize) -> Vec<u8> {
    let mut out = Vec::new();
    match rng.below(if depth > 3 { 6 } else { 7 }) {
        0 => out.extend_from_slice(&[&b"null"[..], &b"true"[..], &b"false"[..]][rng.below(3)]),
        1 => {
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
fn differential_print_fuzz() {
    with_lock(|| {
        let mut rng = Rng(0xDEADBEEF);
        for i in 0..20_000u64 {
            let input = if i % 2 == 0 {
                random_json(&mut rng, 0)
            } else {
                let len = rng.below(32);
                (0..len).map(|_| rng.below(256) as u8).collect()
            };
            check_print(&input);
        }
    });
}
