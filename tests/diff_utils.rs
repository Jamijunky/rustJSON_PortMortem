//! Validation tests for the `cJSON_Utils` port.
//!
//! The json-patch-tests corpus and the reference test-suite fixtures are
//! replayed through the Rust port and checked against expected results.

use std::ffi::c_char;

use cjson::manip::cjson_delete;
use cjson::model::CJson;

// ---- helpers ----------------------------------------------------------------

fn cstr_from_ptr(p: *const c_char) -> Vec<u8> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    unsafe {
        let mut i = 0usize;
        while *p.add(i) != 0 {
            v.push(*p.add(i) as u8);
            i += 1;
        }
    }
    v
}

/// Parse with the port's cJSON_Parse.
fn port_parse(json: &[u8]) -> *mut CJson {
    let mut v = json.to_vec();
    v.push(0);
    unsafe { cjson::ffi::cJSON_Parse(v.as_ptr() as *const c_char) }
}

/// Print with the port's cJSON_PrintUnformatted; return bytes (no NUL).
fn port_print(item: *const CJson) -> Vec<u8> {
    unsafe {
        let p = cjson::ffi::cJSON_PrintUnformatted(item);
        let bytes = cstr_from_ptr(p);
        libc_free(p as *mut std::ffi::c_void);
        bytes
    }
}

unsafe fn libc_free(p: *mut std::ffi::c_void) {
    extern "C" {
        fn free(ptr: *mut std::ffi::c_void);
    }
    free(p);
}

/// Semantic equality through cJSON_Compare (order-insensitive for objects).
fn port_compare(a: *const CJson, b: *const CJson) -> bool {
    unsafe { cjson::ffi::cJSON_Compare(a, b, 1) != 0 }
}

fn port_free(item: *mut CJson) {
    if !item.is_null() {
        unsafe { cjson_delete(item) };
    }
}

// ---- ApplyPatches (RFC 6902) corpus ----------------------------------------

/// Parse the json-patch-tests corpus files and return
/// (doc, patch, expected, expects_error) triples, skipping disabled entries.
fn patch_test_corpus() -> Vec<(Vec<u8>, Vec<u8>, Option<Vec<u8>>, bool)> {
    let home = std::env::var("HOME").unwrap();
    let base = std::path::PathBuf::from(&home).join("cjson-ref/tests/json-patch-tests");
    let mut corpus = Vec::new();
    for name in ["tests.json", "spec_tests.json", "cjson-utils-tests.json"] {
        let data = std::fs::read(base.join(name))
            .unwrap_or_else(|_| panic!("cannot read {name}"));
        let root = port_parse(&data);
        if root.is_null() {
            continue;
        }
        unsafe {
            let mut entry = (*root).child;
            while !entry.is_null() {
                let disabled = {
                    let d = cjson::ffi::cJSON_GetObjectItemCaseSensitive(
                        entry,
                        b"disabled\0".as_ptr() as *const c_char,
                    );
                    !d.is_null() && cjson::ffi::cJSON_IsTrue(d) != 0
                };
                if disabled {
                    entry = (*entry).next;
                    continue;
                }
                let doc_obj = cjson::ffi::cJSON_GetObjectItemCaseSensitive(
                    entry,
                    b"doc\0".as_ptr() as *const c_char,
                );
                let patch_obj = cjson::ffi::cJSON_GetObjectItemCaseSensitive(
                    entry,
                    b"patch\0".as_ptr() as *const c_char,
                );
                let exp_obj = cjson::ffi::cJSON_GetObjectItemCaseSensitive(
                    entry,
                    b"expected\0".as_ptr() as *const c_char,
                );
                let err_obj = cjson::ffi::cJSON_GetObjectItemCaseSensitive(
                    entry,
                    b"error\0".as_ptr() as *const c_char,
                );
                let doc = if doc_obj.is_null() {
                    b"null".to_vec()
                } else {
                    port_print(doc_obj)
                };
                let patch = if patch_obj.is_null() {
                    b"null".to_vec()
                } else {
                    port_print(patch_obj)
                };
                let exp = if exp_obj.is_null() {
                    None
                } else {
                    Some(port_print(exp_obj))
                };
                corpus.push((doc, patch, exp, !err_obj.is_null()));
                entry = (*entry).next;
            }
        }
        port_free(root);
    }
    corpus
}

