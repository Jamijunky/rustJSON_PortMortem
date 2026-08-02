//! Independent-oracle cross-check: parse each input document with the Rust
//! port and with `serde_json` (a completely independent JSON implementation)
//! and compare structurally.
//!
//! A document is only structurally compared when both sides accept it as a
//! single strict JSON value (whole input consumed). Documents are otherwise
//! tallied into report-only buckets:
//!
//! * `port_only`   — the port accepts but serde_json rejects. Expected for
//!   cJSON's documented leniencies (number overflow to +/-inf, lone surrogate
//!   escapes, non-UTF-8 string bytes, duplicate keys), so this is informational.
//! * `close`       — both parse and both numbers differ by less than ~2 ULP.
//!   Informational.
//! * `skipped_dup` — both parse but the object has duplicate keys, which
//!   serde_json cannot represent; skipped.
//!
//! Failures (exit 1):
//! * `port_rejects_valid` — serde_json accepts a strict JSON document that the
//!   port rejects. That would be a genuine port bug.
//! * `mismatch` — both parse, no duplicate keys, and the trees differ
//!   structurally beyond the number tolerance.
//!
//! Usage: `oracle_check [--gen N] FILE...` reads each FILE as one JSON
//! document; with `--gen N` additionally generates and checks N random
//! documents (depth <= 32, no duplicate keys, occasionally extreme exponents).

use std::collections::HashSet;
use std::ffi::c_char;
use std::fs;
use std::process::ExitCode;

use serde::Deserialize;
use serde_json::Value;

use cjson::manip::cjson_delete;
use cjson::model::{
    CJson, CJSON_ARRAY, CJSON_FALSE, CJSON_NULL, CJSON_NUMBER, CJSON_OBJECT, CJSON_STRING,
    CJSON_TRUE,
};
use cjson::parse::cjson_parse_with_opts;

// ---- tally -----------------------------------------------------------------

#[derive(Default)]
struct Tally {
    compared: u64,
    close: u64,
    port_only: u64,
    port_rejects_valid: u64,
    mismatch: u64,
    skipped_dup: u64,
    failures: Vec<String>,
    port_only_samples: Vec<String>,
}

// ---- port tree -> structural value ----------------------------------------

enum PortNode {
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u8>),
    Array(Vec<PortNode>),
    Object(Vec<(String, PortNode)>),
}

unsafe fn port_walk(item: *const CJson, depth: usize) -> Option<PortNode> {
    if item.is_null() || depth > 4096 {
        return None;
    }
    let t = (*item).type_ & 0xFF;
    match t {
        CJSON_NULL => Some(PortNode::Null),
        CJSON_TRUE => Some(PortNode::Bool(true)),
        CJSON_FALSE => Some(PortNode::Bool(false)),
        CJSON_NUMBER => Some(PortNode::Number((*item).valuedouble)),
        CJSON_STRING => {
            if (*item).valuestring.is_null() {
                None
            } else {
                Some(PortNode::String(cstr_bytes((*item).valuestring)))
            }
        }
        CJSON_ARRAY => {
            let mut out = Vec::new();
            let mut c = (*item).child;
            while !c.is_null() {
                out.push(port_walk(c, depth + 1)?);
                c = (*c).next;
            }
            Some(PortNode::Array(out))
        }
        CJSON_OBJECT => {
            let mut out = Vec::new();
            let mut c = (*item).child;
            while !c.is_null() {
                if (*c).string.is_null() {
                    return None;
                }
                out.push((cstr_string((*c).string), port_walk(c, depth + 1)?));
                c = (*c).next;
            }
            Some(PortNode::Object(out))
        }
        _ => None, // CJSON_RAW and unknown types are not produced by parsing
    }
}

unsafe fn cstr_bytes(p: *mut c_char) -> Vec<u8> {
    let mut v = Vec::new();
    let mut i = 0usize;
    while *p.add(i) != 0 {
        v.push(*p.add(i) as u8);
        i += 1;
    }
    v
}

