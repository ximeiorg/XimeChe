use std::env;
use std::path::PathBuf;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let librime_dir = workspace_dir.join("librime");
    let librime_src_dir = librime_dir.join("src");

    let include_dir = librime_src_dir.clone();

    if target_os == "linux" {
        if pkg_config::find_library("rime").is_ok() {
            println!("cargo:warning=Found librime via pkg-config");
        } else {
            let possible_lib_locations = [
                librime_dir.join("build").join("librime.so"),
                librime_dir.join("build").join("lib").join("librime.so"),
                librime_dir.join("dist").join("lib").join("librime.so"),
            ];

            if let Some(lib_path) = possible_lib_locations.iter().find(|p| p.exists()) {
                let lib_dir_found = lib_path.parent().unwrap();
                println!("cargo:rustc-link-search=native={}", lib_dir_found.display());
                println!("cargo:rustc-link-lib=dylib=rime");
                println!("cargo:warning=Found librime at {}", lib_path.display());
            } else {
                println!("cargo:rustc-link-lib=dylib=rime");
            }

            if env::var("RIME_LIB_DIR").is_ok() {
                let lib_dir = PathBuf::from(env::var("RIME_LIB_DIR").unwrap());
                println!("cargo:rustc-link-search=native={}", lib_dir.display());
            }
        }
    } else if target_os == "windows" {
        let dist_dir = librime_dir.join("dist");
        let dist_lib_dir = dist_dir.join("lib");

        if dist_lib_dir.exists() {
            println!("cargo:rustc-link-search=native={}", dist_lib_dir.display());
        }
        println!("cargo:rustc-link-lib=dylib=rime");

        if env::var("RIME_LIB_DIR").is_ok() {
            let lib_dir = PathBuf::from(env::var("RIME_LIB_DIR").unwrap());
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=include/keycodes.h");
    println!("cargo:rerun-if-changed=include/modifiers.h");
    println!("cargo:rerun-if-changed={}", librime_src_dir.join("rime_api.h").display());

    let rime_api_h = include_dir.join("rime_api.h");
    let keycodes_h = manifest_dir.join("include").join("keycodes.h");
    let modifiers_h = manifest_dir.join("include").join("modifiers.h");

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