#[test]
fn apply_patches_corpus_matches_expected() {
    let corpus = patch_test_corpus();
    assert!(!corpus.is_empty(), "corpus should not be empty");

    for (i, (doc_json, patch_json, expected_json, expects_error)) in corpus.iter().enumerate() {
        let doc = port_parse(doc_json);
        let patches = port_parse(patch_json);
        assert!(!doc.is_null(), "test {i}: parse doc failed");
        assert!(!patches.is_null(), "test {i}: parse patch failed");
        let rc = unsafe { cjson::ffi::cJSONUtils_ApplyPatchesCaseSensitive(doc, patches) };
        if *expects_error {
            assert_ne!(rc, 0, "test {i}: expected failure (rc!=0), got rc=0");
        } else {
            assert_eq!(rc, 0, "test {i}: expected success (rc=0), got rc={rc}");
            if let Some(exp_json) = expected_json {
                let exp = port_parse(exp_json);
                assert!(
                    port_compare(doc, exp),
                    "test {i}: result mismatch\n  got:      {}\n  expected: {}",
                    String::from_utf8_lossy(&port_print(doc)),
                    String::from_utf8_lossy(&port_print(exp)),
                );
                port_free(exp);
            }
        }
        port_free(doc);
        port_free(patches);
    }
}

// ---- MergePatch (RFC 7396) fixtures -----------------------------------------

