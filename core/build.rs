fn main() {
    assert_eq!(std::env::var("CARGO_CFG_TARGET_OS").unwrap(), "windows");
    assert_eq!(std::env::var("CARGO_CFG_TARGET_ARCH").unwrap(), "x86_64");
    cc::Build::new().cpp(true).std("c++20").flag("/EHsc").file("native.cpp").compile("eu4h_native");
    cc::Build::new()
        .include("vendor/minhook/include")
        .files(["vendor/minhook/src/buffer.c", "vendor/minhook/src/hook.c", "vendor/minhook/src/trampoline.c", "vendor/minhook/src/hde/hde64.c"])
        .compile("eu4h_minhook");
    // `dev_build`: any of the development tools is built in (src/dev.rs).
    println!("cargo:rustc-check-cfg=cfg(dev_build)");
    if std::env::vars().any(|(name, _)| name.starts_with("CARGO_FEATURE_DEV_")) {
        println!("cargo:rustc-cfg=dev_build");
    }
    println!("cargo:rerun-if-changed=native.cpp");
    println!("cargo:rerun-if-changed=vendor/minhook");
}
