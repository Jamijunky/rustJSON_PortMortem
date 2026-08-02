//! Differential fuzzing against the reference C cJSON, always-on as part of
//! `cargo test`. The default iteration budget is small so the suite stays
//! fast; set `CJSON_FUZZ_ITERS` to scale up (the heavy campaign lives in
//! `src/bin/fuzz_differential.rs`, run by `scripts/fuzz_differential.sh`).

mod common;

use std::ffi::{c_char, c_int, c_void};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use common::{
    assert_trees_equal, mutate_bytes, random_container_doc, random_doc, random_patch,
    random_pointer_json, Rng,
};

use cjson::alloc::{cjson_free, current_hooks};
use cjson::manip::{
    cjson_add_item_to_array, cjson_add_item_to_object, cjson_create_number, cjson_create_string,
    cjson_delete, cjson_delete_item_from_array, cjson_delete_item_from_object,
    cjson_detach_item_from_array, cjson_replace_item_in_array,
};
use cjson::model::{CJson, CJsonBool};
use cjson::parse::{cjson_parse_with_length_opts, get_error_ptr};
use cjson::print::{cjson_print, cjson_print_unformatted};
use cjson::utils::{
    cjson_utils_apply_patches_case_sensitive, cjson_utils_get_pointer_case_sensitive,
    cjson_utils_merge_patch_case_sensitive, cjson_utils_sort_object_case_sensitive,
};

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
    fn ref_cJSON_GetErrorPtr() -> *const c_char;
    fn ref_cJSON_Delete(item: *mut CJson);
    fn ref_cJSON_Print(item: *const CJson) -> *mut c_char;
    fn ref_cJSON_PrintUnformatted(item: *const CJson) -> *mut c_char;
    fn ref_cJSON_CreateString(string: *const c_char) -> *mut CJson;
    fn ref_cJSON_CreateNumber(num: f64) -> *mut CJson;
    fn ref_cJSON_AddItemToArray(array: *mut CJson, item: *mut CJson) -> CJsonBool;
    fn ref_cJSON_AddItemToObject(
        object: *mut CJson,
        string: *const c_char,
        item: *mut CJson,
    ) -> CJsonBool;
    fn ref_cJSON_DeleteItemFromArray(array: *mut CJson, which: c_int);
    fn ref_cJSON_DeleteItemFromObject(object: *mut CJson, string: *const c_char);
    fn ref_cJSON_DetachItemFromArray(array: *mut CJson, which: c_int) -> *mut CJson;
    fn ref_cJSON_ReplaceItemInArray(
        array: *mut CJson,
        which: c_int,
        newitem: *mut CJson,
    ) -> CJsonBool;
    fn ref_cJSONUtils_ApplyPatchesCaseSensitive(object: *mut CJson, patches: *const CJson)
        -> c_int;
    fn ref_cJSONUtils_MergePatchCaseSensitive(
        target: *mut CJson,
        patch: *const CJson,
    ) -> *mut CJson;
    fn ref_cJSONUtils_GetPointerCaseSensitive(
        object: *mut CJson,
        pointer: *const c_char,
    ) -> *mut CJson;
    fn ref_cJSONUtils_SortObjectCaseSensitive(object: *mut CJson);
    fn free(ptr: *mut c_void);
}

// ---- wrappers --------------------------------------------------------------