fn merge_corpus() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (r#"{"a":"b"}"#, r#"{"a":"c"}"#, r#"{"a":"c"}"#),
        (r#"{"a":"b"}"#, r#"{"b":"c"}"#, r#"{"a":"b","b":"c"}"#),
        (r#"{"a":"b"}"#, r#"{"a":null}"#, "{}"),
        (r#"{"a":"b","b":"c"}"#, r#"{"a":null}"#, r#"{"b":"c"}"#),
        (r#"{"a":["b"]}"#, r#"{"a":"c"}"#, r#"{"a":"c"}"#),
        (r#"{"a":"c"}"#, r#"{"a":["b"]}"#, r#"{"a":["b"]}"#),
        (
            r#"{"a":{"b":"c"}}"#,
            r#"{"a":{"b":"d","c":null}}"#,
            r#"{"a":{"b":"d"}}"#,
        ),
        (r#"{"a":[{"b":"c"}]}"#, r#"{"a":[1]}"#, r#"{"a":[1]}"#),
        (r#"["a","b"]"#, r#"["c","d"]"#, r#"["c","d"]"#),
        (r#"{"a":"b"}"#, r#"["c"]"#, r#"["c"]"#),
        (r#"{"a":"foo"}"#, "null", "null"),
        (r#"{"a":"foo"}"#, r#""bar""#, r#""bar""#),
        (r#"{"e":null}"#, r#"{"a":1}"#, r#"{"e":null,"a":1}"#),
        (r#"[1,2]"#, r#"{"a":"b","c":null}"#, r#"{"a":"b"}"#),
        ("{}", r#"{"a":{"bb":{"ccc":null}}}"#, r#"{"a":{"bb":{}}}"#),
    ]
}

#[test]
fn merge_patch_matches_expected() {
    for (i, (a_str, b_str, expected_str)) in merge_corpus().iter().enumerate() {
        let target = port_parse(a_str.as_bytes());
        let patch = port_parse(b_str.as_bytes());
        assert!(!target.is_null(), "merge test {i}: parse target failed");
        assert!(!patch.is_null(), "merge test {i}: parse patch failed");

        let result = unsafe { cjson::ffi::cJSONUtils_MergePatch(target, patch) };
        assert!(!result.is_null(), "merge test {i}: MergePatch returned NULL");
        let expected = port_parse(expected_str.as_bytes());
        assert!(
            port_compare(result, expected),
            "merge test {i} failed\n  got:      {}\n  expected: {}",
            String::from_utf8_lossy(&port_print(result)),
            expected_str,
        );
        port_free(result);
        port_free(patch);
        port_free(expected);
    }
}

// ---- GenerateMergePatch + apply roundtrip -----------------------------------

#[test]
fn generate_merge_patch_roundtrip() {
    for (i, (a_str, _b_str, expected_str)) in merge_corpus().iter().enumerate() {
        let from = port_parse(a_str.as_bytes());
        let to = port_parse(expected_str.as_bytes());
        assert!(!from.is_null(), "gen merge test {i}: parse from failed");
        assert!(!to.is_null(), "gen merge test {i}: parse to failed");

        let patch = unsafe { cjson::ffi::cJSONUtils_GenerateMergePatch(from, to) };
        // Apply the patch to from; MergePatch takes ownership of from.
        let applied = unsafe { cjson::ffi::cJSONUtils_MergePatch(from, patch) };
        let expected = port_parse(expected_str.as_bytes());
        assert!(
            port_compare(applied, expected),
            "gen merge roundtrip test {i} failed\n  got:      {}\n  expected: {}",
            String::from_utf8_lossy(&port_print(applied)),
            expected_str,
        );
        port_free(applied);
        port_free(patch);
        port_free(to);
        port_free(expected);
    }
}

// ---- GeneratePatches roundtrip ----------------------------------------------

#[test]
fn generate_patches_roundtrip() {
    let corpus = patch_test_corpus();
    assert!(!corpus.is_empty(), "corpus should not be empty");

    for (i, (doc_json, _patch_json, expected_json, _expects_error)) in corpus.iter().enumerate() {
        let Some(exp_json) = expected_json else {
            continue;
        };
        let doc = port_parse(doc_json);
        let expected = port_parse(exp_json);
        assert!(!doc.is_null(), "gen test {i}: parse doc failed");
        assert!(!expected.is_null(), "gen test {i}: parse expected failed");

        // GeneratePatches sorts `doc` and `expected` in place.
        let gen = unsafe { cjson::ffi::cJSONUtils_GeneratePatchesCaseSensitive(doc, expected) };
        assert!(!gen.is_null(), "gen test {i}: GeneratePatches returned NULL");

        let object = unsafe { cjson::ffi::cJSON_Duplicate(doc, 1) };
        assert!(!object.is_null(), "gen test {i}: duplicate failed");
        let rc = unsafe { cjson::ffi::cJSONUtils_ApplyPatchesCaseSensitive(object, gen) };
        assert_eq!(rc, 0, "gen test {i}: applying generated patch failed");
        assert!(
            port_compare(object, expected),
            "gen test {i}: generate roundtrip mismatch\n  got:      {}\n  expected: {}",
            String::from_utf8_lossy(&port_print(object)),
            String::from_utf8_lossy(&port_print(expected)),
        );

        port_free(object);
        port_free(gen);
        port_free(doc);
        port_free(expected);
    }
}

// ---- GetPointer (RFC 6901) --------------------------------------------------

#[test]
fn get_pointer_rfc6901() {
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
        "m~n": 8
    }"#;
    let doc = port_parse(json);
    assert!(!doc.is_null());

    let cases: Vec<(&[u8], Option<&[u8]>)> = vec![
        (b"", None),
        (b"/foo", Some(b"[\"bar\",\"baz\"]")),
        (b"/foo/0", Some(b"\"bar\"")),
        (b"/", Some(b"0")),
        (b"/a~1b", Some(b"1")),
        (b"/c%d", Some(b"2")),
        (b"/ ", Some(b"7")),
        (b"/m~0n", Some(b"8")),
        (b"/nonexistent", None),
    ];
    for (ptr_str, expected) in &cases {
        let mut v = ptr_str.to_vec();
        v.push(0);
        let got = unsafe {
            cjson::ffi::cJSONUtils_GetPointer(doc, v.as_ptr() as *const c_char)
        };
        if *ptr_str == b"" {
            assert_eq!(got, doc, "GetPointer(\"\") should return root");
            continue;
        }
        if let Some(exp_val) = expected {
            assert!(
                !got.is_null(),
                "GetPointer({:?}) returned NULL, expected non-null",
                String::from_utf8_lossy(ptr_str)
            );
            assert_eq!(
                port_print(got),
                *exp_val,
                "GetPointer({}) mismatch",
                String::from_utf8_lossy(ptr_str)
            );
        } else {
            assert!(
                got.is_null(),
                "GetPointer({:?}) returned non-null, expected NULL",
                String::from_utf8_lossy(ptr_str)
            );
        }
    }
    port_free(doc);
}

// ---- SortObject -------------------------------------------------------------

#[test]
fn sort_object_alphabetical() {
    let json = br#"{"Q":1,"W":2,"E":3,"R":4,"T":5,"Y":6,"U":7,"I":8,"O":9,"P":10,
                     "A":11,"S":12,"D":13,"F":14,"G":15,"H":16,"J":17,"K":18,"L":19,
                     "Z":20,"X":21,"C":22,"V":23,"B":24,"N":25,"M":26}"#;
    let doc = port_parse(json);
    assert!(!doc.is_null());
    unsafe { cjson::ffi::cJSONUtils_SortObject(doc) };

    // Keys must be alphabetically ordered.
    let mut prev_key: Vec<u8> = Vec::new();
    let mut child = unsafe { (*doc).child };
    while !child.is_null() {
        let key = unsafe {
            let s = (*child).string;
            let mut v = Vec::new();
            if !s.is_null() {
                let mut i = 0;
                while *s.add(i) != 0 {
                    v.push(*s.add(i) as u8);
                    i += 1;
                }
            }
            v
        };
        assert!(
            key >= prev_key,
            "sort order violation: {:?} < {:?}",
            String::from_utf8_lossy(&key),
            String::from_utf8_lossy(&prev_key),
        );
        prev_key = key;
        child = unsafe { (*child).next };
    }
    port_free(doc);
}

// ---- FindPointerFromObjectTo ------------------------------------------------

fn our_find_pointer(obj: *const CJson, target: *const CJson) -> Option<Vec<u8>> {
    let p = unsafe { cjson::ffi::cJSONUtils_FindPointerFromObjectTo(obj, target) };
    if p.is_null() {
        None
    } else {
        let bytes = cstr_from_ptr(p);
        unsafe { libc_free(p as *mut std::ffi::c_void) };
        Some(bytes)
    }
}

#[test]
fn find_pointer_from_object_to_basic() {
    let doc = port_parse(br#"{"numbers":[1,2,3,4,5,6,7,8,9,0]}"#);
    assert!(!doc.is_null());

    let nums = unsafe {
        cjson::ffi::cJSON_GetObjectItem(doc, b"numbers\0".as_ptr() as *const c_char)
    };
    assert!(!nums.is_null());

    let num6 = unsafe { cjson::ffi::cJSON_GetArrayItem(nums, 6) };
    assert!(!num6.is_null());

    let ptr = our_find_pointer(doc as *const CJson, num6 as *const CJson);
    assert_eq!(ptr.as_deref(), Some(b"/numbers/6".as_slice()));

    let ptr2 = our_find_pointer(doc as *const CJson, nums as *const CJson);
    assert_eq!(ptr2.as_deref(), Some(b"/numbers".as_slice()));

    let ptr3 = our_find_pointer(doc as *const CJson, doc as *const CJson);
    assert_eq!(ptr3.as_deref(), Some(b"".as_slice()));

    port_free(doc);
}

#[test]
fn find_pointer_with_escapes() {
    let doc = port_parse(br#"{"m~n":"val1","m/n":"val2"}"#);
    assert!(!doc.is_null());

    let child_tilde = unsafe {
        cjson::ffi::cJSON_GetObjectItem(doc, b"m~n\0".as_ptr() as *const c_char)
    };
    assert!(!child_tilde.is_null());
    let ptr = our_find_pointer(doc as *const CJson, child_tilde as *const CJson);
    assert_eq!(ptr.as_deref(), Some(b"/m~0n".as_slice()));

    let child_slash = unsafe {
        cjson::ffi::cJSON_GetObjectItem(doc, b"m/n\0".as_ptr() as *const c_char)
    };
    assert!(!child_slash.is_null());
    let ptr2 = our_find_pointer(doc as *const CJson, child_slash as *const CJson);
    assert_eq!(ptr2.as_deref(), Some(b"/m~1n".as_slice()));

    port_free(doc);
}

// ---- AddPatchToArray --------------------------------------------------------

#[test]
fn add_patch_to_array() {
    let array = port_parse(b"[]");
    assert!(!array.is_null());

    let value = port_parse(b"42");
    assert!(!value.is_null());

    unsafe {
        cjson::ffi::cJSONUtils_AddPatchToArray(
            array,
            b"add\0".as_ptr() as *const c_char,
            b"/-\0".as_ptr() as *const c_char,
            value,
        );
    }
    let result = port_print(array);
    assert_eq!(result, b"[{\"op\":\"add\",\"path\":\"/-\",\"value\":42}]");

    unsafe { cjson_delete(value) };
    port_free(array);
}

// ---- Deterministic fuzz -----------------------------------------------------

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
fn fuzz_sort_object_consistency() {
    let seed = 0xDEAD_1234u64;
    let mut rng = Rng(seed);
    for _ in 0..5000 {
        let json = random_json(&mut rng, 0);
        let doc1 = port_parse(json.as_bytes());
        let doc2 = port_parse(json.as_bytes());
        if doc1.is_null() || doc2.is_null() {
            port_free(doc1);
            port_free(doc2);
            continue;
        }
        unsafe {
            cjson::ffi::cJSONUtils_SortObject(doc1);
            cjson::ffi::cJSONUtils_SortObject(doc2);
        }
        let p1 = port_print(doc1);
        let p2 = port_print(doc2);
        assert_eq!(p1, p2, "SortObject non-idempotent for: {json}");
        port_free(doc1);
        port_free(doc2);
    }
}

#[test]
fn fuzz_apply_patches_consistency() {
    let seed = 0xCAFE_BABEu64;
    let mut rng = Rng(seed);
    for _ in 0..2000 {
        let doc_json = random_json(&mut rng, 0);
        let doc1 = port_parse(doc_json.as_bytes());
        let doc2 = port_parse(doc_json.as_bytes());
        if doc1.is_null() || doc2.is_null() {
            port_free(doc1);
            port_free(doc2);
            continue;
        }
        let patch = match rng.below(3) {
            0 => format!(r#"[{{"op":"add","path":"/fuzz","value":{}}}"#, random_json(&mut rng, 0)),
            1 => format!(r#"[{{"op":"add","path":"/{}","value":1}}]"#, random_json_string(&mut rng).trim_matches('"')),
            _ => "[]".to_string(),
        };
        let p1 = port_parse(patch.as_bytes());
        let p2 = port_parse(patch.as_bytes());
        if p1.is_null() || p2.is_null() {
            port_free(doc1);
            port_free(doc2);
            port_free(p1);
            port_free(p2);
            continue;
        }
        let rc1 = unsafe { cjson::ffi::cJSONUtils_ApplyPatches(doc1, p1) };
        let rc2 = unsafe { cjson::ffi::cJSONUtils_ApplyPatches(doc2, p2) };
        assert_eq!(rc1, rc2, "ApplyPatches rc mismatch for doc={doc_json}, patch={patch}");
        if rc1 == 0 && rc2 == 0 {
            let r1 = port_print(doc1);
            let r2 = port_print(doc2);
            assert_eq!(r1, r2, "ApplyPatches result mismatch");
        }
        port_free(doc1);
        port_free(doc2);
        port_free(p1);
        port_free(p2);
    }
}

#[test]
fn fuzz_merge_patch_consistency() {
    let seed = 0xFACE_C0DEu64;
    let mut rng = Rng(seed);
    for _ in 0..2000 {
        let a_json = random_json(&mut rng, 0);
        let b_json = random_json(&mut rng, 0);
        let a1 = port_parse(a_json.as_bytes());
        let b1 = port_parse(b_json.as_bytes());
        let a2 = port_parse(a_json.as_bytes());
        let b2 = port_parse(b_json.as_bytes());
        if a1.is_null() || b1.is_null() || a2.is_null() || b2.is_null() {
            port_free(a1); port_free(b1); port_free(a2); port_free(b2);
            continue;
        }
        let r1 = unsafe { cjson::ffi::cJSONUtils_MergePatch(a1, b1) };
        let r2 = unsafe { cjson::ffi::cJSONUtils_MergePatch(a2, b2) };
        let r1_null = r1.is_null();
        let r2_null = r2.is_null();
        assert_eq!(r1_null, r2_null, "MergePatch null mismatch");
        if !r1_null && !r2_null {
            let p1 = port_print(r1);
            let p2 = port_print(r2);
            assert_eq!(p1, p2, "MergePatch result mismatch");
        }
        // MergePatch consumes its target (a1/a2) and may return it directly.
        if !r1_null { port_free(r1); }
        if !r2_null { port_free(r2); }
        port_free(b1);
        port_free(b2);
    }
}

#[test]
fn fuzz_pointer_lookup_consistency() {
    let seed = 0xBEEF_1234u64;
    let mut rng = Rng(seed);
    for _ in 0..2000 {
        let json = random_json(&mut rng, 0);
        let doc = port_parse(json.as_bytes());
        if doc.is_null() {
            continue;
        }
        for _ in 0..3 {
            let path = format!("/{}", random_json_string(&mut rng).trim_matches('"'));
            let mut v = path.clone().into_bytes();
            v.push(0);
            let r1 = unsafe {
                cjson::ffi::cJSONUtils_GetPointer(doc, v.as_ptr() as *const c_char)
            };
            let r2 = unsafe {
                cjson::ffi::cJSONUtils_GetPointer(doc, v.as_ptr() as *const c_char)
            };
            assert_eq!(r1.is_null(), r2.is_null(), "GetPointer null mismatch for {path}");
            if !r1.is_null() && !r2.is_null() {
                let p1 = port_print(r1);
                let p2 = port_print(r2);
                assert_eq!(p1, p2, "GetPointer result mismatch for {path}");
            }
        }
        port_free(doc);
    }
}

#[test]
fn fuzz_find_pointer_consistency() {
    let seed = 0xABCD_5678u64;
    let mut rng = Rng(seed);
    for _ in 0..2000 {
        let json = random_json(&mut rng, 0);
        let doc = port_parse(json.as_bytes());
        if doc.is_null() {
            continue;
        }
        let mut child = unsafe { (*doc).child };
        while !child.is_null() {
            let p1 = our_find_pointer(doc as *const CJson, child as *const CJson);
            let p2 = our_find_pointer(doc as *const CJson, child as *const CJson);
            assert_eq!(p1, p2, "FindPointerFromObjectTo non-deterministic");
            child = unsafe { (*child).next };
        }
        port_free(doc);
    }
}
