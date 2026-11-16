use std::path::Path;

fn main() {
    // rpath 只能由 bin 包的 build.rs 打（rlib 的 rustc-link-arg 不传播到
    // 最终二进制）。语音运行库（sherpa-onnx）已随 speech 功能移除，现在
    // 只剩一条用途：librime 子模块 dist —— 开发机构建链接子模块产物
    //（系统 librime 无 lua 插件且版本旧，退出析构还会崩）；缺失时跳过。
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let dist_lib = Path::new(&manifest_dir).join("../../../libximecore/librime/dist/lib");
    let dist = if dist_lib.join("librime.so").exists() {
        let canonical = dist_lib.canonicalize().unwrap_or(dist_lib);
        canonical.to_string_lossy().to_string()
    } else {
        String::new()
    };
    if !dist.is_empty() {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dist}");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
