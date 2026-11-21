//! Build script for `nexus-capability`.
//!
//! Probes whether the `wasm32-unknown-unknown` target is installed so the
//! embedded-module tests can be cfg-gated. Mirrors `nexus-wasm-host/build.rs`:
//! the cfg is per-crate and does not propagate to dependent crates, so this
//! crate needs its own probe.

use std::path::PathBuf;
use std::process::Command;

const TARGET: &str = "wasm32-unknown-unknown";

fn main() {
    // Declare the custom cfg flag early so rustc does not warn about
    // `unexpected_cfgs` in the library source.
    println!("cargo::rustc-check-cfg=cfg(nexus_no_wasm_target)");

    if !has_wasm_target() {
        println!(
            "cargo:warning=nexus-capability: {TARGET} target not found; \
             skipping embedded-module tests. Install the target with: \
             rustup target add {TARGET}"
        );
        println!("cargo:rustc-cfg=nexus_no_wasm_target");
    }
}

/// Returns `true` when the compiler's sysroot carries the wasm target's std.
///
/// `rustc --print sysroot --target <t>` exits 0 even for a target that is not
/// installed, so presence is decided by the sysroot's `lib/rustlib/<target>`
/// directory instead.
#[must_use]
fn has_wasm_target() -> bool {
    sysroot().is_some_and(|root| root.join("lib/rustlib").join(TARGET).is_dir())
}

/// The sysroot of the compiler Cargo is actually driving (RUSTC, not PATH).
fn sysroot() -> Option<PathBuf> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let output = Command::new(rustc).args(["--print", "sysroot"]).output().ok()?;
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    (output.status.success() && path.is_dir()).then_some(path)
}
