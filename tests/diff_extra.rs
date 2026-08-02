//! Targeted differential tests for the remaining public API surface that the
//! broad fuzz/scripted suites do not reach: the `cJSON_Create*Array` family,
//! the reference constructors, `cJSON_HasObjectItem` and the via-pointer
//! detach/replace operations. Every operation is run against the reference C
//! cJSON and the Rust port and must agree on return values, null-ness and the
//! printed tree.

use std::ffi::{c_char, c_double, c_int};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use cjson::manip::*;
use cjson::model::CJson;
use cjson::print::cjson_print_unformatted;

fn with_lock<R>(f: impl FnOnce() -> R) -> R {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

// ---- reference C cJSON public API -----------------------------------------

#[link(name = "cjson_ref_bench")]
unsafe extern "C" {
    fn ref_cJSON_CreateIntArray(numbers: *const c_int, count: c_int) -> *mut CJson;
    fn ref_cJSON_CreateFloatArray(numbers: *const f32, count: c_int) -> *mut CJson;
    fn ref_cJSON_CreateDoubleArray(numbers: *const c_double, count: c_int) -> *mut CJson;
    fn ref_cJSON_CreateStringArray(strings: *const *const c_char, count: c_int) -> *mut CJson;
    fn ref_cJSON_CreateStringReference(string: *const c_char) -> *mut CJson;
    fn ref_cJSON_CreateObjectReference(child: *const CJson) -> *mut CJson;
    fn ref_cJSON_CreateArrayReference(child: *const CJson) -> *mut CJson;
    fn ref_cJSON_HasObjectItem(object: *const CJson, string: *const c_char) -> c_int;
    fn ref_cJSON_DetachItemViaPointer(parent: *mut CJson, item: *mut CJson) -> *mut CJson;
    fn ref_cJSON_ReplaceItemViaPointer(
        parent: *mut CJson,
        item: *mut CJson,
        replacement: *mut CJson,
    ) -> c_int;
    fn ref_cJSON_PrintUnformatted(item: *const CJson) -> *mut c_char;
    fn ref_cJSON_Delete(item: *mut CJson);
    fn ref_cJSON_free(ptr: *mut core::ffi::c_void);

    fn ref_cJSON_CreateObject() -> *mut CJson;
    fn ref_cJSON_CreateArray() -> *mut CJson;
    fn ref_cJSON_CreateNumber(num: c_double) -> *mut CJson;
    fn ref_cJSON_CreateString(string: *const c_char) -> *mut CJson;
    fn ref_cJSON_AddItemToObject(
        object: *mut CJson,
        string: *const c_char,
        item: *mut CJson,
    ) -> c_int;
    fn ref_cJSON_AddItemToArray(array: *mut CJson, item: *mut CJson) -> c_int;
    fn ref_cJSON_AddNumberToObject(
        object: *mut CJson,
        name: *const c_char,
        number: c_double,
    ) -> *mut CJson;
    fn ref_cJSON_AddStringToObject(
        object: *mut CJson,
        name: *const c_char,
        string: *const c_char,
    ) -> *mut CJson;
    fn ref_cJSON_GetObjectItem(object: *const CJson, string: *const c_char) -> *mut CJson;
    fn ref_cJSON_GetArrayItem(array: *const CJson, index: c_int) -> *mut CJson;
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

fn cstr_buf(bytes: &[u8]) -> Vec<u8> {
    let mut v = bytes.to_vec();
    v.push(0);
    v
}

/// Print both roots (unformatted) and require identical bytes. Either both may
/// print to NULL or both must print the same string.
fn assert_print_eq(ours: *mut CJson, refs: *mut CJson, label: &str) {
    let a = unsafe { cjson_print_unformatted(ours) };
    let b = unsafe { ref_cJSON_PrintUnformatted(refs) };
    assert_eq!(
        a.is_null(),
        b.is_null(),
        "{label}: print null-ness mismatch"
    );
    if !a.is_null() {
        let ab = cstr_bytes(a);
        let bb = cstr_bytes(b);
        unsafe {
            ref_cJSON_free(a as *mut core::ffi::c_void);
            ref_cJSON_free(b as *mut core::ffi::c_void);
        }
        assert_eq!(ab, bb, "{label}: printed tree mismatch");
    }
}

/// Both pointers must be null or both non-null.
fn assert_same_null(a: *const CJson, b: *const CJson, label: &str) {
    assert_eq!(a.is_null(), b.is_null(), "{label}: null-ness mismatch");
}

fn delete_pair(ours: *mut CJson, refs: *mut CJson) {
    unsafe {
        cjson_delete(ours);
        ref_cJSON_Delete(refs);
    }
}

// ---- cJSON_Create{Int,Float,Double,String}Array ----------------------------

#[test]
fn create_number_arrays_match_reference_c() {
    with_lock(|| {
        // non-empty arrays exercise the happy path
        let ints: [c_int; 5] = [0, -1, 2, i32::MIN, i32::MAX];
        let o = unsafe { cjson_create_int_array(ints.as_ptr(), 5) };
        let r = unsafe { ref_cJSON_CreateIntArray(ints.as_ptr(), 5) };
        assert_print_eq(o, r, "create_int_array");
        delete_pair(o, r);

        let floats: [f32; 4] = [0.0, -1.5, 3.25, 1e20];
        let o = unsafe { cjson_create_float_array(floats.as_ptr(), 4) };
        let r = unsafe { ref_cJSON_CreateFloatArray(floats.as_ptr(), 4) };
        assert_print_eq(o, r, "create_float_array");
        delete_pair(o, r);

        let doubles: [f64; 6] = [0.0, -0.0, 1e308, -1e-308, 1.5, 2.0];
        let o = unsafe { cjson_create_double_array(doubles.as_ptr(), 6) };
        let r = unsafe { ref_cJSON_CreateDoubleArray(doubles.as_ptr(), 6) };
        assert_print_eq(o, r, "create_double_array");
        delete_pair(o, r);

        // count == 0 with a valid pointer produces an empty array
        let ints2: [c_int; 1] = [1];
        let o = unsafe { cjson_create_int_array(ints2.as_ptr(), 0) };
        let r = unsafe { ref_cJSON_CreateIntArray(ints2.as_ptr(), 0) };
        assert_print_eq(o, r, "create_int_array(count 0)");
        delete_pair(o, r);

        // negative count and null pointer both fail on both sides
        let o = unsafe { cjson_create_int_array(ints.as_ptr(), -1) };
        let r = unsafe { ref_cJSON_CreateIntArray(ints.as_ptr(), -1) };
        assert_same_null(o, r, "create_int_array(count -1)");
        delete_pair(o, r);

        let o = unsafe { cjson_create_double_array(ptr::null(), 3) };
        let r = unsafe { ref_cJSON_CreateDoubleArray(ptr::null(), 3) };
        assert_same_null(o, r, "create_double_array(null ptr)");
        delete_pair(o, r);
    });
}

#[test]
fn create_string_array_matches_reference_c() {
    with_lock(|| {
        let names = [
            cstr_buf(b"alpha"),
            cstr_buf(b"with\"quote"),
            cstr_buf(b"unicode-\xF0\x9F\x99\x82"),
            cstr_buf(b""),
        ];
        let refs: Vec<*const c_char> = names.iter().map(|b| b.as_ptr() as *const c_char).collect();

        let o = unsafe { cjson_create_string_array(refs.as_ptr(), refs.len() as c_int) };
        let r = unsafe { ref_cJSON_CreateStringArray(refs.as_ptr(), refs.len() as c_int) };
        assert_print_eq(o, r, "create_string_array");
        delete_pair(o, r);

        // a NULL element mid-array is mirrored (both print to NULL)
        let with_null = vec![refs[0], ptr::null(), refs[1]];
        let o = unsafe { cjson_create_string_array(with_null.as_ptr(), 3) };
        let r = unsafe { ref_cJSON_CreateStringArray(with_null.as_ptr(), 3) };
        assert_print_eq(o, r, "create_string_array(null element)");
        delete_pair(o, r);

        let o = unsafe { cjson_create_string_array(refs.as_ptr(), 0) };
        let r = unsafe { ref_cJSON_CreateStringArray(refs.as_ptr(), 0) };
        assert_print_eq(o, r, "create_string_array(count 0)");
        delete_pair(o, r);

        let o = unsafe { cjson_create_string_array(ptr::null(), 2) };
        let r = unsafe { ref_cJSON_CreateStringArray(ptr::null(), 2) };
        assert_same_null(o, r, "create_string_array(null ptr)");
        delete_pair(o, r);
    });
}

// ---- reference constructors ------------------------------------------------

#[test]
fn reference_constructors_match_reference_c() {
    with_lock(|| {
        // string reference wraps the pointer without copying
        let sv = cstr_buf(b"borrowed");
        let o = unsafe { cjson_create_string_reference(sv.as_ptr() as *const c_char) };
        let r = unsafe { ref_cJSON_CreateStringReference(sv.as_ptr() as *const c_char) };
        assert_print_eq(o, r, "create_string_reference");
        delete_pair(o, r);

        // object and array references wrap an existing child
        let (src_o, src_r) = {
            let src_o = unsafe { cjson_create_object() };
            let src_r = unsafe { ref_cJSON_CreateObject() };
            let so = cstr_buf(b"kid");
            unsafe {
                cjson_add_number_to_object(src_o, so.as_ptr() as *const c_char, 42.0);
                let sr = cstr_buf(b"kid");
                ref_cJSON_AddNumberToObject(src_r, sr.as_ptr() as *const c_char, 42.0);
            }
            (src_o, src_r)
        };
        let child_o = unsafe { cjson_get_object_item(src_o, sv.as_ptr() as *const c_char) };
        let child_r = unsafe { ref_cJSON_GetObjectItem(src_r, sv.as_ptr() as *const c_char) };

        // the reference wrapper owns only the wrapper; the child stays owned
        // by the source object. Both must print the referenced child.
        let ko = cstr_buf(b"ref");
        let ro = unsafe { cjson_create_object_reference(child_o) };
        let rr = unsafe { ref_cJSON_CreateObjectReference(child_r) };
        let oo = unsafe { cjson_create_object() };
        let or = unsafe { ref_cJSON_CreateObject() };
        unsafe {
            cjson_add_item_to_object(oo, ko.as_ptr() as *const c_char, ro);
            let kr = cstr_buf(b"ref");
            ref_cJSON_AddItemToObject(or, kr.as_ptr() as *const c_char, rr);
        }
        assert_print_eq(oo, or, "add object reference");
        delete_pair(oo, or);

        let ar = unsafe { ref_cJSON_CreateArrayReference(child_r) };
        let ao = unsafe { cjson_create_array_reference(child_o) };
        let aao = unsafe { cjson_create_array() };
        let aar = unsafe { ref_cJSON_CreateArray() };
        unsafe {
            cjson_add_item_to_array(aao, ao);
            ref_cJSON_AddItemToArray(aar, ar);
        }
        assert_print_eq(aao, aar, "add array reference");
        delete_pair(aao, aar);

        delete_pair(src_o, src_r);

        // null child still yields a wrapper (prints {}) on both sides
        let o = unsafe { cjson_create_object_reference(ptr::null()) };
        let r = unsafe { ref_cJSON_CreateObjectReference(ptr::null()) };
        assert_print_eq(o, r, "create_object_reference(null)");
        delete_pair(o, r);
    });
}

// ---- cJSON_HasObjectItem ---------------------------------------------------

#[test]
fn has_object_item_matches_reference_c() {
    with_lock(|| {
        let o = unsafe { cjson_create_object() };
        let r = unsafe { ref_cJSON_CreateObject() };
        unsafe {
            let k = cstr_buf(b"MiXeD");
            cjson_add_string_to_object(
                o,
                k.as_ptr() as *const c_char,
                b"v".as_ptr() as *const c_char,
            );
            let kr = cstr_buf(b"MiXeD");
            ref_cJSON_AddStringToObject(
                r,
                kr.as_ptr() as *const c_char,
                b"v".as_ptr() as *const c_char,
            );
        }
        for (label, key) in [
            ("exact", &b"MiXeD"[..]),
            ("case-insensitive", &b"mixed"[..]),
            ("absent", &b"missing"[..]),
        ] {
            let k = cstr_buf(key);
            let ho = unsafe { cjson_has_object_item(o, k.as_ptr() as *const c_char) };
            let hr = unsafe { ref_cJSON_HasObjectItem(r, k.as_ptr() as *const c_char) };
            assert_eq!(ho, hr, "has_object_item({label})");
        }
        let ho = unsafe { cjson_has_object_item(ptr::null(), b"x".as_ptr() as *const c_char) };
        let hr = unsafe { ref_cJSON_HasObjectItem(ptr::null(), b"x".as_ptr() as *const c_char) };
        assert_eq!(ho, hr, "has_object_item(null object)");
        delete_pair(o, r);
    });
}

// ---- via-pointer detach / replace ------------------------------------------

#[test]
fn via_pointer_ops_match_reference_c() {
    with_lock(|| {
        // detach first / middle / last / single-child element
        for n in [1usize, 3, 5] {
            let mut detach = |idx: c_int| {
                let o = unsafe { cjson_create_array() };
                let r = unsafe { ref_cJSON_CreateArray() };
                for i in 0..n as i32 {
                    unsafe {
                        cjson_add_item_to_array(o, cjson_create_number(i as c_double));
                        ref_cJSON_AddItemToArray(r, ref_cJSON_CreateNumber(i as c_double));
                    }
                }
                let child_o = unsafe { cjson_get_array_item(o, idx) };
                let child_r = unsafe { ref_cJSON_GetArrayItem(r, idx) };
                let det_o = unsafe { cjson_detach_item_via_pointer(o, child_o) };
                let det_r = unsafe { ref_cJSON_DetachItemViaPointer(r, child_r) };
                assert_same_null(det_o, det_r, "detach return");
                assert_print_eq(o, r, &format!("detach idx {idx} of {n}"));
                unsafe {
                    cjson_delete(det_o);
                    ref_cJSON_Delete(det_r);
                }
                delete_pair(o, r);
            };
            detach(0);
            if n > 1 {
                detach(n as c_int / 2);
                detach(n as c_int - 1);
            }
        }

        // item not directly owned by the parent -> both detach to NULL
        let o = unsafe { cjson_create_array() };
        let r = unsafe { ref_cJSON_CreateArray() };
        let foreign_o = unsafe { cjson_create_number(1.0) };
        let foreign_r = unsafe { ref_cJSON_CreateNumber(1.0) };
        let det_o = unsafe { cjson_detach_item_via_pointer(o, foreign_o) };
        let det_r = unsafe { ref_cJSON_DetachItemViaPointer(r, foreign_r) };
        assert_same_null(det_o, det_r, "detach foreign item");
        assert_print_eq(o, r, "detach foreign item tree");
        unsafe {
            cjson_delete(foreign_o);
            ref_cJSON_Delete(foreign_r);
        }
        delete_pair(o, r);

        // null parent / null item
        let det_o = unsafe { cjson_detach_item_via_pointer(ptr::null_mut(), ptr::null_mut()) };
        let det_r = unsafe { ref_cJSON_DetachItemViaPointer(ptr::null_mut(), ptr::null_mut()) };
        assert_same_null(det_o, det_r, "detach null args");

        // replace first / last / single-child element (old item is freed by
        // the implementation, so we only free the replacement-owner tree)
        for n in [1usize, 3] {
            let replace = |idx: c_int| {
                let o = unsafe { cjson_create_array() };
                let r = unsafe { ref_cJSON_CreateArray() };
                for i in 0..n as i32 {
                    unsafe {
                        cjson_add_item_to_array(o, cjson_create_number(i as c_double));
                        ref_cJSON_AddItemToArray(r, ref_cJSON_CreateNumber(i as c_double));
                    }
                }
                let child_o = unsafe { cjson_get_array_item(o, idx) };
                let child_r = unsafe { ref_cJSON_GetArrayItem(r, idx) };
                let new_o = unsafe { cjson_create_string(b"new".as_ptr() as *const c_char) };
                let new_r = unsafe { ref_cJSON_CreateString(b"new".as_ptr() as *const c_char) };
                let rc_o = unsafe { cjson_replace_item_via_pointer(o, child_o, new_o) };
                let rc_r = unsafe { ref_cJSON_ReplaceItemViaPointer(r, child_r, new_r) };
                assert_eq!(rc_o, rc_r, "replace rc idx {idx} of {n}");
                assert_print_eq(o, r, &format!("replace idx {idx} of {n}"));
                delete_pair(o, r);
            };
            replace(0);
            if n > 1 {
                replace(n as c_int - 1);
            }
        }

        // replacement == item is a no-op success
        let o = unsafe { cjson_create_array() };
        let r = unsafe { ref_cJSON_CreateArray() };
        unsafe {
            cjson_add_item_to_array(o, cjson_create_number(1.0));
            ref_cJSON_AddItemToArray(r, ref_cJSON_CreateNumber(1.0));
        }
        let self_o = unsafe { cjson_get_array_item(o, 0) };
        let self_r = unsafe { ref_cJSON_GetArrayItem(r, 0) };
        let rc_o = unsafe { cjson_replace_item_via_pointer(o, self_o, self_o) };
        let rc_r = unsafe { ref_cJSON_ReplaceItemViaPointer(r, self_r, self_r) };
        assert_eq!(rc_o, rc_r, "replace self rc");
        assert_print_eq(o, r, "replace self tree");
        delete_pair(o, r);

        // null parent / null replacement
        let rc_o = unsafe {
            cjson_replace_item_via_pointer(ptr::null_mut(), ptr::null_mut(), ptr::null_mut())
        };
        let rc_r = unsafe {
            ref_cJSON_ReplaceItemViaPointer(ptr::null_mut(), ptr::null_mut(), ptr::null_mut())
        };
        assert_eq!(rc_o, rc_r, "replace null args rc");
    });
}
