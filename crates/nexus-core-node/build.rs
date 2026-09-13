fn main() {
    napi_build::setup();
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-unknown-unknown".to_string());
    println!("cargo:rustc-env=NEXUS_BUILD_TARGET={target}");
}
