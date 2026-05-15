use std::env;
use std::path::PathBuf;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let include_dir = manifest_dir.join("include");

    if target_os == "linux" {
        if pkg_config::find_library("rime").is_ok() {
            println!("cargo:warning=Found librime via pkg-config");
        } else {
            println!("cargo:rustc-link-lib=dylib=rime");
            if let Ok(lib_dir) = env::var("RIME_LIB_DIR") {
                println!("cargo:rustc-link-search=native={}", lib_dir);
            }
        }
    } else if target_os == "windows" {
        println!("cargo:rustc-link-lib=dylib=rime");
        if let Ok(lib_dir) = env::var("RIME_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", lib_dir);
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=include/rime_api.h");
    println!("cargo:rerun-if-changed=include/keycodes.h");
    println!("cargo:rerun-if-changed=include/modifiers.h");

    let rime_api_h = include_dir.join("rime_api.h");
    let keycodes_h = include_dir.join("keycodes.h");
    let modifiers_h = include_dir.join("modifiers.h");

    let bindings = bindgen::Builder::default()
        .header(rime_api_h.to_string_lossy())
        .header(keycodes_h.to_string_lossy())
        .header(modifiers_h.to_string_lossy())
        .clang_arg(format!("-I{}", include_dir.display()))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}