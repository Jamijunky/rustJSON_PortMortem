//! Shared deterministic input generators and differential comparison helpers
//! used by `tests/diff_fuzz.rs` and `examples/fuzz_differential.rs`.
//!
//! The generators deliberately push the hard parts of JSON parsing/printing:
//! extreme numbers, escapes and surrogate pairs, multi-byte UTF-8, deep
//! nesting, duplicate keys, and whitespace variants, plus a byte-mutation
//! path that produces malformed input.

use core::ffi::c_char;

use cjson::model::CJson;

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform index in `[0, n)`.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }

    /// Uniform index in `[lo, hi)` (requires `hi > lo`).
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo)
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn digits(rng: &mut Rng, max: usize) -> String {
    let n = 1 + rng.below(max);
    (0..n)
        .map(|_| (b'0' + rng.below(10) as u8) as char)
        .collect()
}

pub fn random_number(rng: &mut Rng) -> String {
    match rng.below(7) {
        0 => format!("{}", rng.below(1_000_000)),
        1 => format!("-{}", rng.below(1_000_000)),
        2 => {
            let mut s = if rng.below(2) == 0 { "-" } else { "" }.to_string();
            s.push_str(&digits(rng, 6));
            s.push('.');
            s.push_str(&digits(rng, 12));
            s
        }
        3 => {
            let mut s = if rng.below(2) == 0 { "-" } else { "" }.to_string();
            s.push_str(&digits(rng, 4));
            s.push('.');
            s.push_str(&digits(rng, 6));
            s.push(if rng.below(2) == 0 { 'e' } else { 'E' });
            s.push(if rng.below(2) == 0 { '-' } else { '+' });
            s.push_str(&format!("{}", rng.below(320)));
            s
        }
        4 => rng
            .pick(&[
                "1e308",
                "1e-308",
                "-1e-300",
                "4.9e-324",
                "5e-324",
                "2.2250738585072014e-308",
                "1.7976931348623157e308",
                "1e999",
                "-1e999",
                "0e0",
                "-0e0",
                "1E+309",
                "-1E-400",
            ])
            .to_string(),
        5 => rng
            .pick(&[
                "0.123456789012345678901234567890",
                "123456789012345678901234567890.123456789",
                "0.0000000000000000000000000000001",
                "123456789012345678901234567890",
            ])
            .to_string(),
        _ => rng
            .pick(&["0", "-0", "0.0", "-0.0", "0e10", "-0.00e-5"])
            .to_string(),
    }
}

pub fn random_json_string(rng: &mut Rng) -> String {
    let mut s = String::from("\"");
    let n = rng.below(10);
    for _ in 0..n {
        match rng.below(9) {
            0 => s.push((b'a' + rng.below(26) as u8) as char),
            1 => s.push((b'0' + rng.below(10) as u8) as char),
            2 => s.push(' '),
            3 => s.push_str(rng.pick(&["\\\"", "\\\\", "\\/", "\\b", "\\f", "\\n", "\\r", "\\t"])),
            4 => {
                s.push_str("\\u");
                for _ in 0..4 {
                    s.push(b"0123456789abcdefABCDEF"[rng.below(22)] as char);
                }
            }
            5 => {
                let high = 0xD800 + rng.below(0x400);
                let low = 0xDC00 + rng.below(0x400);
                s.push_str(&format!("\\u{high:04X}\\u{low:04X}"));
            }
            6 => s.push('é'),
            7 => s.push('中'),
            _ => s.push('\u{1F600}'),
        }
    }
    s.push('"');
    s
}

pub fn ws(rng: &mut Rng) -> &'static str {
    rng.pick(&[" ", "\t", "\n", "\r\n", "  ", "\n\t "])
}

fn random_array(rng: &mut Rng, depth: usize) -> String {
    let n = rng.below(9);
    let mut s = String::from("[");
    for i in 0..n {
        if i > 0 {
            s.push(',');
            if rng.below(6) == 0 {
                s.push_str(ws(rng));
            }
        }
        s.push_str(&random_json(rng, depth + 1));
    }
    s.push(']');
    s
}

const KEYS: &[&str] = &["a", "b", "c", "x", "y", "z", "k0", "k1", "\u{00e9}", "a b"];