unsafe fn cstr_string(p: *mut c_char) -> String {
    String::from_utf8_lossy(&cstr_bytes(p)).into_owned()
}

// ---- comparison ------------------------------------------------------------

enum Verdict {
    Match,
    Close,
    Mismatch,
    SkipDup,
}

fn compare_number(a: f64, b: f64) -> Verdict {
    if a.is_nan() || b.is_nan() {
        return if a.is_nan() && b.is_nan() {
            Verdict::Match
        } else {
            Verdict::Mismatch
        };
    }
    if a == b {
        return Verdict::Match;
    }
    if a.is_finite() && b.is_finite() {
        let scale = a.abs().max(b.abs()).max(1.0);
        if (a - b).abs() <= f64::EPSILON * scale * 4.0 {
            return Verdict::Close;
        }
    }
    Verdict::Mismatch
}

fn compare(a: &PortNode, b: &Value, tally: &mut Tally) -> Verdict {
    match (a, b) {
        (PortNode::Null, Value::Null) => Verdict::Match,
        (PortNode::Bool(x), Value::Bool(y)) => {
            if x == y {
                Verdict::Match
            } else {
                Verdict::Mismatch
            }
        }
        (PortNode::Number(x), Value::Number(y)) => {
            let Some(yf) = y.as_f64() else {
                return Verdict::Mismatch;
            };
            compare_number(*x, yf)
        }
        (PortNode::String(x), Value::String(y)) => {
            let Ok(ys) = std::str::from_utf8(x) else {
                return Verdict::Mismatch;
            };
            if ys == y {
                Verdict::Match
            } else {
                Verdict::Mismatch
            }
        }
        (PortNode::Array(xs), Value::Array(ys)) => {
            if xs.len() != ys.len() {
                return Verdict::Mismatch;
            }
            for (x, y) in xs.iter().zip(ys.iter()) {
                if matches!(compare(x, y, tally), Verdict::Mismatch) {
                    return Verdict::Mismatch;
                }
            }
            Verdict::Match
        }
        (PortNode::Object(entries), Value::Object(map)) => {
            // duplicate keys would make key-based lookup ambiguous; skip
            let mut seen = HashSet::new();
            for (k, _) in entries {
                if !seen.insert(k.clone()) {
                    return Verdict::SkipDup;
                }
            }
            if entries.len() != map.len() {
                return Verdict::Mismatch;
            }
            for (k, x) in entries {
                let Some(y) = map.get(k) else {
                    return Verdict::Mismatch;
                };
                if matches!(compare(x, y, tally), Verdict::Mismatch) {
                    return Verdict::Mismatch;
                }
            }
            Verdict::Match
        }
        _ => Verdict::Mismatch,
    }
}

// ---- check one document -----------------------------------------------------

fn check_doc(name: &str, doc: &[u8], tally: &mut Tally) {
    let mut buf = doc.to_vec();
    buf.push(0);
    let mut parse_end: *const c_char = std::ptr::null();
    let port = unsafe { cjson_parse_with_opts(buf.as_ptr() as *const c_char, &mut parse_end, 1) };

    let mut de = serde_json::Deserializer::from_slice(doc);
    de.disable_recursion_limit();
    let oracle = Value::deserialize(&mut de).and_then(|v| {
        de.end()?;
        Ok(v)
    });

    match (port.is_null(), oracle) {
        (true, Err(_)) => return, // both reject
        (true, Ok(_)) => {
            tally.port_rejects_valid += 1;
            tally.failures.push(format!(
                "{name}: port rejected strict JSON that serde_json accepts: {}",
                String::from_utf8_lossy(doc)
            ));
        }
        (false, Err(_)) => {
            tally.port_only += 1;
            if tally.port_only_samples.len() < 8 {
                tally
                    .port_only_samples
                    .push(format!("{}: {}", name, String::from_utf8_lossy(doc)));
            }
        }
        (false, Ok(v)) => {
            let node = unsafe { port_walk(port, 0) };
            match node {
                None => {
                    tally.port_only += 1;
                }
                Some(n) => match compare(&n, &v, tally) {
                    Verdict::Match => tally.compared += 1,
                    Verdict::Close => {
                        tally.close += 1;
                        tally.compared += 1;
                    }
                    Verdict::Mismatch => {
                        tally.mismatch += 1;
                        tally
                            .failures
                            .push(format!("{name}: structural mismatch with serde_json"));
                    }
                    Verdict::SkipDup => {
                        tally.skipped_dup += 1;
                    }
                },
            }
        }
    }

    if !port.is_null() {
        unsafe { cjson_delete(port) };
    }
}

