//! Differential print tests for inputs the always-on fuzz and corpus
//! generators cannot reach from JSON text alone: NaN/±Inf number nodes, raw
//! nodes, nesting beyond `CJSON_NESTING_LIMIT`, invalid type flags,
//! `%1.17g`-escalating doubles, the full control-byte escape table, and
//! `PrintPreallocated` buffers too small for the output.
//!
//! Trees are built on both implementations (via the manip API or by parsing)
//! and printed with every print entry point, asserting byte-identical output
//! or symmetric failure.

use std::ffi::{c_char, c_double, c_int, c_void, CString};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use cjson::manip::{
    cjson_add_item_to_array, cjson_add_item_to_object, cjson_create_array, cjson_create_number,
    cjson_create_object, cjson_create_raw, cjson_create_string, cjson_delete,
};
use cjson::model::CJson;
use cjson::parse::cjson_parse_with_length_opts;
use cjson::print::{
    cjson_print, cjson_print_buffered, cjson_print_preallocated, cjson_print_unformatted,
};
use cjson_ref_sys as _;

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
    fn ref_cJSON_CreateNumber(num: c_double) -> *mut CJson;
    fn ref_cJSON_CreateString(string: *const c_char) -> *mut CJson;
    fn ref_cJSON_CreateRaw(raw: *const c_char) -> *mut CJson;
    fn ref_cJSON_CreateObject() -> *mut CJson;
    fn ref_cJSON_CreateArray() -> *mut CJson;
    fn ref_cJSON_AddItemToObject(
        object: *mut CJson,
        string: *const c_char,
        item: *mut CJson,
    ) -> c_int;
    fn ref_cJSON_AddItemToArray(array: *mut CJson, item: *mut CJson) -> c_int;
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

