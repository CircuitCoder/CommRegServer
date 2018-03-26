extern crate cc;
extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    cc::Build::new()
        .file("impl/jieba.cpp")
        .cpp(true)
        .flag_if_supported("-std=c++14")
        .define("LOGGING_LEVEL", "LL_WARNING")
        .include("lib/include")
        .include("lib/deps")
        .out_dir(out_path.clone())
        .compile("libjieba.a");

    println!("cargo:rustc-link-search=native={}", out_path.to_str().unwrap());
    println!("cargo:rustc-link-lib=static=jieba");

    let bindings = bindgen::Builder::default()
        .header("impl/jieba.h")
        .generate()
        .expect("Unable to generate bindings");
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
