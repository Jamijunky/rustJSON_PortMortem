//! Differential tests for the manipulation API: run identical operation
//! sequences against the reference C cJSON and the Rust port and require
//! byte-identical printed output (plus equal scalar results) at every step.

use std::ffi::{c_char, c_double, c_int};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use cjson::manip::*;
use cjson::model::CJson;
use cjson::print::{cjson_print_unformatted};

fn with_lock<R>(f: impl FnOnce() -> R) -> R {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

// ---- reference C cJSON public API -----------------------------------------

#[link(name = "cjson_ref")]
unsafe extern "C" {
    fn cJSON_CreateObject() -> *mut CJson;
    fn cJSON_CreateArray() -> *mut CJson;
    fn cJSON_CreateNumber(num: c_double) -> *mut CJson;
    fn cJSON_CreateString(string: *const c_char) -> *mut CJson;
    fn cJSON_CreateNull() -> *mut CJson;
    fn cJSON_CreateTrue() -> *mut CJson;
    fn cJSON_AddItemToArray(array: *mut CJson, item: *mut CJson) -> c_int;
    fn cJSON_AddItemToObjectCS(object: *mut CJson, string: *const c_char, item: *mut CJson) -> c_int;
    fn cJSON_AddItemReferenceToArray(array: *mut CJson, item: *mut CJson) -> c_int;
    fn cJSON_AddItemReferenceToObject(object: *mut CJson, string: *const c_char, item: *mut CJson) -> c_int;
    fn cJSON_AddNullToObject(object: *mut CJson, name: *const c_char) -> *mut CJson;
    fn cJSON_AddTrueToObject(object: *mut CJson, name: *const c_char) -> *mut CJson;
    fn cJSON_AddFalseToObject(object: *mut CJson, name: *const c_char) -> *mut CJson;
    fn cJSON_AddBoolToObject(object: *mut CJson, name: *const c_char, boolean: c_int) -> *mut CJson;
    fn cJSON_AddNumberToObject(object: *mut CJson, name: *const c_char, number: c_double) -> *mut CJson;
    fn cJSON_AddStringToObject(object: *mut CJson, name: *const c_char, string: *const c_char) -> *mut CJson;
    fn cJSON_AddRawToObject(object: *mut CJson, name: *const c_char, raw: *const c_char) -> *mut CJson;
    fn cJSON_AddObjectToObject(object: *mut CJson, name: *const c_char) -> *mut CJson;
    fn cJSON_AddArrayToObject(object: *mut CJson, name: *const c_char) -> *mut CJson;
    fn cJSON_GetArraySize(array: *const CJson) -> c_int;
    fn cJSON_GetArrayItem(array: *const CJson, index: c_int) -> *mut CJson;
    fn cJSON_GetObjectItem(object: *const CJson, string: *const c_char) -> *mut CJson;
    fn cJSON_GetObjectItemCaseSensitive(object: *const CJson, string: *const c_char) -> *mut CJson;
    fn cJSON_DetachItemFromArray(array: *mut CJson, which: c_int) -> *mut CJson;
    fn cJSON_DeleteItemFromArray(array: *mut CJson, which: c_int);
    fn cJSON_DetachItemFromObject(object: *mut CJson, string: *const c_char) -> *mut CJson;
    fn cJSON_DeleteItemFromObject(object: *mut CJson, string: *const c_char);
    fn cJSON_DeleteItemFromObjectCaseSensitive(object: *mut CJson, string: *const c_char);
    fn cJSON_InsertItemInArray(array: *mut CJson, which: c_int, newitem: *mut CJson) -> c_int;
    fn cJSON_ReplaceItemInArray(array: *mut CJson, which: c_int, newitem: *mut CJson) -> c_int;
    fn cJSON_ReplaceItemInObject(object: *mut CJson, string: *const c_char, newitem: *mut CJson) -> c_int;
    fn cJSON_ReplaceItemInObjectCaseSensitive(object: *mut CJson, string: *const c_char, newitem: *mut CJson) -> c_int;
    fn cJSON_Duplicate(item: *const CJson, recurse: c_int) -> *mut CJson;
    fn cJSON_Compare(a: *const CJson, b: *const CJson, case_sensitive: c_int) -> c_int;
    fn cJSON_Minify(json: *mut c_char);
    fn cJSON_SetValuestring(object: *mut CJson, valuestring: *const c_char) -> *mut c_char;
    fn cJSON_SetNumberHelper(object: *mut CJson, number: c_double) -> c_double;
    fn cJSON_GetStringValue(item: *const CJson) -> *mut c_char;
    fn cJSON_GetNumberValue(item: *const CJson) -> c_double;
    fn cJSON_IsInvalid(item: *const CJson) -> c_int;
    fn cJSON_IsNull(item: *const CJson) -> c_int;
    fn cJSON_IsNumber(item: *const CJson) -> c_int;
    fn cJSON_IsString(item: *const CJson) -> c_int;
    fn cJSON_IsArray(item: *const CJson) -> c_int;
    fn cJSON_IsObject(item: *const CJson) -> c_int;
    fn cJSON_Version() -> *const c_char;
    fn cJSON_PrintUnformatted(item: *const CJson) -> *mut c_char;
    fn cJSON_Delete(item: *mut CJson);
    fn cJSON_free(ptr: *mut core::ffi::c_void);
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

/// Print both roots (unformatted) and require identical bytes.
fn assert_print_eq(ours: *mut CJson, refs: *mut CJson, label: &str) {
    let a = unsafe { cjson_print_unformatted(ours) };
    let b = unsafe { cJSON_PrintUnformatted(refs) };
    assert!(!a.is_null() && !b.is_null(), "{label}: null print");
    let ab = cstr_bytes(a);
    let bb = cstr_bytes(b);
    unsafe {
        cJSON_free(a as *mut core::ffi::c_void);
        cJSON_free(b as *mut core::ffi::c_void);
    }
    assert_eq!(ab, bb, "{label}: printed tree mismatch");
}

fn cstr_buf(bytes: &[u8]) -> Vec<u8> {
    let mut v = bytes.to_vec();
    v.push(0);
    v
}

/// Run a scripted program against both implementations. Each closure receives
/// its own root; the two closures are intended to perform the same operations.
fn run_both(
    build: impl Fn() -> (*mut CJson, *mut CJson),
    program: impl Fn(*mut CJson, *mut CJson),
    label: &str,
) {
    let (ours, refs) = build();
    program(ours, refs);
    assert_print_eq(ours, refs, label);
    unsafe {
        cjson_delete(ours);
        cJSON_Delete(refs);
    }
}

/// Both pointers must be null or both non-null.
fn assert_same_null(a: *const CJson, b: *const CJson, label: &str) {
    assert_eq!(a.is_null(), b.is_null(), "{label}: null-ness mismatch");
}

// ---- scripted tests --------------------------------------------------------

#[test]
fn differential_manip_scripted() {
    with_lock(|| {
        // Build a nested tree with every Add helper on both sides.
        run_both(
            || (unsafe { cjson_create_object() }, unsafe { cJSON_CreateObject() }),
            |o: *mut CJson, c: *mut CJson| {
                let _ = (o, c);
            },
            "empty",
        );

        run_both(
            || {
                (
                    unsafe { cjson_create_object() },
                    unsafe { cJSON_CreateObject() },
                )
            },
            |o, c| unsafe {
                let k = cstr_buf(b"num");
                assert_same_null(cjson_add_number_to_object(o, k.as_ptr() as *const c_char, 1.5), cJSON_AddNumberToObject(c, k.as_ptr() as *const c_char, 1.5), "add_number");
                let k = cstr_buf(b"str");
                assert_same_null(cjson_add_string_to_object(o, k.as_ptr() as *const c_char, cstr_buf(b"hi").as_ptr() as *const c_char), cJSON_AddStringToObject(c, k.as_ptr() as *const c_char, cstr_buf(b"hi").as_ptr() as *const c_char), "add_string");
                let k = cstr_buf(b"nul");
                assert_same_null(cjson_add_null_to_object(o, k.as_ptr() as *const c_char), cJSON_AddNullToObject(c, k.as_ptr() as *const c_char), "add_null");
                let k = cstr_buf(b"t");
                assert_same_null(cjson_add_true_to_object(o, k.as_ptr() as *const c_char), cJSON_AddTrueToObject(c, k.as_ptr() as *const c_char), "add_true");
                let k = cstr_buf(b"f");
                assert_same_null(cjson_add_false_to_object(o, k.as_ptr() as *const c_char), cJSON_AddFalseToObject(c, k.as_ptr() as *const c_char), "add_false");
                let k = cstr_buf(b"b");
                assert_same_null(cjson_add_bool_to_object(o, k.as_ptr() as *const c_char, 0), cJSON_AddBoolToObject(c, k.as_ptr() as *const c_char, 0), "add_bool");
                let k = cstr_buf(b"raw");
                assert_same_null(cjson_add_raw_to_object(o, k.as_ptr() as *const c_char, cstr_buf(b"1e3").as_ptr() as *const c_char), cJSON_AddRawToObject(c, k.as_ptr() as *const c_char, cstr_buf(b"1e3").as_ptr() as *const c_char), "add_raw");
                let k = cstr_buf(b"obj");
                assert_same_null(cjson_add_object_to_object(o, k.as_ptr() as *const c_char), cJSON_AddObjectToObject(c, k.as_ptr() as *const c_char), "add_object");
                let k = cstr_buf(b"arr");
                assert_same_null(cjson_add_array_to_object(o, k.as_ptr() as *const c_char), cJSON_AddArrayToObject(c, k.as_ptr() as *const c_char), "add_array");
            },
            "build-object",
        );

        run_both(
            || {
                (
                    unsafe { cjson_create_array() },
                    unsafe { cJSON_CreateArray() },
                )
            },
            |o, c| unsafe {
                for i in 0..12 {
                    assert_eq!(
                        cjson_add_item_to_array(o, cjson_create_number(i as c_double)),
                        cJSON_AddItemToArray(c, cJSON_CreateNumber(i as c_double)),
                        "add_item_to_array {i}"
                    );
                }
                assert_eq!(cjson_get_array_size(o), cJSON_GetArraySize(c));
                // add a reference to element 3
                let el = cjson_get_array_item(o, 3);
                assert_eq!(cjson_add_item_reference_to_array(o, el), cJSON_AddItemReferenceToArray(c, cJSON_GetArrayItem(c, 3)));
                assert_eq!(cjson_get_array_size(o), cJSON_GetArraySize(c));
            },
            "build-array",
        );

        // mutation sequence with fixed indices
        run_both(
            || {
                let o = unsafe { cjson_create_object() };
                let c = unsafe { cJSON_CreateObject() };
                unsafe {
                    for i in 0..10 {
                        let name = cstr_buf(format!("k{i}").as_bytes());
                        cjson_add_number_to_object(o, name.as_ptr() as *const c_char, i as c_double);
                        cJSON_AddNumberToObject(c, name.as_ptr() as *const c_char, i as c_double);
                    }
                    // array member too
                    let name = cstr_buf(b"arr");
                    cjson_add_array_to_object(o, name.as_ptr() as *const c_char);
                    cJSON_AddArrayToObject(c, name.as_ptr() as *const c_char);
                    let arr_o = cjson_get_object_item(o, name.as_ptr() as *const c_char);
                    let arr_c = cJSON_GetObjectItem(c, name.as_ptr() as *const c_char);
                    for i in 0..5 {
                        cjson_add_item_to_array(arr_o, cjson_create_number(i as c_double));
                        cJSON_AddItemToArray(arr_c, cJSON_CreateNumber(i as c_double));
                    }
                }
                (o, c)
            },
            |o, c| unsafe {
                // delete array element 2
                let name = cstr_buf(b"arr");
                let arr_o = cjson_get_object_item(o, name.as_ptr() as *const c_char);
                let arr_c = cJSON_GetObjectItem(c, name.as_ptr() as *const c_char);
                cjson_delete_item_from_array(arr_o, 2);
                cJSON_DeleteItemFromArray(arr_c, 2);
                assert_print_eq(o, c, "after DeleteItemFromArray");

                // detach then delete object element k3
                let k3 = cstr_buf(b"k3");
                let det_o = cjson_detach_item_from_object(o, k3.as_ptr() as *const c_char);
                let det_c = cJSON_DetachItemFromObject(c, k3.as_ptr() as *const c_char);
                assert_eq!(det_o.is_null(), det_c.is_null(), "detach nullness");
                cjson_delete(det_o);
                cJSON_Delete(det_c);
                assert_print_eq(o, c, "after detach k3");

                // replace array element 0
                let n_o = cjson_create_string(cstr_buf(b"replaced").as_ptr() as *const c_char);
                let n_c = cJSON_CreateString(cstr_buf(b"replaced").as_ptr() as *const c_char);
                assert_eq!(
                    cjson_replace_item_in_array(arr_o, 0, n_o),
                    cJSON_ReplaceItemInArray(arr_c, 0, n_c),
                    "replace_item_in_array"
                );
                assert_print_eq(o, c, "after replace-in-array");

                // insert array element at 1
                let i_o = cjson_create_number(99.0);
                let i_c = cJSON_CreateNumber(99.0);
                assert_eq!(
                    cjson_insert_item_in_array(arr_o, 1, i_o),
                    cJSON_InsertItemInArray(arr_c, 1, i_c),
                    "insert_item_in_array"
                );
                assert_print_eq(o, c, "after insert-in-array");

                // replace object member
                let k7 = cstr_buf(b"k7");
                assert_eq!(
                    cjson_replace_item_in_object(o, k7.as_ptr() as *const c_char, cjson_create_null()),
                    cJSON_ReplaceItemInObject(c, k7.as_ptr() as *const c_char, cJSON_CreateNull()),
                    "replace_item_in_object"
                );
                assert_print_eq(o, c, "after replace-in-object");

                // replace object member case sensitive
                let k9 = cstr_buf(b"k9");
                assert_eq!(
                    cjson_replace_item_in_object_case_sensitive(o, k9.as_ptr() as *const c_char, cjson_create_true()),
                    cJSON_ReplaceItemInObjectCaseSensitive(c, k9.as_ptr() as *const c_char, cJSON_CreateTrue()),
                    "replace_item_in_object_case_sensitive"
                );
                assert_print_eq(o, c, "after replace-in-object-cs");

                // GetObjectItem case-insensitive + HasObjectItem
                let mix = cstr_buf(b"K0");
                let got_o = cjson_get_object_item(o, mix.as_ptr() as *const c_char);
                let got_c = cJSON_GetObjectItem(c, mix.as_ptr() as *const c_char);
                assert_eq!(got_o.is_null(), got_c.is_null());
                let got_cs_o = cjson_get_object_item_case_sensitive(o, mix.as_ptr() as *const c_char);
                let got_cs_c = cJSON_GetObjectItemCaseSensitive(c, mix.as_ptr() as *const c_char);
                assert_eq!(got_cs_o.is_null(), got_cs_c.is_null(), "case sensitive should miss K0");

                // SetValuestring on a string
                let kstr = cstr_buf(b"str");
                let sv_o = cjson_get_object_item(o, kstr.as_ptr() as *const c_char);
                let sv_c = cJSON_GetObjectItem(c, kstr.as_ptr() as *const c_char);
                if !sv_o.is_null() {
                    let newv = cstr_buf(b"changed!longer");
                    let r_o = cjson_set_valuestring(sv_o, newv.as_ptr() as *const c_char);
                    let r_c = cJSON_SetValuestring(sv_c, newv.as_ptr() as *const c_char);
                    assert_eq!(r_o.is_null(), r_c.is_null(), "set_valuestring");
                }
                assert_print_eq(o, c, "after set_valuestring");

                // SetNumberHelper
                let knum = cstr_buf(b"num");
                let sn_o = cjson_get_object_item(o, knum.as_ptr() as *const c_char);
                let sn_c = cJSON_GetObjectItem(c, knum.as_ptr() as *const c_char);
                if !sn_o.is_null() {
                    assert_eq!(
                        cjson_set_number_helper(sn_o, 2.75),
                        cJSON_SetNumberHelper(sn_c, 2.75),
                        "set_number_helper"
                    );
                }
                assert_print_eq(o, c, "after set_number_helper");

                // AddItemReferenceToObject
                let ka = cstr_buf(b"k1");
                let ref_o = cjson_get_object_item(o, ka.as_ptr() as *const c_char);
                let ref_c = cJSON_GetObjectItem(c, ka.as_ptr() as *const c_char);
                let kn = cstr_buf(b"ref");
                assert_eq!(
                    cjson_add_item_reference_to_object(o, kn.as_ptr() as *const c_char, ref_o),
                    cJSON_AddItemReferenceToObject(c, kn.as_ptr() as *const c_char, ref_c),
                    "add_item_reference_to_object"
                );
                assert_print_eq(o, c, "after add-reference");

                // AddItemToObjectCS
                let kcs = cstr_buf(b"constkey");
                let item_o = cjson_create_string(cstr_buf(b"cs").as_ptr() as *const c_char);
                let item_c = cJSON_CreateString(cstr_buf(b"cs").as_ptr() as *const c_char);
                assert_eq!(
                    cjson_add_item_to_object_cs(o, kcs.as_ptr() as *const c_char, item_o),
                    cJSON_AddItemToObjectCS(c, kcs.as_ptr() as *const c_char, item_c),
                    "add_item_to_object_cs"
                );
                assert_print_eq(o, c, "after add-item-cs");

                // DeleteItemFromObjectCaseSensitive on a member
                let kd = cstr_buf(b"K9");
                cjson_delete_item_from_object_case_sensitive(o, kd.as_ptr() as *const c_char);
                cJSON_DeleteItemFromObjectCaseSensitive(c, kd.as_ptr() as *const c_char);
                assert_print_eq(o, c, "after delete-item-cs");

                // Duplicate (deep) and compare
                let dup_o = cjson_duplicate(o, 1);
                let dup_c = cJSON_Duplicate(c, 1);
                assert!(!dup_o.is_null() && !dup_c.is_null());
                assert_eq!(cjson_compare(o, dup_o, 1), cJSON_Compare(c, dup_c, 1), "compare");
                assert_print_eq(dup_o, dup_c, "duplicate");
                cjson_delete(dup_o);
                cJSON_Delete(dup_c);

                // shallow duplicate
                let sh_o = cjson_duplicate(o, 0);
                let sh_c = cJSON_Duplicate(c, 0);
                assert!(!sh_o.is_null() && !sh_c.is_null());
                assert_print_eq(sh_o, sh_c, "shallow-duplicate");
                cjson_delete(sh_o);
                cJSON_Delete(sh_c);
            },
            "mutate",
        );
    });
}

// ---- minify differential ---------------------------------------------------

#[test]
fn differential_minify() {
    with_lock(|| {
        let cases: Vec<Vec<u8>> = vec![
            b"  { \"a\" : 1 }  ".to_vec(),
            b"{\n  \"a\": 1,\n  \"b\": [1,2,3]\n}".to_vec(),
            b"// comment\n{\"a\":1}".to_vec(),
            b"/* block */ { \"a\" : \"x\" }".to_vec(),
            b"[1, 2, 3]".to_vec(),
            b"{\"s\":\"hello \\\"world\\\"\"}".to_vec(),
            b"{\"s\":\"a/b\"}".to_vec(),
            b"   \t\r\n".to_vec(),
            b"\"\\/\"".to_vec(),
            b"{\"a\":true,\"b\":false,\"c\":null}".to_vec(),
        ];
        for case in cases {
            let mut a = cstr_buf(&case);
            let mut b = cstr_buf(&case);
            unsafe {
                cjson_minify(a.as_mut_ptr() as *mut c_char);
                cJSON_Minify(b.as_mut_ptr() as *mut c_char);
            }
            assert_eq!(
                cstr_bytes(a.as_ptr() as *const c_char),
                cstr_bytes(b.as_ptr() as *const c_char),
                "minify mismatch for {:?}",
                case
            );
        }
    });
}

// ---- type predicates / accessors -------------------------------------------

#[test]
fn differential_predicates() {
    with_lock(|| {
        let o = unsafe { cjson_create_object() };
        let c = unsafe { cJSON_CreateObject() };
        let a = unsafe { cjson_create_array() };
        let ac = unsafe { cJSON_CreateArray() };
        let n = unsafe { cjson_create_number(3.0) };
        let nc = unsafe { cJSON_CreateNumber(3.0) };
        let s = unsafe { cjson_create_string(cstr_buf(b"x").as_ptr() as *const c_char) };
        let sc = unsafe { cJSON_CreateString(cstr_buf(b"x").as_ptr() as *const c_char) };
        unsafe {
            assert_eq!(cjson_is_object(o), cJSON_IsObject(c));
            assert_eq!(cjson_is_array(a), cJSON_IsArray(ac));
            assert_eq!(cjson_is_number(n), cJSON_IsNumber(nc));
            assert_eq!(cjson_is_string(s), cJSON_IsString(sc));
            assert_eq!(cjson_is_invalid(o), cJSON_IsInvalid(c));
            assert_eq!(cjson_is_null(o), cJSON_IsNull(c));
            // accessors
            assert_eq!(cjson_get_number_value(n), cJSON_GetNumberValue(nc));
            assert_eq!(
                cstr_bytes(cjson_get_string_value(s)),
                cstr_bytes(cJSON_GetStringValue(sc))
            );
            // version
            assert_eq!(
                cstr_bytes(cjson_version()),
                cstr_bytes(cJSON_Version())
            );
            // GetArrayItem on NULL array / out of range
            assert_eq!(cjson_get_array_item(ptr::null(), 0).is_null(), cJSON_GetArrayItem(ptr::null(), 0).is_null());
            assert_eq!(cjson_get_array_item(a, 0).is_null(), cJSON_GetArrayItem(ac, 0).is_null());
            assert_eq!(cjson_get_array_item(a, -1).is_null(), cJSON_GetArrayItem(ac, -1).is_null());
            // GetNumberValue on non-number (NAN)
            assert_eq!(
                cjson_get_number_value(o).is_nan(),
                cJSON_GetNumberValue(c).is_nan()
            );
        }
        unsafe {
            cjson_delete(o);
            cJSON_Delete(c);
            cjson_delete(a);
            cJSON_Delete(ac);
            cjson_delete(n);
            cJSON_Delete(nc);
            cjson_delete(s);
            cJSON_Delete(sc);
        }
    });
}

// ---- fuzz ----------------------------------------------------------------

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

/// Apply a random manipulation to both trees; indices are bounded by current size.
fn random_mutate(rng: &mut Rng, o: *mut CJson, c: *mut CJson) {
    unsafe {
        let size = cjson_get_array_size(o) as usize;
        let osize = cJSON_GetArraySize(c) as usize;
        assert_eq!(size, osize, "array sizes out of sync");
        let idx = if size == 0 { 0 } else { rng.below(size) };
        let name = cstr_buf(format!("k{}", rng.below(7)).as_bytes());
        let newnum = (rng.below(1000) as i32 - 500) as c_double;
        let rbool = rng.below(2) as c_int;

        match rng.below(12) {
            0 => {
                // add number to object
                assert_same_null(
                    cjson_add_number_to_object(o, name.as_ptr() as *const c_char, newnum),
                    cJSON_AddNumberToObject(c, name.as_ptr() as *const c_char, newnum),
                    "fuzz add_number",
                );
            }
            1 => {
                // add string to object
                assert_same_null(
                    cjson_add_string_to_object(o, name.as_ptr() as *const c_char, name.as_ptr() as *const c_char),
                    cJSON_AddStringToObject(c, name.as_ptr() as *const c_char, name.as_ptr() as *const c_char),
                    "fuzz add_string",
                );
            }
            2 => {
                // add bool to object
                assert_same_null(
                    cjson_add_bool_to_object(o, name.as_ptr() as *const c_char, rbool),
                    cJSON_AddBoolToObject(c, name.as_ptr() as *const c_char, rbool),
                    "fuzz add_bool",
                );
            }
            3 => {
                // add raw to object
                assert_same_null(
                    cjson_add_raw_to_object(o, name.as_ptr() as *const c_char, cstr_buf(b"1234").as_ptr() as *const c_char),
                    cJSON_AddRawToObject(c, name.as_ptr() as *const c_char, cstr_buf(b"1234").as_ptr() as *const c_char),
                    "fuzz add_raw",
                );
            }
            4 => {
                // add array to object
                assert_same_null(
                    cjson_add_array_to_object(o, name.as_ptr() as *const c_char),
                    cJSON_AddArrayToObject(c, name.as_ptr() as *const c_char),
                    "fuzz add_array",
                );
            }
            5 => {
                // add object to object
                assert_same_null(
                    cjson_add_object_to_object(o, name.as_ptr() as *const c_char),
                    cJSON_AddObjectToObject(c, name.as_ptr() as *const c_char),
                    "fuzz add_object",
                );
            }
            6 => {
                // add item to array (via cjson_add_item_to_array)
                assert_eq!(
                    cjson_add_item_to_array(o, cjson_create_number(newnum)),
                    cJSON_AddItemToArray(c, cJSON_CreateNumber(newnum))
                );
            }
            7 => {
                // delete item from array
                if size > 0 {
                    cjson_delete_item_from_array(o, idx as c_int);
                    cJSON_DeleteItemFromArray(c, idx as c_int);
                }
            }
            8 => {
                // delete item from object
                cjson_delete_item_from_object(o, name.as_ptr() as *const c_char);
                cJSON_DeleteItemFromObject(c, name.as_ptr() as *const c_char);
            }
            9 => {
                // detach item from array then delete
                if size > 0 {
                    let det_o = cjson_detach_item_from_array(o, idx as c_int);
                    let det_c = cJSON_DetachItemFromArray(c, idx as c_int);
                    assert_eq!(det_o.is_null(), det_c.is_null());
                    cjson_delete(det_o);
                    cJSON_Delete(det_c);
                }
            }
            10 => {
                // replace item in array
                if size > 0 {
                    assert_eq!(
                        cjson_replace_item_in_array(o, idx as c_int, cjson_create_string(name.as_ptr() as *const c_char)),
                        cJSON_ReplaceItemInArray(c, idx as c_int, cJSON_CreateString(name.as_ptr() as *const c_char))
                    );
                }
            }
            11 => {
                // insert item in array
                assert_eq!(
                    cjson_insert_item_in_array(o, idx as c_int, cjson_create_number(newnum)),
                    cJSON_InsertItemInArray(c, idx as c_int, cJSON_CreateNumber(newnum))
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn differential_manip_fuzz() {
    with_lock(|| {
        let mut rng = Rng(0x0FACADE);
        let o = unsafe { cjson_create_object() };
        let c = unsafe { cJSON_CreateObject() };

        // seed with a nested array
        let na = cstr_buf(b"arr");
        unsafe {
            cjson_add_array_to_object(o, na.as_ptr() as *const c_char);
            cJSON_AddArrayToObject(c, na.as_ptr() as *const c_char);
            let arr_o = cjson_get_object_item(o, na.as_ptr() as *const c_char);
            let arr_c = cJSON_GetObjectItem(c, na.as_ptr() as *const c_char);
            for i in 0..8 {
                cjson_add_item_to_array(arr_o, cjson_create_number(i as c_double));
                cJSON_AddItemToArray(arr_c, cJSON_CreateNumber(i as c_double));
            }
        }

        for step in 0..5000u64 {
            random_mutate(&mut rng, o, c);
            // occasionally also compare duplicate/compare
            if step % 500 == 0 {
                let d_o = unsafe { cjson_duplicate(o, 1) };
                let d_c = unsafe { cJSON_Duplicate(c, 1) };
                assert!(!d_o.is_null() && !d_c.is_null());
                assert_eq!(
                    unsafe { cjson_compare(o, d_o, 1) },
                    unsafe { cJSON_Compare(c, d_c, 1) }
                );
                assert_print_eq(d_o, d_c, "fuzz duplicate");
                unsafe {
                    cjson_delete(d_o);
                    cJSON_Delete(d_c);
                }
            }
            assert_print_eq(o, c, &format!("fuzz step {step}"));
        }

        unsafe {
            cjson_delete(o);
            cJSON_Delete(c);
        }
    });
}
