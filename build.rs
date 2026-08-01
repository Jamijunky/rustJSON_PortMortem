use std::env;

fn main() {
    let ref_dir = env::var("CJSON_REF_DIR").unwrap_or_else(|_| {
        let home = env::var("HOME").expect("HOME not set");
        format!("{home}/cjson-ref")
    });
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    // The reference library, compiled from the pristine upstream sources with
    // every public symbol prefixed `ref_` (see bench_ref_rename.h). The port
    // exports the same public names via #[no_mangle], so prefixing avoids any
    // collision in one binary; the differential tests and the benchmark link
    // this archive and always get the real C.
    let rename_h = format!("{manifest}/bench_ref_rename.h");
    cc::Build::new()
        .flag("-include")
        .flag(&rename_h)
        .file(format!("{ref_dir}/cJSON.c"))
        .file(format!("{ref_dir}/cJSON_Utils.c"))
        .include(&ref_dir)
        .warnings(false)
        .compile("cjson_ref_bench");
    println!("cargo:rerun-if-changed={rename_h}");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON.c");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON.h");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON_Utils.c");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON_Utils.h");
}
