//! Differential tests for the `cJSON_Utils` port.
//!
//! Every operation is executed against the real reference C library (linked
//! from the pristine upstream sources with `ref_`-prefixed symbols, see
//! `bench_ref_rename.h` / cjson-ref-sys) and against the Rust port (called through
//! its internal entry points). The two must produce identical return codes and
//! byte-identical output for the same input.

use core::ffi::{c_char, c_int, c_void};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use cjson::alloc::{cjson_free, current_hooks};
use cjson::manip::{cjson_delete, cjson_duplicate, cjson_is_true};
use cjson::model::CJson;
use cjson::parse::cjson_parse_with_length_opts;
use cjson::print::cjson_print_unformatted;
use cjson::utils::{
    cjson_utils_apply_patches_case_sensitive, cjson_utils_generate_merge_patch_case_sensitive,
    cjson_utils_generate_patches_case_sensitive, cjson_utils_get_pointer_case_sensitive,
    cjson_utils_merge_patch_case_sensitive, cjson_utils_sort_object_case_sensitive,
};
use cjson_ref_sys as _;

// ---- reference C cJSON_Utils (symbol-prefixed, always the real C) ----------

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
    fn ref_cJSON_Duplicate(item: *const CJson, recurse: c_int) -> *mut CJson;
    fn ref_cJSONUtils_ApplyPatchesCaseSensitive(object: *mut CJson, patches: *const CJson)
        -> c_int;
    fn ref_cJSONUtils_GeneratePatchesCaseSensitive(from: *mut CJson, to: *mut CJson) -> *mut CJson;
    fn ref_cJSONUtils_MergePatchCaseSensitive(
        target: *mut CJson,
        patch: *const CJson,
    ) -> *mut CJson;
    fn ref_cJSONUtils_GenerateMergePatchCaseSensitive(
        from: *mut CJson,
        to: *mut CJson,
    ) -> *mut CJson;
    fn ref_cJSONUtils_GetPointerCaseSensitive(
        object: *mut CJson,
        pointer: *const c_char,
    ) -> *mut CJson;
    fn ref_cJSONUtils_SortObjectCaseSensitive(object: *mut CJson);
    fn free(ptr: *mut c_void);
}

// ---- global state serialization -------------------------------------------

/// Both implementations keep process-global parse state; serialize every
/// differential run so parallel test threads cannot race on it.
fn with_lock<R>(f: impl FnOnce() -> R) -> R {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();
    f()
}

// ---- helpers ---------------------------------------------------------------

fn with_nul(s: &[u8]) -> Vec<u8> {
    let mut v = s.to_vec();
    v.push(0);
    v
}

fn cstr_bytes(p: *const c_char) -> Vec<u8> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    let mut i = 0usize;
    unsafe {
        while *p.add(i) != 0 {
            v.push(*p.add(i) as u8);
            i += 1;
        }
    }
    v
}

fn port_parse(s: &[u8]) -> *mut CJson {
    let v = with_nul(s);
    unsafe {
        cjson_parse_with_length_opts(v.as_ptr() as *const c_char, v.len() - 1, ptr::null_mut(), 0)
    }
}

fn ref_parse(s: &[u8]) -> *mut CJson {
    let v = with_nul(s);
    unsafe {
        ref_cJSON_ParseWithLengthOpts(v.as_ptr() as *const c_char, v.len() - 1, ptr::null_mut(), 0)
    }
}

fn port_print(item: *const CJson) -> Vec<u8> {
    let p = unsafe { cjson_print_unformatted(item) };
    let bytes = cstr_bytes(p);
    if !p.is_null() {
        let hooks = unsafe { current_hooks() };
        unsafe { cjson_free(&hooks, p as *mut c_void) };
    }
    bytes
}

