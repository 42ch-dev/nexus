// contract — docs/specs/architecture/spoke-adapter-architecture.md.
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

#[path = "../nexus-wasm-host/wasm_target_probe.rs"]
mod wasm_target_probe;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(nexus_spoke_adapter_no_wasm_target)");
    println!("cargo:rerun-if-env-changed=RUSTC");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    // `rustup target add` rewrites this directory; without the watch, an
    // "absent" decision cached in an earlier build survives the install and
    // the gated tests stay skipped.
    println!(
        "cargo:rerun-if-changed={}",
        wasm_target_probe::sysroot_of(&rustc)
            .join("lib/rustlib")
            .display()
    );

    // Presence is decided from `sysroot/lib/rustlib/wasm32-unknown-unknown`:
    // `rustc --print sysroot --target <t>` exits 0 even without the target.
    if !wasm_target_probe::wasm_target_installed(&rustc) {
        println!(
            "cargo:warning=nexus-spoke-adapter: wasm32-unknown-unknown target not found; \
             skipping embedded-module integration tests. Install the target with: \
             rustup target add wasm32-unknown-unknown"
        );
        println!("cargo:rustc-cfg=nexus_spoke_adapter_no_wasm_target");
    }
}