// ---- random document generator (independent of tests/common) ----------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const NUMBER_BANK: &[&str] = &[
    "0",
    "-1",
    "3.141592653589793",
    "1e308",
    "5e-324",
    "1e-308",
    "0.1",
    "-0.0",
    "1.0e20",
    "9007199254740993",
    "-9007199254740993",
    "123456789012345678901234567890",
    "4.9406564584124654e-324",
    "2.2250738585072014e-308",
];
const STRING_BANK: &[&str] = &[
    "",
    "hello",
    "a\"b\\c",
    "tab\there",
    "newline\nhere",
    "uni \u{00e9} \u{1f642}",
    "\u{00df}\u{03a9}\u{4e2d}\u{6587}",
    "esc\\/\\b\\f\\r\\u0041",
];

fn gen_doc(rng: &mut Rng, depth: u64) -> String {
    let r = rng.below(10);
    if depth == 0 || r < 5 {
        match rng.below(4) {
            0 => format!("{}", rng.below(2_000_000_000_000_000_000) as i64),
            1 => NUMBER_BANK[rng.below(NUMBER_BANK.len() as u64) as usize].to_string(),
            2 => format!(
                "\"{}\"",
                STRING_BANK[rng.below(STRING_BANK.len() as u64) as usize]
            ),
            _ => match rng.below(3) {
                0 => "true".to_string(),
                1 => "false".to_string(),
                _ => "null".to_string(),
            },
        }
    } else if r == 5 {
        let n = rng.below(6);
        let elems: Vec<String> = (0..n).map(|_| gen_doc(rng, depth - 1)).collect();
        format!("[{}]", elems.join(","))
    } else {
        let n = rng.below(6);
        let elems: Vec<String> = (0..n)
            .map(|_| {
                let key = STRING_BANK[rng.below(STRING_BANK.len() as u64) as usize];
                format!("\"{}\":{}", key, gen_doc(rng, depth - 1))
            })
            .collect();
        format!("{{{}}}", elems.join(","))
    }
}

// ---- main -------------------------------------------------------------------

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut gen = 0u64;
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--gen" {
            i += 1;
            gen = args[i].parse().expect("--gen N");
        } else {
            files.push(args[i].clone());
        }
        i += 1;
    }

    let mut tally = Tally::default();

    for f in &files {
        let Ok(doc) = fs::read(f) else {
            eprintln!("skipping unreadable file: {f}");
            continue;
        };
        check_doc(f, &doc, &mut tally);
    }

    if gen > 0 {
        let mut rng = Rng(0x8BAD_F00D_DEAD_BEEF);
        for k in 0..gen {
            let doc = gen_doc(&mut rng, 32).into_bytes();
            let name = format!("generated[{k}]");
            check_doc(&name, &doc, &mut tally);
        }
    }

    let total = files.len() as u64 + gen;
    println!("oracle_check: {total} documents");
    println!("  compared         : {}", tally.compared);
    println!("  close (report)   : {}", tally.close);
    println!("  port-only (report): {}", tally.port_only);
    println!("  duplicate-key skips: {}", tally.skipped_dup);
    println!("  port-rejects-valid: {}", tally.port_rejects_valid);
    println!("  structural mismatch: {}", tally.mismatch);
    if !tally.port_only_samples.is_empty() {
        println!("  port-only samples:");
        for s in &tally.port_only_samples {
            println!("    {s}");
        }
    }
    for f in &tally.failures {
        eprintln!("FAIL: {f}");
    }

    if tally.failures.is_empty() {
        println!("oracle_check: PASS");
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