fn cs(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap()
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

/// Parse `input` with both implementations; panic on asymmetric parse
/// outcomes, exactly like the corpus differential test. `require_null_terminated`
/// needs the declared length to include the NUL terminator.
fn parse_both(input: &[u8]) -> (*mut CJson, *mut CJson) {
    let input_c = {
        let mut v = input.to_vec();
        v.push(0);
        v
    };
    let ours = unsafe {
        cjson_parse_with_length_opts(
            input_c.as_ptr() as *const c_char,
            input_c.len(),
            ptr::null_mut(),
            1,
        )
    };
    let refs = unsafe {
        ref_cJSON_ParseWithLengthOpts(
            input_c.as_ptr() as *const c_char,
            input_c.len(),
            ptr::null_mut(),
            1,
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

#[derive(Clone, Copy, PartialEq)]
enum Expect {
    /// Both prints must succeed and the output must match.
    BothSucceed,
    /// Both prints must fail (NULL/0).
    BothFail,
    /// Outcomes must be symmetric; content compared only when both succeed.
    Either,
}

/// Print `ours`/`refs` with every print entry point and assert byte-identical
/// output (or symmetric failure). `PrintPreallocated` is swept across small
/// and large buffers to exercise the noalloc/truncation path on both sides.
fn assert_pair_symmetric(ours: *mut CJson, refs: *mut CJson, label: &str, expect: Expect) {
    let run_pair = |name: &str,
                    o_f: unsafe fn(*const CJson) -> *mut c_char,
                    r_f: unsafe extern "C" fn(*const CJson) -> *mut c_char| {
        let a = unsafe { o_f(ours) };
        let b = unsafe { r_f(refs) };
        match expect {
            Expect::BothSucceed => {
                assert!(
                    !a.is_null() && !b.is_null(),
                    "{label}/{name}: unexpected print failure"
                );
            }
            Expect::BothFail => {
                assert!(
                    a.is_null() && b.is_null(),
                    "{label}/{name}: expected both prints to fail"
                );
            }
            Expect::Either => {
                assert_eq!(
                    a.is_null(),
                    b.is_null(),
                    "{label}/{name}: asymmetric print failure"
                );
            }
        }
        if !a.is_null() && !b.is_null() {
            assert_eq!(cstr_bytes(a), cstr_bytes(b), "{label}/{name} mismatch");
        }
        unsafe { ref_cJSON_free(a as *mut c_void) };
        unsafe { ref_cJSON_free(b as *mut c_void) };
    };

    run_pair("Print", cjson_print, ref_cJSON_Print);
    run_pair(
        "PrintUnformatted",
        cjson_print_unformatted,
        ref_cJSON_PrintUnformatted,
    );

    // buffered, various prebuffer sizes to exercise ensure()/realloc growth
    for prebuffer in [0, 1, 2, 7, 255, 256, 257, 4096] {
        for fmt in [0, 1] {
            let a = unsafe { cjson_print_buffered(ours, prebuffer, fmt) };
            let b = unsafe { ref_cJSON_PrintBuffered(refs, prebuffer, fmt) };
            assert_eq!(
                a.is_null(),
                b.is_null(),
                "{label}/PrintBuffered({prebuffer},{fmt}) asymmetric failure"
            );
            if !a.is_null() {
                assert_eq!(
                    cstr_bytes(a),
                    cstr_bytes(b),
                    "{label}/PrintBuffered({prebuffer},{fmt}) mismatch"
                );
            }
            unsafe { ref_cJSON_free(a as *mut c_void) };
            unsafe { ref_cJSON_free(b as *mut c_void) };
        }
    }

    // preallocated: sweep sizes both sides of the output length, so the
    // noalloc "buffer too small" branch is exercised alongside success
    for size in [1, 2, 3, 7, 16, 64, 255, 1024, 8192] {
        for fmt in [0, 1] {
            let mut bo = vec![0xCCu8; size];
            let mut br = vec![0xCCu8; size];
            let ro = unsafe {
                cjson_print_preallocated(ours, bo.as_mut_ptr() as *mut c_char, size as c_int, fmt)
            };
            let rr = unsafe {
                ref_cJSON_PrintPreallocated(
                    refs as *mut CJson,
                    br.as_mut_ptr() as *mut c_char,
                    size as c_int,
                    fmt,
                )
            };
            assert_eq!(
                ro, rr,
                "{label}/PrintPreallocated({size},{fmt}) return mismatch"
            );
            if ro != 0 {
                assert_eq!(
                    cstr_bytes(bo.as_ptr() as *const c_char),
                    cstr_bytes(br.as_ptr() as *const c_char),
                    "{label}/PrintPreallocated({size},{fmt}) mismatch"
                );
            }
        }
    }
}

// ---- tree builders (manip API, both implementations) ----------------------

unsafe fn build_object_chain(n: usize) -> (*mut CJson, *mut CJson) {
    let key = cs(b"k");
    let o_root = cjson_create_object();
    let r_root = ref_cJSON_CreateObject();
    let mut o = o_root;
    let mut r = r_root;
    for _ in 0..n {
        let no = cjson_create_object();
        let nr = ref_cJSON_CreateObject();
        cjson_add_item_to_object(o, key.as_ptr(), no);
        ref_cJSON_AddItemToObject(r, key.as_ptr(), nr);
        o = no;
        r = nr;
    }
    (o_root, r_root)
}

unsafe fn build_array_chain(n: usize) -> (*mut CJson, *mut CJson) {
    let a_root = cjson_create_array();
    let r_root = ref_cJSON_CreateArray();
    let mut a = a_root;
    let mut r = r_root;
    for _ in 0..n {
        let na = cjson_create_array();
        let nr = ref_cJSON_CreateArray();
        cjson_add_item_to_array(a, na);
        ref_cJSON_AddItemToArray(r, nr);
        a = na;
        r = nr;
    }
    (a_root, r_root)
}

unsafe fn build_number_array(n: usize) -> (*mut CJson, *mut CJson) {
    let a_root = cjson_create_array();
    let r_root = ref_cJSON_CreateArray();
    for i in 0..n {
        let no = cjson_create_number(i as f64);
        let nr = ref_cJSON_CreateNumber(i as f64);
        cjson_add_item_to_array(a_root, no);
        ref_cJSON_AddItemToArray(r_root, nr);
    }
    (a_root, r_root)
}

unsafe fn build_string_object(bytes: &[u8]) -> (*mut CJson, *mut CJson) {
    let key = cs(b"k");
    let c = cs(bytes);
    let o_root = cjson_create_object();
    let r_root = ref_cJSON_CreateObject();
    let no = cjson_create_string(c.as_ptr());
    let nr = ref_cJSON_CreateString(c.as_ptr());
    cjson_add_item_to_object(o_root, key.as_ptr(), no);
    ref_cJSON_AddItemToObject(r_root, key.as_ptr(), nr);
    (o_root, r_root)
}

// ---- tests ----------------------------------------------------------------

#[test]
fn print_edge_special_doubles() {
    with_lock(|| {
        // NaN/±Inf are not reachable from JSON text; build them via the
        // number-construction API on both sides.
        for v in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.0,
            0.0,
            5e-324,
            -5e-324,
            1.7976931348623157e308,
            -1.7976931348623157e308,
            1e308,
            -1e308,
            2.2250738585072014e-308,
        ] {
            let ours = unsafe { cjson_create_number(v) };
            let refs = unsafe { ref_cJSON_CreateNumber(v) };
            assert!(!ours.is_null() && !refs.is_null());
            assert_pair_symmetric(
                ours,
                refs,
                &format!("create_number({v:?})"),
                Expect::BothSucceed,
            );
            unsafe { cjson_delete(ours) };
            unsafe { ref_cJSON_Delete(refs) };
        }
    });
}

#[test]
fn print_edge_numbers_via_parse() {
    with_lock(|| {
        // Doubles that force the `%1.17g` fallback (a `%1.15g` round-trip
        // fails the DBL_EPSILON comparison), plus subnormals, DBL_MAX, and
        // boundary integers around 2^53.
        let inputs: Vec<&[u8]> = vec![
            b"0",
            b"0.0",
            b"-0",
            b"-0.0",
            b"1.0000000000000004",
            b"1.0000000000000008",
            b"2.0000000000000004",
            b"1.9999999999999998",
            b"0.30000000000000004",
            b"0.1000000000000000001",
            b"3.141592653589793238462643383279502884197169399375105820974944",
            b"0.1234567890123456789",
            b"9876543210.123456789",
            b"123456789.123456789",
            b"1.234567890123456789e-10",
            b"123456789012345678901234567890.123456789",
            b"9007199254740991",
            b"9007199254740992",
            b"9007199254740993",
            b"9007199254740994",
            b"-9007199254740993",
            b"5e-324",
            b"-5e-324",
            b"4.9406564584124654e-324",
            b"2.2250738585072014e-308",
            b"1e-307",
            b"1e308",
            b"-1e308",
            b"1.7976931348623157e308",
            b"-1.7976931348623157e308",
        ];
        for input in inputs {
            let (ours, refs) = parse_both(input);
            if !ours.is_null() {
                assert_pair_symmetric(
                    ours,
                    refs,
                    &format!("parse {}", String::from_utf8_lossy(input)),
                    Expect::BothSucceed,
                );
                unsafe { cjson_delete(ours) };
                unsafe { ref_cJSON_Delete(refs) };
            }
        }
    });
}

#[test]
fn print_edge_raw_nodes() {
    with_lock(|| {
        // Raw nodes are not parseable from JSON text; build them on both sides.
        let raws: Vec<&[u8]> = vec![
            b"{\"key\":[1,2,3]}",
            b"[true,false,null]",
            b"42",
            b"",
            b"\"a quoted string\"",
            b"{\"nested\":{\"object\":{}}}",
        ];
        for raw in raws {
            let c = cs(raw);
            let ours = unsafe { cjson_create_raw(c.as_ptr()) };
            let refs = unsafe { ref_cJSON_CreateRaw(c.as_ptr()) };
            assert!(!ours.is_null() && !refs.is_null());
            assert_pair_symmetric(
                ours,
                refs,
                &format!("raw {:?}", String::from_utf8_lossy(raw)),
                Expect::BothSucceed,
            );
            unsafe { cjson_delete(ours) };
            unsafe { ref_cJSON_Delete(refs) };
        }

        // raw nested inside an object and an array
        let key = cs(b"r");
        let raw_c = cs(b"{\"k\":[1,2]}");
        let ours = unsafe { cjson_create_object() };
        let refs = unsafe { ref_cJSON_CreateObject() };
        let oraw = unsafe { cjson_create_raw(raw_c.as_ptr()) };
        let rraw = unsafe { ref_cJSON_CreateRaw(raw_c.as_ptr()) };
        unsafe { cjson_add_item_to_object(ours, key.as_ptr(), oraw) };
        unsafe { ref_cJSON_AddItemToObject(refs, key.as_ptr(), rraw) };
        assert_pair_symmetric(ours, refs, "raw-in-object", Expect::BothSucceed);
        unsafe { cjson_delete(ours) };
        unsafe { ref_cJSON_Delete(refs) };

        let ours_a = unsafe { cjson_create_array() };
        let refs_a = unsafe { ref_cJSON_CreateArray() };
        let araw = unsafe { cjson_create_raw(raw_c.as_ptr()) };
        let arraw = unsafe { ref_cJSON_CreateRaw(raw_c.as_ptr()) };
        unsafe { cjson_add_item_to_array(ours_a, araw) };
        unsafe { ref_cJSON_AddItemToArray(refs_a, arraw) };
        assert_pair_symmetric(ours_a, refs_a, "raw-in-array", Expect::BothSucceed);
        unsafe { cjson_delete(ours_a) };
        unsafe { ref_cJSON_Delete(refs_a) };
    });
}

#[test]
fn print_edge_nesting_limit() {
    with_lock(|| {
        // `CJSON_NESTING_LIMIT` is 1000; a parse can never build past it, so
        // build the chains through the manip API. A chain of `n` nested
        // containers is checked at depth `n` and fails once `n >= 1000`.
        for n in [500usize, 999, 1000, 1001, 1500] {
            let expect = if n <= 999 {
                Expect::BothSucceed
            } else if n >= 1000 && n <= 1001 {
                Expect::Either
            } else {
                Expect::BothFail
            };

            let (ours, refs) = unsafe { build_object_chain(n) };
            assert_pair_symmetric(ours, refs, &format!("object-chain({n})"), expect);
            unsafe { cjson_delete(ours) };
            unsafe { ref_cJSON_Delete(refs) };

            let (ours_a, refs_a) = unsafe { build_array_chain(n) };
            assert_pair_symmetric(ours_a, refs_a, &format!("array-chain({n})"), expect);
            unsafe { cjson_delete(ours_a) };
            unsafe { ref_cJSON_Delete(refs_a) };
        }
    });
}

#[test]
fn print_edge_invalid_type() {
    with_lock(|| {
        // A type flag byte that matches no cJSON type makes every print entry
        // point fail on both implementations.
        for input in [
            &b"123"[..],
            &b"\"abc\""[..],
            &b"{\"a\":1}"[..],
            &b"[1,2,3]"[..],
        ] {
            let (ours, refs) = parse_both(input);
            assert!(!ours.is_null());
            unsafe {
                (*ours).type_ = 0xFF;
                (*refs).type_ = 0xFF;
            }
            assert_pair_symmetric(
                ours,
                refs,
                &format!("invalid-type {:?}", input),
                Expect::BothFail,
            );
            unsafe { cjson_delete(ours) };
            unsafe { ref_cJSON_Delete(refs) };
        }
    });
}

#[test]
fn print_edge_control_byte_escapes() {
    with_lock(|| {
        // Every control byte 0x01..=0x1F must render as a \u00xx escape.
        // Inject via CreateString so coverage is guaranteed regardless of how
        // lenient the parser is about control bytes in JSON text.
        let bytes: Vec<u8> = (1..=0x1F).collect();
        let (ours, refs) = unsafe { build_string_object(&bytes) };
        assert_pair_symmetric(ours, refs, "control-byte-string", Expect::BothSucceed);
        unsafe { cjson_delete(ours) };
        unsafe { ref_cJSON_Delete(refs) };

        // ...and as raw bytes inside JSON text (symmetric either way)
        let mut s = Vec::new();
        s.extend_from_slice(b"{\"c\":\"");
        for b in 1..=0x1F {
            s.push(b);
        }
        s.extend_from_slice(b"\"}");
        let (o2, r2) = parse_both(&s);
        if !o2.is_null() {
            assert_pair_symmetric(o2, r2, "control-bytes-in-text", Expect::BothSucceed);
            unsafe { cjson_delete(o2) };
            unsafe { ref_cJSON_Delete(r2) };
        }
    });
}

#[test]
fn print_edge_large_outputs() {
    with_lock(|| {
        // Large outputs force repeated ensure()/realloc growth and exercise
        // update_offset on outputs far larger than the default 256-byte buffer.
        let (oa, ra) = unsafe { build_number_array(10_000) };
        assert_pair_symmetric(oa, ra, "large-array", Expect::BothSucceed);
        unsafe { cjson_delete(oa) };
        unsafe { ref_cJSON_Delete(ra) };

        let big = vec![b'x'; 65536];
        let (os, rs) = unsafe { build_string_object(&big) };
        assert_pair_symmetric(os, rs, "large-string", Expect::BothSucceed);
        unsafe { cjson_delete(os) };
        unsafe { ref_cJSON_Delete(rs) };
    });
}