fn ref_print(item: *const CJson) -> Vec<u8> {
    let p = unsafe { ref_cJSON_PrintUnformatted(item) };
    let bytes = cstr_bytes(p);
    if !p.is_null() {
        unsafe { free(p as *mut c_void) };
    }
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

fn child_named(item: *mut CJson, key: &[u8]) -> *mut CJson {
    let mut child = unsafe { (*item).child };
    while !child.is_null() {
        let name = unsafe { (*child).string };
        if !name.is_null() {
            let len = cstr_bytes(name);
            if len == key {
                return child;
            }
        }
        child = unsafe { (*child).next };
    }
    ptr::null_mut()
}

/// Replay every entry from the json-patch-tests corpus as (doc, patch) JSON
/// text pairs, skipping entries marked `"disabled": true` (mirroring the
/// upstream test runner).
fn corpus_entries() -> Vec<(Vec<u8>, Vec<u8>)> {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vendor/cjson-ref/tests/json-patch-tests");
    let mut out = Vec::new();
    for name in ["tests.json", "spec_tests.json", "cjson-utils-tests.json"] {
        let data = std::fs::read(base.join(name)).unwrap_or_else(|_| panic!("cannot read {name}"));
        let root = port_parse(&data);
        if root.is_null() {
            continue;
        }
        let mut entry = unsafe { (*root).child };
        while !entry.is_null() {
            let disabled = child_named(entry, b"disabled");
            if !disabled.is_null() && unsafe { cjson_is_true(disabled) } != 0 {
                entry = unsafe { (*entry).next };
                continue;
            }
            let doc = child_named(entry, b"doc");
            let patch = child_named(entry, b"patch");
            let doc_json = if doc.is_null() {
                b"null".to_vec()
            } else {
                port_print(doc)
            };
            let patch_json = if patch.is_null() {
                b"null".to_vec()
            } else {
                port_print(patch)
            };
            out.push((doc_json, patch_json));
            entry = unsafe { (*entry).next };
        }
        port_free(root);
    }
    out
}

// ---- ApplyPatches (RFC 6902) against the reference -------------------------

#[test]
fn apply_patches_matches_reference_c() {
    let corpus = corpus_entries();
    assert!(!corpus.is_empty(), "corpus should not be empty");
    with_lock(|| {
        for (i, (doc_json, patch_json)) in corpus.iter().enumerate() {
            let doc_p = port_parse(doc_json);
            let doc_r = ref_parse(doc_json);
            let patch_p = port_parse(patch_json);
            let patch_r = ref_parse(patch_json);
            assert!(
                !doc_p.is_null() && !doc_r.is_null(),
                "test {i}: parse doc failed"
            );
            assert!(
                !patch_p.is_null() && !patch_r.is_null(),
                "test {i}: parse patch failed"
            );

            let rc_p = unsafe { cjson_utils_apply_patches_case_sensitive(doc_p, patch_p) };
            let rc_r = unsafe { ref_cJSONUtils_ApplyPatchesCaseSensitive(doc_r, patch_r) };
            assert_eq!(rc_p, rc_r, "test {i}: rc mismatch (doc={doc_json:?})");
            if rc_p == 0 {
                assert_eq!(
                    port_print(doc_p),
                    ref_print(doc_r),
                    "test {i}: applied doc mismatch"
                );
            }

            port_free(doc_p);
            port_free(patch_p);
            ref_free(doc_r);
            ref_free(patch_r);
        }
    });
}

// ---- GeneratePatches (RFC 6902) against the reference ----------------------

const GENERATE_FIXTURES: [(&str, &str); 12] = [
    (r#"{"a":"b"}"#, r#"{"a":"c"}"#),
    (r#"{"a":"b","c":1}"#, r#"{"a":"b","d":2}"#),
    (r#"{"a":{},"b":1}"#, r#"{"a":{"x":1},"b":1}"#),
    (r#"{"a":[1,2,3]}"#, r#"{"a":[1,2,3,4]}"#),
    (r#"{"x":null}"#, r#"{"x":null,"y":true}"#),
    (r#"{"k":"v"}"#, "{}"),
    (
        r#"{"nested":{"deep":[true,false]}}"#,
        r#"{"nested":{"deep":[false,true]}}"#,
    ),
    (r#"{"a":1}"#, r#"{"a":1}"#),
    (r#"{"m~n":"x","m/n":"y"}"#, r#"{"m~n":"X","m/n":"Y"}"#),
    (r#"{"u":"x","a":"b"}"#, r#"{"u":"x","a":"b","z":[1]}"#),
    (r#"{"s":"abc"}"#, r#"{"s":"abc","t":null}"#),
    (r#"{"num":10}"#, r#"{"num":10.0}"#),
];

#[test]
fn generate_patches_matches_reference_c() {
    with_lock(|| {
        for (i, (from_str, to_str)) in GENERATE_FIXTURES.iter().enumerate() {
            let from_p = port_parse(from_str.as_bytes());
            let to_p = port_parse(to_str.as_bytes());
            let from_r = ref_parse(from_str.as_bytes());
            let to_r = ref_parse(to_str.as_bytes());
            assert!(
                !from_p.is_null() && !to_p.is_null(),
                "gen test {i}: parse failed"
            );

            // GeneratePatches sorts `from`/`to` in place and returns the patch array.
            let gen_p = unsafe { cjson_utils_generate_patches_case_sensitive(from_p, to_p) };
            let gen_r = unsafe { ref_cJSONUtils_GeneratePatchesCaseSensitive(from_r, to_r) };
            assert!(!gen_p.is_null(), "gen test {i}: port returned NULL");
            assert!(!gen_r.is_null(), "gen test {i}: ref returned NULL");
            assert_eq!(
                port_print(gen_p),
                ref_print(gen_r),
                "gen test {i}: generated patch array mismatch"
            );

            // Applying the generated patch to `from` must yield `to` on both sides.
            let object_p = unsafe { cjson_duplicate(from_p, 1) };
            let object_r = unsafe { ref_cJSON_Duplicate(from_r, 1) };
            assert!(
                !object_p.is_null() && !object_r.is_null(),
                "gen test {i}: dup failed"
            );
            let rc_p = unsafe { cjson_utils_apply_patches_case_sensitive(object_p, gen_p) };
            let rc_r = unsafe { ref_cJSONUtils_ApplyPatchesCaseSensitive(object_r, gen_r) };
            assert_eq!(rc_p, rc_r, "gen test {i}: apply rc mismatch");
            if rc_p == 0 {
                assert_eq!(
                    port_print(object_p),
                    ref_print(object_r),
                    "gen test {i}: applied result mismatch"
                );
            }

            port_free(from_p);
            port_free(to_p);
            port_free(object_p);
            port_free(gen_p);
            ref_free(from_r);
            ref_free(to_r);
            ref_free(object_r);
            ref_free(gen_r);
        }
    });
}

// ---- MergePatch (RFC 7396) against the reference ---------------------------

const MERGE_FIXTURES: [(&str, &str); 14] = [
    (r#"{"a":"b"}"#, r#"{"a":"c"}"#),
    (r#"{"a":"b"}"#, r#"{"b":"c"}"#),
    (r#"{"a":"b"}"#, r#"{"a":null}"#),
    (r#"{"a":"b","b":"c"}"#, r#"{"a":null}"#),
    (r#"{"a":["b"]}"#, r#"{"a":"c"}"#),
    (r#"{"a":"c"}"#, r#"{"a":["b"]}"#),
    (r#"{"a":{"b":"c"}}"#, r#"{"a":{"b":"d","c":null}}"#),
    (r#"{"a":[{"b":"c"}]}"#, r#"{"a":[1]}"#),
    (r#"["a","b"]"#, r#"["c","d"]"#),
    (r#"{"a":"b"}"#, r#"["c"]"#),
    (r#"{"a":"foo"}"#, "null"),
    (r#"{"a":"foo"}"#, r#""bar""#),
    (r#"{"e":null}"#, r#"{"a":1}"#),
    ("{}", r#"{"a":{"bb":{"ccc":null}}}"#),
];

#[test]
fn merge_patch_matches_reference_c() {
    with_lock(|| {
        for (i, (target_str, patch_str)) in MERGE_FIXTURES.iter().enumerate() {
            let target_p = port_parse(target_str.as_bytes());
            let patch_p = port_parse(patch_str.as_bytes());
            let target_r = ref_parse(target_str.as_bytes());
            let patch_r = ref_parse(patch_str.as_bytes());
            assert!(
                !target_p.is_null() && !target_r.is_null(),
                "merge test {i}: parse target failed"
            );
            assert!(
                !patch_p.is_null() && !patch_r.is_null(),
                "merge test {i}: parse patch failed"
            );

            // MergePatch consumes `target` (it may return it directly) but the
            // patch array remains owned by the caller.
            let result_p = unsafe { cjson_utils_merge_patch_case_sensitive(target_p, patch_p) };
            let result_r = unsafe { ref_cJSONUtils_MergePatchCaseSensitive(target_r, patch_r) };
            assert_eq!(
                result_p.is_null(),
                result_r.is_null(),
                "merge test {i}: null mismatch"
            );
            if !result_p.is_null() {
                assert_eq!(
                    port_print(result_p),
                    ref_print(result_r),
                    "merge test {i}: result mismatch"
                );
            }

            port_free(result_p);
            port_free(patch_p);
            ref_free(result_r);
            ref_free(patch_r);
        }
    });
}

// ---- GenerateMergePatch against the reference ------------------------------

#[test]
fn generate_merge_patch_matches_reference_c() {
    with_lock(|| {
        for (i, (from_str, to_str)) in MERGE_FIXTURES.iter().enumerate() {
            let from_p = port_parse(from_str.as_bytes());
            let to_p = port_parse(to_str.as_bytes());
            let from_r = ref_parse(from_str.as_bytes());
            let to_r = ref_parse(to_str.as_bytes());
            assert!(
                !from_p.is_null() && !to_p.is_null(),
                "gen merge test {i}: parse failed"
            );

            let patch_p = unsafe { cjson_utils_generate_merge_patch_case_sensitive(from_p, to_p) };
            let patch_r = unsafe { ref_cJSONUtils_GenerateMergePatchCaseSensitive(from_r, to_r) };
            assert_eq!(
                patch_p.is_null(),
                patch_r.is_null(),
                "gen merge test {i}: null mismatch"
            );
            if !patch_p.is_null() {
                assert_eq!(
                    port_print(patch_p),
                    ref_print(patch_r),
                    "gen merge test {i}: patch mismatch"
                );
            }

            port_free(from_p);
            port_free(to_p);
            port_free(patch_p);
            ref_free(from_r);
            ref_free(to_r);
            ref_free(patch_r);
        }
    });
}

// ---- GetPointer (RFC 6901) against the reference ---------------------------

#[test]
fn get_pointer_matches_reference_c() {
    let json = br#"{
        "foo": ["bar", "baz"],
        "": 0,
        "a/b": 1,
        "c%d": 2,
        "e^f": 3,
        "g|h": 4,
        "i\\j": 5,
        "k\"l": 6,
        " ": 7,
        "m~n": 8,
        "obj": {"deep": {"list": [1, 2, 3]}}
    }"#;
    with_lock(|| {
        let doc_p = port_parse(json);
        let doc_r = ref_parse(json);
        assert!(!doc_p.is_null() && !doc_r.is_null());

        for ptr_str in [
            &b""[..],
            &b"/foo"[..],
            &b"/foo/0"[..],
            &b"/foo/1"[..],
            &b"/"[..],
            &b"/a~1b"[..],
            &b"/c%d"[..],
            &b"/e^f"[..],
            &b"/g|h"[..],
            &b"/i\\j"[..],
            &b"/k\"l"[..],
            &b"/ "[..],
            &b"/m~0n"[..],
            &b"/obj/deep/list/2"[..],
            &b"/obj/deep/list/3"[..],
            &b"/nonexistent"[..],
            &b"/foo/2"[..],
        ] {
            let mut v = ptr_str.to_vec();
            v.push(0);
            let got_p =
                unsafe { cjson_utils_get_pointer_case_sensitive(doc_p, v.as_ptr() as *const u8) };
            let got_r = unsafe {
                ref_cJSONUtils_GetPointerCaseSensitive(doc_r, v.as_ptr() as *const c_char)
            };
            assert_eq!(
                got_p.is_null(),
                got_r.is_null(),
                "GetPointer({}) null mismatch",
                String::from_utf8_lossy(ptr_str)
            );
            if !got_p.is_null() {
                assert_eq!(
                    port_print(got_p),
                    ref_print(got_r),
                    "GetPointer({}) mismatch",
                    String::from_utf8_lossy(ptr_str)
                );
            }
        }

        port_free(doc_p);
        ref_free(doc_r);
    });
}

// ---- SortObject against the reference --------------------------------------

#[test]
fn sort_object_matches_reference_c() {
    let json = br#"{"Q":1,"W":2,"E":3,"R":4,"T":5,"Y":6,"U":7,"I":8,"O":9,"P":10,
                     "A":11,"S":12,"D":13,"F":14,"G":15,"H":16,"J":17,"K":18,"L":19,
                     "Z":20,"X":21,"C":22,"V":23,"B":24,"N":25,"M":26,"nested":{"z":1,"a":2}}"#;
    with_lock(|| {
        let doc_p = port_parse(json);
        let doc_r = ref_parse(json);
        assert!(!doc_p.is_null() && !doc_r.is_null());

        unsafe {
            cjson_utils_sort_object_case_sensitive(doc_p);
            ref_cJSONUtils_SortObjectCaseSensitive(doc_r);
        }
        assert_eq!(
            port_print(doc_p),
            ref_print(doc_r),
            "sorted output mismatch"
        );

        // Sorting must be idempotent on both sides.
        unsafe {
            cjson_utils_sort_object_case_sensitive(doc_p);
            ref_cJSONUtils_SortObjectCaseSensitive(doc_r);
        }
        assert_eq!(
            port_print(doc_p),
            ref_print(doc_r),
            "second sort output mismatch"
        );

        port_free(doc_p);
        ref_free(doc_r);
    });
}

// ---- Differential fuzz -----------------------------------------------------

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

fn random_json_string(rng: &mut Rng) -> String {
    let len = rng.below(8);
    let mut s = String::from('"');
    for _ in 0..len {
        let b = b'a' + (rng.below(26) as u8);
        if b == b'"' || b == b'\\' {
            s.push('\\');
        }
        s.push(b as char);
    }
    s.push('"');
    s
}

fn random_json(rng: &mut Rng, depth: usize) -> String {
    if depth > 4 {
        return match rng.below(3) {
            0 => "null".to_string(),
            1 => "true".to_string(),
            _ => random_json_string(rng),
        };
    }
    match rng.below(6) {
        0 => "null".to_string(),
        1 => "true".to_string(),
        2 => format!("{}", rng.next() as i64 % 1000),
        3 => random_json_string(rng),
        4 => {
            let n = rng.below(4);
            let mut s = '['.to_string();
            for i in 0..n {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&random_json(rng, depth + 1));
            }
            s.push(']');
            s
        }
        5 => {
            let n = rng.below(4);
            let mut s = '{'.to_string();
            for i in 0..n {
                if i > 0 {
                    s.push(',');
                }
                s.push('"');
                s.push((b'a' + i as u8) as char);
                s.push_str("\":");
                s.push_str(&random_json(rng, depth + 1));
            }
            s.push('}');
            s
        }
        _ => unreachable!(),
    }
}

#[test]
fn fuzz_apply_patches_matches_reference_c() {
    let seed = 0xD1FF_3A81u64;
    let mut rng = Rng(seed);
    with_lock(|| {
        for i in 0..600 {
            let doc_json = random_json(&mut rng, 0);
            let patch = match rng.below(4) {
                0 => format!(
                    r#"[{{"op":"add","path":"/fuzz","value":{}}}]"#,
                    random_json(&mut rng, 0)
                ),
                1 => format!(
                    r#"[{{"op":"add","path":"/{}","value":1}}]"#,
                    random_json_string(&mut rng).trim_matches('"')
                ),
                2 => r#"[]"#.to_string(),
                _ => format!(
                    r#"[{{"op":"remove","path":"/{}"}}]"#,
                    random_json_string(&mut rng).trim_matches('"')
                ),
            };

            let doc_p = port_parse(doc_json.as_bytes());
            let doc_r = ref_parse(doc_json.as_bytes());
            let patch_p = port_parse(patch.as_bytes());
            let patch_r = ref_parse(patch.as_bytes());
            if doc_p.is_null() || doc_r.is_null() {
                port_free(doc_p);
                ref_free(doc_r);
                continue;
            }
            if patch_p.is_null() || patch_r.is_null() {
                port_free(doc_p);
                ref_free(doc_r);
                port_free(patch_p);
                ref_free(patch_r);
                continue;
            }

            let rc_p = unsafe { cjson_utils_apply_patches_case_sensitive(doc_p, patch_p) };
            let rc_r = unsafe { ref_cJSONUtils_ApplyPatchesCaseSensitive(doc_r, patch_r) };
            assert_eq!(
                rc_p, rc_r,
                "fuzz {i}: rc mismatch for doc={doc_json}, patch={patch}"
            );
            if rc_p == 0 {
                assert_eq!(
                    port_print(doc_p),
                    ref_print(doc_r),
                    "fuzz {i}: result mismatch for doc={doc_json}, patch={patch}"
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
fn fuzz_merge_patch_matches_reference_c() {
    let seed = 0x4D3A_81B2u64;
    let mut rng = Rng(seed);
    with_lock(|| {
        for i in 0..600 {
            let target_json = random_json(&mut rng, 0);
            let patch_json = random_json(&mut rng, 0);

            let target_p = port_parse(target_json.as_bytes());
            let patch_p = port_parse(patch_json.as_bytes());
            let target_r = ref_parse(target_json.as_bytes());
            let patch_r = ref_parse(patch_json.as_bytes());
            if target_p.is_null() || target_r.is_null() {
                port_free(target_p);
                ref_free(target_r);
                continue;
            }
            if patch_p.is_null() || patch_r.is_null() {
                port_free(target_p);
                ref_free(target_r);
                port_free(patch_p);
                ref_free(patch_r);
                continue;
            }

            let result_p = unsafe { cjson_utils_merge_patch_case_sensitive(target_p, patch_p) };
            let result_r = unsafe { ref_cJSONUtils_MergePatchCaseSensitive(target_r, patch_r) };
            assert_eq!(
                result_p.is_null(),
                result_r.is_null(),
                "fuzz {i}: null mismatch for target={target_json}, patch={patch_json}"
            );
            if !result_p.is_null() {
                assert_eq!(
                    port_print(result_p),
                    ref_print(result_r),
                    "fuzz {i}: result mismatch for target={target_json}, patch={patch_json}"
                );
            }

            port_free(result_p);
            port_free(patch_p);
            ref_free(result_r);
            ref_free(patch_r);
        }
    });
}
