//! Build script for `nexus-spoke-adapter`.
//!
//! Probes whether the `wasm32-unknown-unknown` target is installed so
//! integration tests that exercise the embedded WASM module can be gated.
//! Mirrors the probe in `nexus-wasm-host/build.rs` — the upstream cfg
//! (`nexus_no_wasm_target`) is per-crate and does not propagate to
//! dependent crates, so this crate needs its own probe.
//!
//! When the target is absent, the `nexus_spoke_adapter_no_wasm_target` cfg
//! is set; the two embedded-module integration tests in
//! `computable_port.rs` are gated behind `#[cfg(not(...))]` accordingly.

fn main() {
    // Declare the custom cfg flag early so rustc does not warn about
    // `unexpected_cfgs` in the library source.
    println!("cargo::rustc-check-cfg=cfg(nexus_spoke_adapter_no_wasm_target)");

    if !has_wasm_target() {
        println!(
            "cargo:warning=nexus-spoke-adapter: wasm32-unknown-unknown target not found; \
             skipping embedded-module integration tests. Install the target with: \
             rustup target add wasm32-unknown-unknown"
        );
        println!("cargo:rustc-cfg=nexus_spoke_adapter_no_wasm_target");
    }
}

/// Returns `true` when the `wasm32-unknown-unknown` sysroot is installed.
///
/// Uses `rustc --print sysroot --target wasm32-unknown-unknown` which exits 0
/// only when the target is available. This is a fast metadata query — it does
/// not compile anything.
#[must_use]
fn has_wasm_target() -> bool {
    std::process::Command::new("rustc")
        .args(["--print", "sysroot", "--target", "wasm32-unknown-unknown"])
        .output()
        .is_ok_and(|o| o.status.success())
}