fn iters() -> usize {
    std::env::var("CJSON_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
}

fn cstr_bytes_of(p: *mut c_char) -> Vec<u8> {
    if p.is_null() {
        return Vec::new();
    }
    unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes().to_vec()
}

fn port_print(item: *const CJson) -> Vec<u8> {
    let p = unsafe { cjson_print_unformatted(item) };
    let bytes = cstr_bytes_of(p);
    let hooks = unsafe { current_hooks() };
    unsafe { cjson_free(&hooks, p as *mut c_void) };
    bytes
}

fn ref_print(item: *const CJson) -> Vec<u8> {
    let p = unsafe { ref_cJSON_PrintUnformatted(item) };
    let bytes = cstr_bytes_of(p);
    unsafe { free(p as *mut c_void) };
    bytes
}

fn port_free(item: *mut CJson) {
    if !item.is_null() {
        unsafe { cjson_delete(item) };
    }
}

fn ref_free(item: *mut CJson) {
    if !item.is_null() {
        unsafe { ref_cJSON_Delete(item) };
    }
}

fn port_parse(input: &[u8]) -> *mut CJson {
    let mut v = input.to_vec();
    v.push(0);
    let mut end: *const c_char = ptr::null();
    unsafe { cjson_parse_with_length_opts(v.as_ptr() as *const c_char, input.len(), &mut end, 0) }
}

fn ref_parse(input: &[u8]) -> *mut CJson {
    let mut v = input.to_vec();
    v.push(0);
    let mut end: *const c_char = ptr::null();
    unsafe { ref_cJSON_ParseWithLengthOpts(v.as_ptr() as *const c_char, input.len(), &mut end, 0) }
}

/// Compare a parse of the same input on both sides: outcome, tree, error
/// offset, and parse-end offset must all agree.
fn run_parse_case(input: &[u8]) {
    let mut oc = input.to_vec();
    oc.push(0);
    let mut rc = input.to_vec();
    rc.push(0);
    let o_base = oc.as_ptr() as *const c_char;
    let r_base = rc.as_ptr() as *const c_char;
    let mut o_end: *const c_char = ptr::null();
    let mut r_end: *const c_char = ptr::null();

    let ours = unsafe { cjson_parse_with_length_opts(o_base, input.len(), &mut o_end, 0) };
    let refs = unsafe { ref_cJSON_ParseWithLengthOpts(r_base, input.len(), &mut r_end, 0) };

    if ours.is_null() != refs.is_null() {
        port_free(ours);
        ref_free(refs);
        panic!(
            "parse outcome mismatch for {:?}: ours={} refs={}",
            String::from_utf8_lossy(input),
            ours.is_null(),
            refs.is_null()
        );
    }
    if !ours.is_null() {
        assert_trees_equal(ours, refs, "root");
    }

    let o_err = unsafe { get_error_ptr() };
    let r_err = unsafe { ref_cJSON_GetErrorPtr() };
    let o_err_off = if o_err.is_null() {
        None
    } else {
        Some(unsafe { o_err.offset_from(o_base) })
    };
    let r_err_off = if r_err.is_null() {
        None
    } else {
        Some(unsafe { r_err.offset_from(r_base) })
    };
    assert_eq!(
        o_err_off,
        r_err_off,
        "error offset mismatch for {:?}",
        String::from_utf8_lossy(input)
    );

    let o_end_off = if o_end.is_null() {
        None
    } else {
        Some(unsafe { o_end.offset_from(o_base) })
    };
    let r_end_off = if r_end.is_null() {
        None
    } else {
        Some(unsafe { r_end.offset_from(r_base) })
    };
    assert_eq!(
        o_end_off,
        r_end_off,
        "parse-end offset mismatch for {:?}",
        String::from_utf8_lossy(input)
    );

    port_free(ours);
    ref_free(refs);
}

/// A raw (unquoted) string value for `cJSON_CreateString`.
fn raw_string(rng: &mut Rng) -> String {
    let n = 1 + rng.below(6);
    (0..n)
        .map(|_| (b'a' + rng.below(26) as u8) as char)
        .collect()
}

// ---- tests ----------------------------------------------------------------

#[test]
fn fuzz_parse_reference() {
    let n = iters();
    let seed = 0xFEED_0001u64;
    with_lock(|| {
        let mut rng = Rng::new(seed);
        for i in 0..n {
            let base = random_doc(&mut rng);
            let input = if i % 2 == 0 {
                base
            } else {
                mutate_bytes(&mut rng, base)
            };
            run_parse_case(&input);
        }
    });
}

#[test]
fn fuzz_print_and_roundtrip_reference() {
    let n = iters() / 2 + 1;
    let seed = 0xFEED_0002u64;
    with_lock(|| {
        let mut rng = Rng::new(seed);
        for i in 0..n {
            let input = random_doc(&mut rng);
            let ours = port_parse(&input);
            let refs = ref_parse(&input);
            if ours.is_null() || refs.is_null() {
                port_free(ours);
                ref_free(refs);
                continue;
            }

            let p_uf = port_print(ours);
            let r_uf = ref_print(refs);
            assert_eq!(
                p_uf,
                r_uf,
                "unformatted print mismatch (case {i}) for {:?}",
                String::from_utf8_lossy(&input)
            );

            let p_f = {
                let p = unsafe { cjson_print(ours) };
                let bytes = cstr_bytes_of(p);
                let hooks = unsafe { current_hooks() };
                unsafe { cjson_free(&hooks, p as *mut c_void) };
                bytes
            };
            let r_f = {
                let p = unsafe { ref_cJSON_Print(refs) };
                let bytes = cstr_bytes_of(p);
                unsafe { free(p as *mut c_void) };
                bytes
            };
            assert_eq!(
                p_f,
                r_f,
                "formatted print mismatch (case {i}) for {:?}",
                String::from_utf8_lossy(&input)
            );

            // Round-trip cross-check: both sides printed byte-identical output
            // (asserted above); re-parsing that output must give the same tree
            // on both sides. (A strict parse(print(x)) == parse(x) check is
            // intentionally NOT used: cJSON itself flattens -0.0 to 0 on print,
            // so round-trips are not bit-stable by design in the reference.)
            let re_p = {
                let mut v = p_uf.clone();
                v.push(0);
                let mut end: *const c_char = ptr::null();
                unsafe {
                    cjson_parse_with_length_opts(
                        v.as_ptr() as *const c_char,
                        p_uf.len(),
                        &mut end,
                        0,
                    )
                }
            };
            let re_r = {
                let mut v = r_uf.clone();
                v.push(0);
                let mut end: *const c_char = ptr::null();
                unsafe {
                    ref_cJSON_ParseWithLengthOpts(
                        v.as_ptr() as *const c_char,
                        r_uf.len(),
                        &mut end,
                        0,
                    )
                }
            };
            assert_eq!(
                re_p.is_null(),
                re_r.is_null(),
                "round-trip reparse null parity mismatch (case {i}) for {:?}",
                String::from_utf8_lossy(&input)
            );
            if !re_p.is_null() {
                assert_trees_equal(re_p, re_r, "round-trip cross-check");
            }
            port_free(re_p);
            ref_free(re_r);

            port_free(ours);
            ref_free(refs);
        }
    });
}

#[test]
fn fuzz_manip_reference() {
    let n = iters() / 4 + 1;
    let seed = 0xFEED_0003u64;
    with_lock(|| {
        let mut rng = Rng::new(seed);
        for i in 0..n {
            let input = random_container_doc(&mut rng);
            let ours = port_parse(&input);
            let refs = ref_parse(&input);
            if ours.is_null() || refs.is_null() {
                port_free(ours);
                ref_free(refs);
                continue;
            }

            let ops = rng.below(6);
            for _ in 0..ops {
                let o = unsafe { &*ours };
                let r = unsafe { &*refs };
                match (o.type_, r.type_) {
                    (0x06, 0x06) => {
                        // array root: add/delete/detach/replace
                        let idx = rng.below(4) as c_int;
                        let num = rng.below(1000) as f64;
                        match rng.below(5) {
                            0 => {
                                let p = unsafe { cjson_create_number(num) };
                                let q = unsafe { ref_cJSON_CreateNumber(num) };
                                unsafe { cjson_add_item_to_array(ours, p) };
                                unsafe { ref_cJSON_AddItemToArray(refs, q) };
                            }
                            1 => {
                                let s = raw_string(&mut rng);
                                let mut pv = s.clone().into_bytes();
                                pv.push(0);
                                let qv = pv.clone();
                                let p =
                                    unsafe { cjson_create_string(pv.as_ptr() as *const c_char) };
                                let q =
                                    unsafe { ref_cJSON_CreateString(qv.as_ptr() as *const c_char) };
                                unsafe { cjson_add_item_to_array(ours, p) };
                                unsafe { ref_cJSON_AddItemToArray(refs, q) };
                            }
                            2 => {
                                unsafe { cjson_delete_item_from_array(ours, idx) };
                                unsafe { ref_cJSON_DeleteItemFromArray(refs, idx) };
                            }
                            3 => {
                                let p = unsafe { cjson_detach_item_from_array(ours, idx) };
                                let q = unsafe { ref_cJSON_DetachItemFromArray(refs, idx) };
                                port_free(p);
                                ref_free(q);
                            }
                            _ => {
                                let p = unsafe { cjson_create_number(num) };
                                let q = unsafe { ref_cJSON_CreateNumber(num) };
                                unsafe { cjson_replace_item_in_array(ours, idx, p) };
                                unsafe { ref_cJSON_ReplaceItemInArray(refs, idx, q) };
                            }
                        }
                    }
                    (0x07, 0x07) => {
                        // object root: add/delete/replace/sort
                        let key = [b"a"[0], b"b"[0], b"c"[0], b"x"[0]][rng.below(4)];
                        let key_c = [key, 0];
                        let num = rng.below(1000) as f64;
                        match rng.below(4) {
                            0 => {
                                let p = unsafe { cjson_create_number(num) };
                                let q = unsafe { ref_cJSON_CreateNumber(num) };
                                unsafe {
                                    cjson_add_item_to_object(
                                        ours,
                                        key_c.as_ptr() as *const c_char,
                                        p,
                                    )
                                };
                                unsafe {
                                    ref_cJSON_AddItemToObject(
                                        refs,
                                        key_c.as_ptr() as *const c_char,
                                        q,
                                    )
                                };
                            }
                            1 => {
                                unsafe {
                                    cjson_delete_item_from_object(
                                        ours,
                                        key_c.as_ptr() as *const c_char,
                                    )
                                };
                                unsafe {
                                    ref_cJSON_DeleteItemFromObject(
                                        refs,
                                        key_c.as_ptr() as *const c_char,
                                    )
                                };
                            }
                            2 => {
                                let s = raw_string(&mut rng);
                                let mut pv = s.clone().into_bytes();
                                pv.push(0);
                                let qv = pv.clone();
                                let p =
                                    unsafe { cjson_create_string(pv.as_ptr() as *const c_char) };
                                let q =
                                    unsafe { ref_cJSON_CreateString(qv.as_ptr() as *const c_char) };
                                unsafe {
                                    cjson_add_item_to_object(
                                        ours,
                                        key_c.as_ptr() as *const c_char,
                                        p,
                                    )
                                };
                                unsafe {
                                    ref_cJSON_AddItemToObject(
                                        refs,
                                        key_c.as_ptr() as *const c_char,
                                        q,
                                    )
                                };
                            }
                            _ => {
                                unsafe { cjson_utils_sort_object_case_sensitive(ours) };
                                unsafe { ref_cJSONUtils_SortObjectCaseSensitive(refs) };
                            }
                        }
                    }
                    _ => break,
                }

                let p_out = port_print(ours);
                let r_out = ref_print(refs);
                assert_eq!(
                    p_out,
                    r_out,
                    "manip mismatch (case {i}) for {:?}",
                    String::from_utf8_lossy(&input)
                );
            }

            port_free(ours);
            ref_free(refs);
        }
    });
}

#[test]
fn fuzz_utils_patches_reference() {
    let n = iters() / 4 + 1;
    let seed = 0xFEED_0004u64;
    with_lock(|| {
        let mut rng = Rng::new(seed);
        for i in 0..n {
            let doc_json = random_doc(&mut rng);
            let patch_json = random_patch(&mut rng);
            let doc_p = port_parse(&doc_json);
            let doc_r = ref_parse(&doc_json);
            let patch_p = port_parse(patch_json.as_bytes());
            let patch_r = ref_parse(patch_json.as_bytes());
            if doc_p.is_null() || doc_r.is_null() || patch_p.is_null() || patch_r.is_null() {
                port_free(doc_p);
                port_free(patch_p);
                ref_free(doc_r);
                ref_free(patch_r);
                continue;
            }

            let rc_p = unsafe { cjson_utils_apply_patches_case_sensitive(doc_p, patch_p) };
            let rc_r = unsafe { ref_cJSONUtils_ApplyPatchesCaseSensitive(doc_r, patch_r) };
            assert_eq!(
                rc_p,
                rc_r,
                "patch rc mismatch (case {i}) for doc={}, patch={}",
                String::from_utf8_lossy(&doc_json),
                patch_json
            );
            if rc_p == 0 {
                assert_eq!(
                    port_print(doc_p),
                    ref_print(doc_r),
                    "patch result mismatch (case {i}) for doc={}, patch={}",
                    String::from_utf8_lossy(&doc_json),
                    patch_json
                );
            }

            port_free(doc_p);
            port_free(patch_p);
            ref_free(doc_r);
            ref_free(patch_r);
        }
    });
}

#[test]
fn fuzz_utils_merge_and_pointer_reference() {
    let n = iters() / 4 + 1;
    let seed = 0xFEED_0005u64;
    with_lock(|| {
        let mut rng = Rng::new(seed);
        for i in 0..n {
            let target_json = random_doc(&mut rng);
            let patch_json = random_doc(&mut rng);
            let target_p = port_parse(&target_json);
            let patch_p = port_parse(&patch_json);
            let target_r = ref_parse(&target_json);
            let patch_r = ref_parse(&patch_json);
            if target_p.is_null() || target_r.is_null() || patch_p.is_null() || patch_r.is_null() {
                port_free(target_p);
                port_free(patch_p);
                ref_free(target_r);
                ref_free(patch_r);
                continue;
            }

            // Pointer lookup borrows the target, so do it before merge_patch
            // consumes the target.
            let ptr_json = random_pointer_json(&mut rng);
            let mut pv = ptr_json.as_bytes().to_vec();
            pv.push(0);
            let found_p = unsafe { cjson_utils_get_pointer_case_sensitive(target_p, pv.as_ptr()) };
            let found_r = unsafe {
                ref_cJSONUtils_GetPointerCaseSensitive(target_r, pv.as_ptr() as *const c_char)
            };
            assert_eq!(
                found_p.is_null(),
                found_r.is_null(),
                "pointer null parity mismatch (case {i}) for {}",
                ptr_json
            );

            // MergePatch CONSUMES the target (it may return the same pointer or
            // delete it and return a fresh duplicate). Only the result and the
            // patch remain owned by the caller.
            let merged_p = unsafe { cjson_utils_merge_patch_case_sensitive(target_p, patch_p) };
            let merged_r = unsafe { ref_cJSONUtils_MergePatchCaseSensitive(target_r, patch_r) };
            assert_eq!(
                merged_p.is_null(),
                merged_r.is_null(),
                "merge-patch null parity mismatch (case {i})"
            );
            if !merged_p.is_null() {
                assert_eq!(
                    port_print(merged_p),
                    ref_print(merged_r),
                    "merge-patch result mismatch (case {i}) for target={}, patch={}",
                    String::from_utf8_lossy(&target_json),
                    String::from_utf8_lossy(&patch_json)
                );
            }
            port_free(merged_p);
            ref_free(merged_r);
            port_free(patch_p);
            ref_free(patch_r);
        }
    });
}
