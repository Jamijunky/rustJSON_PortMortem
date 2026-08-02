use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let root = manifest.parent().expect("helper crate must live under repo root");
    let ref_dir = env::var("CJSON_REF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("vendor/cjson-ref"));
    let rename_h = root.join("bench_ref_rename.h");

    cc::Build::new()
        .flag("-include")
        .flag(rename_h.to_str().expect("non-UTF-8 rename header path"))
        .file(ref_dir.join("cJSON.c"))
        .file(ref_dir.join("cJSON_Utils.c"))
        .include(&ref_dir)
        .warnings(false)
        .compile("cjson_ref_bench");

    println!("cargo:rerun-if-changed={}", rename_h.display());
    println!("cargo:rerun-if-changed={}", ref_dir.join("cJSON.c").display());
    println!("cargo:rerun-if-changed={}", ref_dir.join("cJSON.h").display());
    println!("cargo:rerun-if-changed={}", ref_dir.join("cJSON_Utils.c").display());
    println!("cargo:rerun-if-changed={}", ref_dir.join("cJSON_Utils.h").display());
}
