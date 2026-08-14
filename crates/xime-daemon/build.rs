use std::path::PathBuf;

fn main() {
    // 运行时 rpath 指向子模块构建的 librime。优先级：
    // 1. XIME_LIBRIME_DIST 环境变量（CI/打包场景显式指定）
    // 2. dev 布局兜底：xime-wayland/crates/<name> 的上上级是源码根，
    //    与 Cargo.toml 中 librime 的 path = "../libximecore/crates/librime" 对应
    // 3. 都不可用（如 CI 走 pkg-config 系统 librime）则跳过，不加 rpath
    let dist_lib = std::env::var("XIME_LIBRIME_DIST")
        .ok()
        .or_else(|| std::env::var("RIME_LIB_DIR").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../libximecore/librime/dist/lib")
        });
    println!("cargo:rerun-if-env-changed=XIME_LIBRIME_DIST");
    println!("cargo:rerun-if-env-changed=RIME_LIB_DIR");
    if dist_lib.join("librime.so").exists() || dist_lib.join("librime.dylib").exists() {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            dist_lib.to_string_lossy()
        );
    }
}
