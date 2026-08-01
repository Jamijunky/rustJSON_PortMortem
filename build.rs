use std::env;

fn main() {
    let ref_dir = env::var("CJSON_REF_DIR").unwrap_or_else(|_| {
        let home = env::var("HOME").expect("HOME not set");
        format!("{home}/cjson-ref")
    });

    cc::Build::new()
        .file(format!("{ref_dir}/cJSON.c"))
        .file(format!("{ref_dir}/cJSON_Utils.c"))
        .include(&ref_dir)
        .warnings(false)
        .compile("cjson_ref");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON.c");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON.h");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON_Utils.c");
    println!("cargo:rerun-if-changed={ref_dir}/cJSON_Utils.h");
}