fn random_object(rng: &mut Rng, depth: usize) -> String {
    let n = rng.below(9);
    let mut s = String::from("{");
    for i in 0..n {
        if i > 0 {
            s.push(',');
            if rng.below(6) == 0 {
                s.push_str(ws(rng));
            }
        }
        let key = if rng.below(4) == 0 {
            // duplicate an earlier key on purpose
            KEYS[rng.below(KEYS.len())]
        } else {
            KEYS[rng.below(KEYS.len())]
        };
        s.push('"');
        s.push_str(key);
        s.push_str("\":");
        if rng.below(6) == 0 {
            s.push_str(ws(rng));
        }
        s.push_str(&random_json(rng, depth + 1));
    }
    s.push('}');
    s
}

pub fn random_json(rng: &mut Rng, depth: usize) -> String {
    if depth > 9 {
        return match rng.below(5) {
            0 => "null".to_string(),
            1 => "true".to_string(),
            2 => "false".to_string(),
            _ => random_number(rng),
        };
    }
    match rng.below(6) {
        0 => random_number(rng),
        1 => random_json_string(rng),
        2 => "null".to_string(),
        3 => "true".to_string(),
        4 => random_array(rng, depth),
        _ => random_object(rng, depth),
    }
}

/// A well-formed JSON document with optional BOM and surrounding whitespace.
pub fn random_doc(rng: &mut Rng) -> Vec<u8> {
    let mut s = String::new();
    if rng.below(8) == 0 {
        s.push('\u{feff}');
    }
    if rng.below(4) == 0 {
        s.push_str(ws(rng));
    }
    s.push_str(&random_json(rng, 0));
    if rng.below(4) == 0 {
        s.push_str(ws(rng));
    }
    s.into_bytes()
}

/// A document guaranteed to be an array or object at the root (for manip
/// fuzzing).
pub fn random_container_doc(rng: &mut Rng) -> Vec<u8> {
    let mut s = String::new();
    if rng.below(2) == 0 {
        s.push_str(&random_array(rng, 0));
    } else {
        s.push_str(&random_object(rng, 0));
    }
    s.into_bytes()
}

/// Corrupt an input: flip/insert/delete/truncate bytes, or leave as-is.
pub fn mutate_bytes(rng: &mut Rng, input: Vec<u8>) -> Vec<u8> {
    if input.is_empty() {
        return input;
    }
    match rng.below(6) {
        0 => input,
        1 => {
            let mut v = input;
            let idx = rng.below(v.len());
            v[idx] = rng.below(256) as u8;
            v
        }
        2 => {
            let mut v = input;
            v.insert(rng.below(v.len() + 1), rng.below(256) as u8);
            v
        }
        3 if input.len() > 1 => {
            let mut v = input;
            v.remove(rng.below(v.len()));
            v
        }
        4 if input.len() > 2 => {
            let mut v = input;
            v.truncate(rng.range(1, v.len()));
            v
        }
        _ => {
            let mut v = input;
            let idx = rng.below(v.len());
            let end = v.len().min(idx + 1 + rng.below(4));
            v.drain(idx..end);
            v
        }
    }
}

/// A JSON Pointer as a JSON string, e.g. `"/a/b/0"` or `"/-"`.
pub fn random_pointer_json(rng: &mut Rng) -> String {
    let n = 1 + rng.below(4);
    let mut s = String::from("/");
    for i in 0..n {
        if i > 0 {
            s.push('/');
        }
        s.push_str(rng.pick(&["a", "b", "c", "0", "1", "2", "-", "k0", "x"]));
    }
    format!("\"{s}\"")
}

/// A small but structured JSON Patch document (add/remove/replace ops).
pub fn random_patch(rng: &mut Rng) -> String {
    let n = 1 + rng.below(3);
    let mut ops = Vec::new();
    for _ in 0..n {
        let op = rng.below(3);
        let path = random_pointer_json(rng);
        let obj = match op {
            0 => format!(
                r#"{{"op":"add","path":{path},"value":{}}}"#,
                random_json(rng, 0)
            ),
            1 => format!(r#"{{"op":"remove","path":{path}}}"#),
            _ => format!(
                r#"{{"op":"replace","path":{path},"value":{}}}"#,
                random_json(rng, 0)
            ),
        };
        ops.push(obj);
    }
    format!("[{}]", ops.join(","))
}

// ---- comparison helpers ----------------------------------------------------

pub fn cstr_bytes(p: *const c_char) -> Option<Vec<u8>> {
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
pub fn assert_trees_equal(ours: *const CJson, refs: *const CJson, path: &str) {
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
